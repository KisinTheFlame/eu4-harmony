//! `cargo xtask game claim | release | renew | watch | status`: the claim
//! (GLOSSARY), one holder of this computer's game at a time
//! (docs/game-testing.md).
//!
//! The claim's course is `course.rs`, over the computer as `machine.rs`
//! reaches it; its record and the decisions over it are `record.rs`. This
//! file reads the command line, and gives the rest of xtask nothing: no
//! other command looks at the claim or changes it (ADR
//! only-the-claims-commands-know-the-claim).
mod course;
mod machine;
mod record;

use crate::{repo, Args, Result};
use course::Claimed;
use machine::ThisComputer;
use record::Asking;
use std::fs;

/// The exit codes besides 0 and 1.
const EXIT_BUSY: i32 = 2;

/// `--owner`, else the leaf name of this checkout's folder, as the script
/// names it; the player's claim is `player`'s.
fn owner(args: &Args) -> String {
    if args.flag("player") {
        return "player".into();
    }
    args.value("owner").map(String::from).unwrap_or_else(|| repo().file_name().map_or("checkout".into(), |n| n.to_string_lossy().into_owned()))
}

/// `cargo xtask game <command>`, for the claim's commands.
pub fn run(command: &str, rest: &[String]) -> Result<i32> {
    let args = Args::parse(rest, &["wait", "player", "now", "keep-crashes"], &["purpose", "owner", "lease", "warn", "game-dir"])?;
    match command {
        "claim" => claim(&args),
        "release" => course::release(&ThisComputer, &owner(&args), &args.game_dir()?, args.flag("keep-crashes")).map(|()| 0),
        "renew" => course::renew(&ThisComputer, &owner(&args)).map(|()| 0),
        "watch" => watch(&args),
        "status" => course::status(&ThisComputer).map(|()| 0),
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
