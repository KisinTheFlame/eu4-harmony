//! `cargo xtask game`: claims of test games (GLOSSARY), each holder with a
//! test game of its own until it releases it, and starting and stopping it
//! within the claim (docs/plans/test-games.md).
//!
//! The claim's course is `course.rs`, over the computer as `machine.rs`
//! reaches it; its record and the decisions over it are `record.rs`. This
//! file reads the command line and gives the rest of xtask the claim's
//! operations on this computer.
mod course;
mod machine;
mod record;
#[cfg(test)]
pub(crate) mod recorded;

use crate::{Args, Result};
use course::Claimed;
pub use course::{Run, Starting};
use machine::ThisComputer;
pub use record::GameEnd;
use record::{Asking, Claim};
use std::{path::Path, time::Duration};

/// The exit codes besides 0 and 1.
pub const EXIT_BUSY: i32 = 2;

/// `--owner`, else the leaf name of this checkout's folder.
pub fn owner(args: &Args) -> String {
    owner_of(args.value("owner"))
}

/// The owner `given`, else the leaf name of this checkout's folder; it
/// names the owner's test game and its pipe, so it is made of letters,
/// digits, `.`, `_` and `-`.
pub fn owner_of(given: Option<&str>) -> String {
    let name = given.map(String::from).unwrap_or_else(|| crate::repo().file_name().map_or("checkout".into(), |n| n.to_string_lossy().into_owned()));
    let name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).take(64).collect();
    if name.is_empty() {
        "checkout".into()
    } else {
        name
    }
}

/// `owner`'s claim, once it is made.
pub fn held_by(owner: &str) -> Result<Claim> {
    course::held_by(&ThisComputer, owner)
}

/// The pipe of `owner`'s test game.
pub fn pipe_of(owner: &str) -> Result<String> {
    course::pipe_of(owner)
}

/// The user folder of `owner`'s test game.
pub fn user_dir(owner: &str) -> Result<std::path::PathBuf> {
    held_by(owner).map(|c| course::user_dir(&c))
}

/// The process of `owner`'s test game while it runs.
pub fn game_of(owner: &str) -> Option<u32> {
    course::game_of_owner(&ThisComputer, owner)
}

/// The `eu4.exe` serving the test game `name`'s pipe, if any.
pub fn game_named(name: &str) -> Option<u32> {
    use machine::Machine as _;
    ThisComputer.serving(&pipe_of(name).ok()?)
}

/// `cargo xtask game <command>`.
pub fn run(raw: &[String]) -> Result<i32> {
    let (command, rest) = raw.split_first().ok_or("cargo xtask game claim | start | stop | release | status")?;
    if command == "follow" {
        return follow(rest).map(|()| 0);
    }
    let args = Args::parse_repeated(
        rest,
        &["wait", "sound", "visible", "player", "now"],
        &["purpose", "owner", "lease", "timeout", "game-dir", "seed"],
        &["save", "mod"],
    )?;
    match command.as_str() {
        "claim" => claim(&args),
        "release" => course::release(&ThisComputer, &owner(&args)).map(|()| 0),
        "status" => course::status(&ThisComputer).map(|()| 0),
        "start" => start(&args).map(|_| 0),
        "stop" => course::stop(&ThisComputer, &owner(&args)).map(|()| 0),
        "watch" | "renew" => {
            Err(format!("game {command} is gone: a test game keeps nothing to put back, and a claim holds it until it is released (docs/plans/test-games.md)"))
        }
        other => Err(format!("unknown command game {other:?}; cargo xtask --help lists them")),
    }
}

fn claim(args: &Args) -> Result<i32> {
    if args.flag("player") || args.flag("now") {
        return Err("the player's claim is gone: the player's game touches no test game's files (docs/plans/test-games.md)".into());
    }
    if args.value("lease").is_some() {
        return Err("--lease is gone: a claim holds its test game until it is released (docs/plans/test-games.md)".into());
    }
    let asking = Asking { owner: owner(args), wait: args.flag("wait") };
    let purpose = args.value("purpose").unwrap_or("").to_string();
    Ok(match course::claim(&ThisComputer, &asking, &purpose, &args.game_dir()?)? {
        Claimed::Busy => EXIT_BUSY,
        Claimed::Now | Claimed::Already => 0,
    })
}

/// `cargo xtask game follow <folder> <id> <pipe>`: what a claim's
/// `follow.exe` runs, started by `game start`.
fn follow(rest: &[String]) -> Result<()> {
    let [folder, id, pipe] = rest else { return Err("cargo xtask game follow <folder> <process id> <pipe>".into()) };
    let id = id.parse().map_err(|_| format!("{id}: a process id"))?;
    machine::follow_games(Path::new(folder), id, pipe)
}

/// Holds a test game for a run of `owner`'s, as the scenario runner's: the
/// claim `owner` holds already, else a new one, waiting with `wait` while
/// it is busy.
pub fn hold_for_run(owner: &str, purpose: &str, wait: bool, game_dir: &Path) -> Result<Run> {
    course::hold_for_run(&ThisComputer, owner, purpose, wait, game_dir)
}

