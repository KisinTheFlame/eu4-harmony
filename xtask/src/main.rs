//! `cargo xtask`: the repository's development commands, for development
//! machines only. They need the repository: its builds in `target\`, its
//! `dev.toml`, its sources. None of them is among the player's files, which
//! `release` packs; what is useful without the repository is a command of
//! the command line tool, `cli/`.
mod claim;
mod crash_dump;
mod files;
mod game;
mod profile;
mod scenario;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;

const USAGE: &str = "\
cargo xtask: the repository's development commands

usage:
  cargo xtask game-dir
      Prints the game folder: EU4_GAME_DIR when it is set, or else the Steam
      library that holds Europa Universalis IV. Every command that needs the
      game finds it so; game, install and scenario take --game-dir for
      another.
  cargo xtask game claim --purpose <text> [--wait] [--lease <minutes>] [--owner <name>]
  cargo xtask game claim --player [--now]
  cargo xtask game start [--visible] [--sound] [--timeout <seconds>]
  cargo xtask game stop | release [--keep-crashes] | renew | watch [--warn 5] | status
      The claim: one holder of this computer's game at a time
      (docs/game-testing.md). claim backs up the player's files and copies
      the game folder's framework files and dev.toml into the claim's folder,
      which it prints; busy, it exits with 2, or with --wait joins the queue
      and returns once it holds the game. start starts the game hidden and
      without sound and waits for its main menu, ending a start that does
      not get there; --visible shows its window there, --sound keeps its
      sound. stop ends it. release ends the game, keeps the logs and the run's crash
      folders in the claim's kept\\, puts back the player's files and the
      game folder's, and frees the game. watch, run in the background by the
      holder, exits with 3 when the lease (30 minutes unless --lease says
      otherwise, kept going while the game runs) is --warn minutes from its
      end, with 4 when the player takes the game, and
      with 0 when the claim is gone. The player's claim (--player) keeps
      their files as they play and goes first in the queue; with --now it
      takes the game from its holder after two minutes. The owner is this
      checkout's folder's name unless --owner names another.
  cargo xtask install [--uninstall] [--game-dir <dir>] [--owner <name>]
      Within the claim, copies the player's files as last built, a player build or a developer
      build, into the game folder: version.dll in its root, the core and
      eu4-harmony.exe in eu4-harmony\\. A version.dll there that is not ours
      is moved to eu4-harmony\\backup\\ and never loaded; the repository's
      dev.toml goes into eu4-harmony\\ too, and one installed before is
      removed when the repository has none. --uninstall removes them all and
      puts a moved version.dll back.
  cargo xtask release [--out <dir>]
      Builds the player build and packs the player's files for players:
      <dir> (target\\eu4-harmony by default) and <dir>.zip, whose contents go
      into the game's root folder. Both are made anew.
  cargo xtask disasm <arg>...
      Reads the installed game's code (core/examples/disasm.rs, whose
      arguments docs/dev-tools.md describes), with EU4_GAME_DIR set to the
      game folder.
  cargo xtask offline-check
      Compares every dependency point's bytes with the game's eu4.exe, and
      checks that each observer forwards at least the arguments its function
      reads: the core's ignored tests, with EU4_GAME_DIR set.
  cargo xtask scenario
  cargo xtask scenario <name>... [--out <dir>] [--wait] [--owner <name>]
                       [--game-dir <dir>] [--<name>.<parameter> <value>]...
  cargo xtask scenario --from-report <dir>
      Plays scenarios in the game, driven through remote control, and
      writes report.json and report.md into <dir> (target\\scenarios\\<time>
      by default): how each ended and what it found (docs/scenarios.md).
      With no name it lists them, with their builds and parameters. It
      needs Steam running, claims the game unless this checkout holds it
      (busy, it waits in the queue with --wait), builds and installs each
      build the scenarios need, starts the game hidden, and releases the
      claim it made. --<name>.<parameter> changes a parameter for this run.
      --from-report writes report.md again from the report.json there.
  cargo xtask profile <out> [--seconds 60] [--hz 50] [--slice <seconds>] [--wait] [--thread <name>|busiest]
  cargo xtask profile <out>.folded --tree [--root <frame>] [--depth 6] [--min 1]
      Samples the running game's main thread, its call stack at a steady
      rate with Windows' own dbghelp.dll, and writes where its time went into
      <out>.txt, and its stacks into <out>.folded for flame graph tools:
      inside days, in frames inside days and between days, by part and by
      function. Each sample slows the game, about 7% at 50 a second, so
      measure shares at 50. --slice counts the samples of each stretch of
      that many seconds apart too, and --wait waits for eu4.exe to start:
      for the game's startup or a load. --thread samples the thread the game
      named so while it runs, busiest the busiest of each second, or 0x...
      a thread with that function on its stack, instead of the main thread. --tree prints a .folded file's stacks
      as a call tree, or the tree below one frame, such as a stretch's.
  cargo xtask crash-dump <dump> <log> [--symbols <dir>] <command>...
      Runs debugger commands on a crash dump with Windows' own dbgeng.dll,
      writing their output to <log>, such as !analyze -v, .ecxr, kn 30, r,
      u @rip-0x20 L20 and lmvm eu4_harmony_core. Symbols come from this
      checkout's release build and Microsoft's symbol server, cached in
      --symbols (C:\\symbols by default).
