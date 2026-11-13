//! `cargo xtask`: the repository's development commands, for development
//! machines only. They need the repository: its builds in `target\`, its
//! `dev.toml`, its sources. None of them is among the player's files, which
//! `release` packs; what is useful without the repository is a command of
//! the command line tool, `cli/`.
mod addresses;
mod claim;
mod crash_dump;
mod files;
mod game;
mod profile;
mod release;
mod scenario;
mod workshop;

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
  cargo xtask game claim --owner <name> --purpose <text>
  cargo xtask game start --owner <name> [--save <name>]... [--visible] [--sound] [--seed <n>] [--timeout <seconds>]
  cargo xtask game stop | release --owner <name>
  cargo xtask game status
      Claims of test games: games started from the game folder with a user
      folder, a core and a pipe of their own, any number at once
      (docs/plans/test-games.md). claim makes the claim's folder, with a user
      folder made from the player's, and prints it. start copies this
      checkout's built core, which needs --features
      eu4-harmony-core/dev-test-games, and dev.toml into the test game, and
      each --save from the player's saves, then starts it hidden
      and without sound and waits for its main menu, ending a start that
      does not get there; --visible shows its window there, --sound keeps
      its sound, --seed passes the game its own -seed=<n>. Its pipe is the
      owner's: eu4-harmony game --test-game <owner>, or
      EU4_HARMONY_TEST_GAME. stop ends it. release ends it, keeps its logs
      and crash folders in the claim's kept\\ and removes the rest; a claim
      holds its test game until then. status lists the test games and the
      eu4.exe no test game's. --owner names the test game and its pipe;
      it has no default, so that no two runs share a test game.
  cargo xtask install [--uninstall] [--game-dir <dir>]
      With no game running, copies the files the game loads as last built, a
      player build or a developer build, into the game folder: version.dll in
      its root, the core in eu4-harmony\\. The command line tool is not among
      them: run the one in target\\. A version.dll there that is not ours
      is moved to eu4-harmony\\backup\\ and never loaded; the repository's
      dev.toml goes into eu4-harmony\\ too, and one installed before is
      removed when the repository has none. --uninstall removes them all and
      puts a moved version.dll back.
  cargo xtask release-check [--tag vX.Y.Z] [--out <dir>]
      Checks clean source merged into master, matching workspace and lockfile
      versions, and the version's change note. --tag also requires that local
      and remote tag to name HEAD. Writes release.json under <dir>
      (target\\dist by default); without --tag checks before tag creation.
  cargo xtask release [--out <dir>] [--no-workshop]
      Builds the optimized player build and packs two versioned Windows x64
      ZIPs under <dir> (target\\dist by default): the framework's proxy and
      core, and the separate command line tool. Checks each archive's files
      against the build. Packs Harmony's Workshop item and renders its
      comparison images too, unless --no-workshop is given. GitHub publication
      uses --no-workshop and needs neither the game nor Steam.
  cargo xtask workshop-upload --user <steam login> [--create]
      Uploads target\\workshop\\harmony\\ to Harmony's Workshop item with
      SteamCMD (STEAMCMD, else on PATH), which asks for the password itself,
      with the version's section of CHANGELOG.md as the change note and no
      description. It needs clean source merged into master, the existing
      local and remote vX.Y.Z tag at HEAD, and the item packed from that
      commit. A detached checkout is supported; it never creates a tag. --create makes the item, private, and records its id in
      workshop\\item.toml; commit that, release and upload again, so that the
      proxy is built with it (docs/dev-tools.md).
  cargo xtask disasm <arg>...
      Reads the installed game's code (core/examples/disasm.rs, whose
      arguments docs/dev-tools.md describes), with EU4_GAME_DIR set to the
      game folder.
  cargo xtask addresses
      Writes docs/research/addresses.toml again from the research notes and
      the game's pages: every address of eu4.exe they name, with the notes
      that name it, each name kept, and its kind (a function begins there,
      code inside one, data) from the game's eu4.exe when it is found
      (docs/plans/address-index.md).
  cargo xtask what <rva>
      The index's entry of an address, and of the function it is in, and each
      line of the notes, the game's pages and the code that names either.
  cargo xtask offline-check
      Compares every dependency point's bytes with the game's eu4.exe, and
      checks that each observer forwards at least the arguments its function
      reads: the core's ignored tests, with EU4_GAME_DIR set, in a player's
      build and then in a developer build with every tool, whose points
      only it declares.
  cargo xtask scenario
  cargo xtask scenario <name>... --owner <name> [--out <dir>] [--jobs <n>]
                       [--game-dir <dir>] [--<name>.<parameter> <value>]...
  cargo xtask scenario --from-report <dir>
      Plays scenarios in the game, driven through remote control, and
      writes report.json and report.md into <dir> (target\\scenarios\\<time>
      by default): how each ended and what it found (docs/plans/scenarios.md).
      With no name it lists them, with their builds and parameters. It
      needs Steam running, builds each build the scenarios need with the
      test games' pipes, set aside in the report's folder, and plays each
      scenario in a test game of its own, claimed as
      <owner>-<scenario>-<six letters and digits>, started hidden with its
      build and released after it; the report is written again as each
      scenario ends. --<name>.<parameter> changes a parameter for this run.
      --jobs <n> plays n scenarios at a time, at most the 4 test games that
      run at once (whole and release: 4 unless given; else 1), the longest
      first; its builds made first and set aside in the report's
      folder; perf, which measures what the game costs the computer, plays
      alone after the rest, once other games have ended or 20 minutes passed.
      --from-report writes report.md again from the report.json there.
  cargo xtask profile <out> [--seconds 60] [--hz 50] [--slice <seconds>] [--wait] [--threads] [--waiting-in <functions>] [--test-game <name>]
  cargo xtask profile <out>.folded --tree [--root <frame>] [--depth 6] [--min 1]
      Samples the running game's main thread (the test game --test-game
      names, else the only eu4.exe), its call stack
      at a steady rate, and writes where its time went into <out>.txt, and its stacks
      into <out>.folded for flame graph tools: inside days, in frames inside
      days and between days, by part and by function. A sample holds the
      thread about 0.1 ms, for its registers and a copy of its stack.
      --threads samples every thread that runs at each tick too, into
      <out>.threads.folded (a root frame per thread) and <out>.game.folded
      (the threads merged), counting a thread only while it runs, and
      <out>.waited.folded: what the other threads ran while the main thread
      waited, each tick's wait shared out among them, in microseconds. The
      main thread waits asleep in the kernel, or in a function that
      --waiting-in names as the stacks do (0x179e90,ntdll.dll+0x1234). --slice
      counts the samples of each stretch of that many seconds apart too, and
      --wait waits for eu4.exe to start: for the game's startup or a load.
      --tree prints a .folded file's stacks as a call tree, or the tree
      below one frame, such as a stretch's.
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
    /// The values of the options that may be given more than once, in order.
    repeated: Vec<(String, String)>,
}