/// Plays `play` within `run`'s claim; then, whatever `play` comes to, ends
/// the test game and releases the claim the run made.
pub fn play_within<T>(run: &Run, interrupted: &(dyn Fn() -> bool + Sync), play: impl FnOnce() -> Result<T>) -> Result<T> {
    course::play_within(&ThisComputer, run, interrupted, play)
}

/// The test game of a run within a claim, the scenario runner's, as the run
/// reaches its process: through the claim's machine, so that how a run's
/// game ends can be tried on a recorded one.
pub trait RunGame {
    /// The test game's process while it runs.
    fn game(&self) -> Option<u32>;
    /// Starts the test game hidden, waiting up to `wait` for its main menu:
    /// the seconds it took.
    fn start(&self, starting: &Starting, wait: Duration) -> Result<f64>;
    /// Ends the test game, if it runs.
    fn stop(&self) -> Result<()>;
    /// How its games have ended so far, as the claim follows them.
    fn ends(&self) -> Vec<GameEnd>;
    /// Waits before looking again.
    fn wait(&self, time: Duration);
}

/// The test game of `owner`'s run on `m`.
pub struct InClaim<'m, M> {
    m: &'m M,
    owner: String,
    game_dir: std::path::PathBuf,
}

impl<M: machine::Machine> RunGame for InClaim<'_, M> {
    fn game(&self) -> Option<u32> {
        course::game_of_owner(self.m, &self.owner)
    }

    fn start(&self, starting: &Starting, wait: Duration) -> Result<f64> {
        course::start_game(self.m, &self.owner, &self.game_dir, starting, wait)
    }

    fn stop(&self) -> Result<()> {
        course::stop(self.m, &self.owner)
    }

    fn ends(&self) -> Vec<GameEnd> {
        course::ends_of(self.m, &self.owner)
    }

    fn wait(&self, time: Duration) {
        self.m.wait(time);
    }
}

/// The test game of `owner`'s run on this computer, started from `game_dir`.
pub fn run_game(owner: &str, game_dir: &Path) -> InClaim<'static, ThisComputer> {
    InClaim { m: &ThisComputer, owner: owner.into(), game_dir: game_dir.into() }
}

/// The test game of `owner`'s run on the recorded computer `m`.
#[cfg(test)]
pub fn recorded_run_game<'m>(m: &'m recorded::Recorded, owner: &str) -> InClaim<'m, recorded::Recorded> {
    InClaim { m, owner: owner.into(), game_dir: recorded::GAME_DIR.into() }
}

/// Ends `owner`'s test game, if it runs.
pub fn stop_game_as(owner: &str) -> Result<()> {
    course::stop(&ThisComputer, owner)
}

/// Starts `owner`'s test game hidden, with this checkout's built core,
/// waiting up to `wait` for its main menu. The seconds it took.
pub fn start_game_as(owner: &str, game_dir: &Path, starting: &Starting, wait: Duration) -> Result<f64> {
    course::start_game(&ThisComputer, owner, game_dir, starting, wait)
}

/// `cargo xtask game start`: starts this owner's test game hidden and waits
/// for its main menu; with `--visible` shows it there, as `desktop.show`
/// shows it. The start is a hidden start either way, so that `--sound`
/// chooses the sound and the game can be hidden again. `--seed` starts it
/// with the game's own `-seed`; each `--save` copies a save of the player's
/// into the test game's user folder first, and each `--mod` puts a mod's
/// folder in its playset, as a scenario's test mods are.
fn start(args: &Args) -> Result<f64> {
    let wait = args
        .value("timeout")
        .map_or(Ok(eu4_harmony_client::start::WAIT), |v| v.parse().map(Duration::from_secs).map_err(|_| format!("--timeout {v}: seconds")))?;
    let seed = args.value("seed").map(|v| v.parse::<u32>().map_err(|_| format!("--seed {v}: a whole number below 2^32"))).transpose()?;
    let owner = owner(args);
    let mods = args.values("mod").into_iter().map(std::path::PathBuf::from).collect();
    let starting = Starting { sound: args.flag("sound"), seed, saves: args.values("save"), options: None, mods };
    let took = start_game_as(&owner, &args.game_dir()?, &starting, wait)?;
    if args.flag("visible") {
        show(&pipe_of(&owner)?).map_err(|why| format!("the game runs hidden at its main menu, not shown: {why}; eu4-harmony game desktop show shows it"))?;
        println!("the window is shown");
    }
    Ok(took)
}

/// Shows the game's window, through remote control on the pipe `name`.
fn show(name: &str) -> Result<()> {
    use eu4_harmony_client::pipe;
    use eu4_harmony_remote::desktop::{Desktop, DesktopRequest};
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut client = pipe::connect(name, pipe::GAME, deadline).map_err(|e| e.to_string())?;
    match client.ask_for::<_, Desktop>(&DesktopRequest::Show {}, Some(deadline)).map_err(|e| e.to_string())? {
        Ok(desktop) if !desktop.hidden => Ok(()),
        Ok(_) => Err("desktop.show answered that the window is still hidden".into()),
        Err(refused) => Err(format!("desktop.show: {refused}")),
    }
}
