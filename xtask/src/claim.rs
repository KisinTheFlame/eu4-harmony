//! `cargo xtask game`: the claim (GLOSSARY), one holder of this computer's
//! game at a time, and starting and stopping the game within it
//! (docs/game-testing.md).
//!
//! The claim's course is `course.rs`, over the computer as `machine.rs`
//! reaches it; its record and the decisions over it are `record.rs`. This
//! file reads the command line and gives the rest of xtask the claim's
//! operations on this computer.
mod course;
mod machine;
mod record;

use crate::{repo, Args, Result};
use course::Claimed;
pub use course::Run;
use machine::ThisComputer;
use record::{Asking, Record};
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

/// The exit codes besides 0 and 1.
pub const EXIT_BUSY: i32 = 2;

/// `--owner`, else the leaf name of this checkout's folder, as the script
/// names it; the player's claim is `player`'s.
pub fn owner(args: &Args) -> String {
    if args.flag("player") {
        return "player".into();
    }
    owner_of(args.value("owner"))
}

/// The owner `given`, else the leaf name of this checkout's folder.
pub fn owner_of(given: Option<&str>) -> String {
    given.map(String::from).unwrap_or_else(|| repo().file_name().map_or("checkout".into(), |n| n.to_string_lossy().into_owned()))
}

/// The game holds a claim of `owner`'s; the record if so.
pub fn held_by(owner: &str) -> Result<Record> {
    course::held_by(&ThisComputer, owner)
}

/// Refuses unless this checkout (or `--owner`) holds the game: for the
/// commands that change the game folder or start the game.
pub fn needs_claim(args: &Args) -> Result<Record> {
    held_by(&owner(args))
}

/// `cargo xtask game <command>`.
pub fn run(raw: &[String]) -> Result<i32> {
    let (command, rest) = raw.split_first().ok_or("cargo xtask game claim | start | stop | release | renew | watch | status")?;
    let args =
        Args::parse(rest, &["wait", "player", "now", "sound", "visible", "keep-crashes"], &["purpose", "owner", "lease", "warn", "timeout", "game-dir"])?;
    match command.as_str() {
        "claim" => claim(&args),
        "release" => give_back(&owner(&args), &args.game_dir()?, args.flag("keep-crashes")).map(|()| 0),
        "renew" => renew_as(&owner(&args)).map(|()| 0),
        "watch" => watch(&args),
        "status" => course::status(&ThisComputer).map(|()| 0),
        "start" => start(&args).map(|_| 0),
        "stop" => course::stop(&ThisComputer, &owner(&args)).map(|()| 0),
        other => Err(format!("unknown command game {other:?}; cargo xtask --help lists them")),
    }
}

fn claim(args: &Args) -> Result<i32> {
    let asking = Asking { owner: owner(args), player: args.flag("player"), now: args.flag("now"), wait: args.flag("wait") || args.flag("player") };
    if asking.now && !asking.player {
        return Err("--now is the player's: cargo xtask game claim --player --now".into());
    }
    let lease = if asking.player {
        record::PLAYER_LEASE_MINUTES
    } else {
        args.value("lease").map_or(Ok(course::LEASE_MINUTES), |v| v.parse().map_err(|_| format!("--lease {v}: minutes")))?
    };
    let purpose = args.value("purpose").unwrap_or(if asking.player { "playing" } else { "" }).to_string();
    Ok(match course::claim(&ThisComputer, &asking, &purpose, lease, &args.game_dir()?)? {
        Claimed::Busy => EXIT_BUSY,
        Claimed::Now | Claimed::Already => 0,
    })
}

/// Releases `owner`'s claim, as `cargo xtask game release` does.
fn give_back(owner: &str, game_dir: &Path, keep_crashes: bool) -> Result<()> {
    course::release(&ThisComputer, owner, game_dir, keep_crashes)
}

/// Starts `owner`'s lease again.
fn renew_as(owner: &str) -> Result<()> {
    course::renew(&ThisComputer, owner)
}