impl Args {
    /// Reads `raw`, where `flags` take no value and the options in `values`
    /// take one; any other option is refused.
    fn parse(raw: &[String], flags: &[&str], values: &[&str]) -> Result<Self> {
        Self::parse_repeated(raw, flags, values, &[])
    }

    /// Reads `raw` as [`Args::parse`] does, the options in `repeated` taking
    /// a value each time they are given.
    fn parse_repeated(raw: &[String], flags: &[&str], values: &[&str], repeated: &[&str]) -> Result<Self> {
        let mut args = Args { positional: Vec::new(), options: HashMap::new(), repeated: Vec::new() };
        let mut i = 0;
        while i < raw.len() {
            if let Some(key) = raw[i].strip_prefix("--") {
                if flags.contains(&key) {
                    args.options.insert(key.to_string(), None);
                } else if repeated.contains(&key) {
                    let value = raw.get(i + 1).ok_or_else(|| format!("--{key} needs a value"))?;
                    args.repeated.push((key.to_string(), value.clone()));
                    i += 1;
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

    /// Every value of the repeated option `key`, in order.
    fn values(&self, key: &str) -> Vec<String> {
        self.repeated.iter().filter(|(k, _)| k == key).map(|(_, v)| v.clone()).collect()
    }

    /// The whole number `--key` gives, or `default`.
    fn number(&self, key: &str, default: u32) -> Result<u32> {
        self.value(key).map_or(Ok(default), |v| v.parse().map_err(|_| format!("--{key} takes a whole number; {v:?} is not one")))
    }

    /// `--game-dir`, else the game folder `game::dir` finds.
    fn game_dir(&self) -> Result<PathBuf> {
        eu4_harmony_folders::game_dir(self.value("game-dir").map(Path::new))
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

/// Where `cargo xtask release` builds the player package before packing it,
/// in the profile of its own that Cargo.toml gives it.
fn built_package() -> PathBuf {
    built().with_file_name(files::PACKAGE_PROFILE)
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
        Some("install") => Args::parse(rest, &["uninstall"], &["game-dir"]).and_then(|a| files::install(&a)),
        Some("game") => claim::run(rest),
        Some("release") => Args::parse(rest, &["no-workshop"], &["out"]).and_then(|a| files::release(&a)),
        Some("release-check") => Args::parse(rest, &[], &["tag", "out"]).and_then(|a| release::run(&a)),
        Some("workshop-upload") => Args::parse(rest, &["create"], &["user"]).and_then(|a| workshop::upload(&a)),
        Some("disasm") => with_game(
            &["run", "--release", "--quiet", "-p", "eu4-harmony-core", "--features", "analysis", "--example", "disasm", "--"],
            rest,
            "the disassembler",
        ),
        // A player's build's points, then a developer build's with every
        // tool, whose points the player's build does not declare.
        Some("offline-check") => with_game(&["test", "--release", "-p", "eu4-harmony-core", "--", "--ignored"], rest, "the offline checks").and_then(|()| {
            with_game(
                &["test", "--release", "-p", "eu4-harmony-core", "--features", "eu4-harmony-core/dev", "--", "--ignored"],
                rest,
                "the offline checks of the developer build",
            )
        }),
        Some("addresses") => Args::parse(rest, &[], &[]).and_then(|_| addresses::update(&repo())),
        Some("what") => Args::parse(rest, &[], &[]).and_then(|a| match a.positional.as_slice() {
            [rva] => addresses::what(&repo(), rva),
            _ => Err(format!(
                "what takes one address

{USAGE}"
            )),
        }),
        Some("scenario") => scenario::run(rest),
        Some("profile") => Args::parse(rest, &["tree", "wait", "threads"], &["seconds", "hz", "slice", "root", "depth", "min", "waiting-in", "test-game"])
            .and_then(|a| profile::run(&a)),
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