";

/// Positional arguments and `--key value` / `--flag` options.
#[derive(Debug)]
struct Args {
    positional: Vec<String>,
    options: HashMap<String, Option<String>>,
}

impl Args {
    /// Reads `raw`, where `flags` take no value and the options in `values`
    /// take one; any other option is refused.
    fn parse(raw: &[String], flags: &[&str], values: &[&str]) -> Result<Self> {
        let mut args = Args { positional: Vec::new(), options: HashMap::new() };
        let mut i = 0;
        while i < raw.len() {
            if let Some(key) = raw[i].strip_prefix("--") {
                if flags.contains(&key) {
                    args.options.insert(key.to_string(), None);
                } else if values.contains(&key) {
                    let value = raw.get(i + 1).ok_or_else(|| format!("--{key} needs a value"))?;
                    args.options.insert(key.to_string(), Some(value.clone()));
                    i += 1;
                } else {
                    return Err(format!("unknown option --{key}; cargo xtask --help lists them"));
                }
            } else {
                args.positional.push(raw[i].clone());
            }
            i += 1;
        }
        Ok(args)
    }

    fn value(&self, key: &str) -> Option<&str> {
        self.options.get(key).and_then(|v| v.as_deref())
    }

    fn flag(&self, key: &str) -> bool {
        self.options.contains_key(key)
    }

    /// The whole number `--key` gives, or `default`.
    fn number(&self, key: &str, default: u32) -> Result<u32> {
        self.value(key).map_or(Ok(default), |v| v.parse().map_err(|_| format!("--{key} takes a whole number; {v:?} is not one")))
    }

    /// `--game-dir`, else the game folder `game::dir` finds.
    fn game_dir(&self) -> Result<PathBuf> {
        self.value("game-dir").map_or_else(game::dir, |d| Ok(PathBuf::from(d)))
    }
}

/// The repository's root, which holds this crate.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask sits in the repository").to_path_buf()
}

/// Where `cargo build --release` puts what it builds, for the target
/// `.cargo/config.toml` names.
fn built() -> PathBuf {
    repo().join("target").join("x86_64-pc-windows-msvc").join("release")
}

/// Cargo, run from the repository: the one running this command.
fn cargo() -> std::process::Command {
    let mut cargo = std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cargo.current_dir(repo());
    cargo
}

/// Runs `command` to its end; an error unless it succeeds.
fn run(mut command: std::process::Command, what: &str) -> Result<()> {
    let status = command.status().map_err(|e| format!("cannot run {what}: {e}"))?;
    status.success().then_some(()).ok_or_else(|| format!("{what} failed"))
}

/// `cargo xtask disasm` and `offline-check`: the core's example and ignored
/// tests, with the game found.
fn with_game(arguments: &[&str], rest: &[String], what: &str) -> Result<()> {
    let mut cargo = cargo();
    cargo.args(arguments).args(rest).env("EU4_GAME_DIR", game::dir()?);
    run(cargo, what)
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let rest = raw.get(1..).unwrap_or_default();
    let result = match raw.first().map(String::as_str) {
        Some("game-dir") => game::dir().map(|dir| println!("{}", dir.display())),
        Some("install") => Args::parse(rest, &["uninstall"], &["game-dir", "owner"]).and_then(|a| files::install(&a)),
        Some("game") => match claim::run(rest) {
            Ok(code) => std::process::exit(code),
            Err(e) => Err(e),
        },
        Some("release") => Args::parse(rest, &[], &["out"]).and_then(|a| files::release(&a)),
        Some("disasm") => with_game(
            &["run", "--release", "--quiet", "-p", "eu4-harmony-core", "--features", "analysis", "--example", "disasm", "--"],
            rest,
            "the disassembler",
        ),
        Some("offline-check") => with_game(&["test", "--release", "-p", "eu4-harmony-core", "--", "--ignored"], rest, "the offline checks"),
        Some("scenario") => scenario::run(rest),
        Some("profile") => Args::parse(rest, &["tree", "wait"], &["seconds", "hz", "slice", "thread", "root", "depth", "min"]).and_then(|a| profile::run(&a)),
        // Debugger commands are words of their own, which may start with --.
        Some("crash-dump") => crash_dump::run(rest),
        None | Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn args_take_flags_values_and_positionals_and_refuse_the_rest() {
        let a = Args::parse(&words("--out D:\\r a b --keep-game --rounds 3"), &["keep-game"], &["out", "rounds"]).unwrap();
        assert_eq!(a.positional, ["a", "b"]);
        assert_eq!(a.value("out"), Some("D:\\r"));
        assert!(a.flag("keep-game") && !a.flag("from-report"));
        assert_eq!(a.number("rounds", 10), Ok(3));
        assert_eq!(a.number("days", 30), Ok(30));
        assert!(Args::parse(&words("--rounds"), &[], &["rounds"]).unwrap_err().contains("needs a value"));
        assert!(Args::parse(&words("--round 3"), &[], &["rounds"]).unwrap_err().contains("unknown option --round"));
        assert!(Args::parse(&words("--rounds x"), &[], &["rounds"]).unwrap().number("rounds", 1).unwrap_err().contains("whole number"));
    }
}