/// Holds the game for a run of `owner`'s, as the scenario runner's: the
/// claim `owner` holds already, else a new one with the usual lease,
/// waiting in the queue with `wait`.
pub fn hold_for_run(owner: &str, purpose: &str, wait: bool, game_dir: &Path) -> Result<Run> {
    course::hold_for_run(&ThisComputer, owner, purpose, wait, game_dir)
}

/// Plays `play` within `run`'s claim, kept going meanwhile, its flag set
/// once the player takes the game; then, whatever `play` comes to, ends the
/// game and gives back the claim the run made.
pub fn play_within<T>(run: &Run, game_dir: &Path, interrupted: &(dyn Fn() -> bool + Sync), play: impl FnOnce(&Arc<AtomicBool>) -> Result<T>) -> Result<T> {
    course::play_within(&ThisComputer, run, game_dir, interrupted, play)
}

/// Exits once the lease is down to `--warn` minutes (5) with
/// [`course::EXIT_LEASE_ENDING`], once the player takes the game with
/// [`course::EXIT_PLAYER_TAKES`], and once the claim is not this owner's
/// with 0.
fn watch(args: &Args) -> Result<i32> {
    step_aside();
    let warn = args.value("warn").map_or(Ok(5), |v| v.parse::<u64>().map_err(|_| format!("--warn {v}: minutes")))?;
    course::watch(&ThisComputer, &owner(args), warn)
}

/// Lets cargo build xtask again while `watch` runs, which is for the whole
/// claim: Windows keeps a running program's file from being replaced but
/// not from being renamed, so it moves its own file aside and puts a copy
/// where cargo looks. The files of watches that ended go.
fn step_aside() {
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(dir) = exe.parent() else { return };
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("xtask-watch-") && name.ends_with(".exe") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let aside = dir.join(format!("xtask-watch-{}.exe", std::process::id()));
    if fs::rename(&exe, &aside).is_ok() {
        let _ = fs::copy(&aside, &exe);
    }
}

/// Starts the game hidden within `owner`'s claim, waiting up to `wait` for
/// its main menu; a start that does not reach it ends the game. The seconds
/// it took.
pub fn start_game_as(owner: &str, game_dir: &Path, sound: bool, wait: Duration) -> Result<f64> {
    course::start_game(&ThisComputer, owner, game_dir, sound, wait)
}

/// `cargo xtask game start`: starts the game hidden, within this owner's
/// claim, and waits for its main menu; with `--visible` shows it there, as
/// `desktop.show` shows it. The start is a hidden start either way, so that
/// `--sound` chooses the sound and the game can be hidden again.
fn start(args: &Args) -> Result<f64> {
    let wait = args
        .value("timeout")
        .map_or(Ok(eu4_harmony_client::start::WAIT), |v| v.parse().map(Duration::from_secs).map_err(|_| format!("--timeout {v}: seconds")))?;
    let took = start_game_as(&owner(args), &args.game_dir()?, args.flag("sound"), wait)?;
    if args.flag("visible") {
        show().map_err(|why| format!("the game runs hidden at its main menu, not shown: {why}; eu4-harmony game desktop show shows it"))?;
        println!("the window is shown");
    }
    Ok(took)
}

/// Shows the game's window, through remote control.
fn show() -> Result<()> {
    use eu4_harmony_client::pipe;
    use eu4_harmony_remote::desktop::{Desktop, DesktopRequest};
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut client = pipe::connect(pipe::NAME, pipe::GAME, deadline).map_err(|e| e.to_string())?;
    match client.ask_for::<_, Desktop>(&DesktopRequest::Show {}, Some(deadline)).map_err(|e| e.to_string())? {
        Ok(desktop) if !desktop.hidden => Ok(()),
        Ok(_) => Err("desktop.show answered that the window is still hidden".into()),
        Err(refused) => Err(format!("desktop.show: {refused}")),
    }
}
