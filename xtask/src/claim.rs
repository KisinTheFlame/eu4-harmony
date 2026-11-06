//! `cargo xtask game`: the claim (GLOSSARY), one holder of this computer's
//! game at a time, and starting and stopping the game within it
//! (docs/game-testing.md).
//!
//! The record of who holds the game is `%LOCALAPPDATA%\eu4-harmony-dev\
//! game-lock.json`, as the game-lock skill's script kept it and still keeps
//! it in checkouts from before the claim's commands, the queue
//! `game-queue.json` beside it; a named mutex, the script's, guards every
//! read and write of both. Each claim has a folder of its own under
//! `claims\`: `backup\` (the player's files, as `eu4-harmony backup` keeps them) and
//! `framework\` (the game folder's framework files and `dev.toml`, each as
//! it was) while it is held, `kept\` (the framework's and the game's logs and
//! the run's crash folders) after it is released.
mod record;

use crate::{game, repo, Args, Result};
use record::{Asking, Found, Process, Record, Release, Seen, Waiting};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The files of the game folder a run may change, by their path in it.
const FRAMEWORK_FILES: [&str; 5] =
    ["version.dll", "eu4-harmony\\eu4_harmony_core.dll", "eu4-harmony\\eu4-harmony.exe", "eu4-harmony\\dev.toml", "eu4-harmony\\backup\\version.dll"];
/// The lease of a claim, in minutes, without `--lease`.
const LEASE_MINUTES: u64 = 30;
/// How often a waiting claim looks again, and says it still waits.
const WAIT_EVERY: Duration = Duration::from_secs(5);
/// How often `watch` looks.
const WATCH_EVERY: Duration = Duration::from_secs(5);
/// A `kept\` older than this goes at the next claim.
const KEEP_DAYS: u64 = 7;

/// The exit codes besides 0 and 1.
pub const EXIT_BUSY: i32 = 2;
pub const EXIT_LEASE_ENDING: i32 = 3;
pub const EXIT_PLAYER_TAKES: i32 = 4;

fn base() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let base = PathBuf::from(local).join("eu4-harmony-dev");
    fs::create_dir_all(&base).map_err(|e| format!("cannot make {}: {e}", base.display()))?;
    Ok(base)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The process `id` is now, with when it started; none when no process has
/// that id.
fn process(id: u32) -> Option<Process> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id);
        if handle.is_null() {
            return None;
        }
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        let read = GetProcessTimes(handle, created, exited, kernel, user) != 0;
        CloseHandle(handle);
        read.then(|| Process { id, started: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime) })
    }
}

/// This process.
fn me() -> Process {
    process(std::process::id()).unwrap_or(Process { id: std::process::id(), started: 0 })
}

/// Whether `p` runs still: a process has its id, started when it did, and
/// has not ended. One that cannot be looked at for another reason than its
/// being gone runs, as far as the claim goes, which waits for it then.
fn alive(p: &Process) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, FILETIME},
        System::Threading::{GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, p.id);
        if handle.is_null() {
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [created, exited, kernel, user] = &mut times;
        let mut code = 0;
        let alive = if GetProcessTimes(handle, created, exited, kernel, user) == 0 || GetExitCodeProcess(handle, &mut code) == 0 {
            true
        } else {
            let started = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
            started == p.started && code == STILL_ACTIVE
        };
        CloseHandle(handle);
        alive
    }
}

/// What this process sees now, for a look at the record.
fn seen() -> Seen<'static> {
    let games = eu4_harmony_client::game_processes();
    Seen { now: now(), game_running: games.as_ref().map_or(true, |g| !g.is_empty()), games: games.unwrap_or_default(), me: me(), alive: &alive }
}

/// The local date and time: `yyyy-MM-dd HH:mm`, and `yyyyMMdd-HHmmss` for a
/// folder's name.
fn stamps() -> (String, String) {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    (
        format!("{:04}-{:02}-{:02} {:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute),
        format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond),
    )
}

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

/// The record and the queue, under the mutex that guards them.
struct State {
    record: PathBuf,
    queue: PathBuf,
    held: Option<Record>,
    waiting: Vec<Waiting>,
}

/// Runs `action` on the record and the queue, holding the mutex, and writes
/// back what it changed.
fn locked<T>(action: impl FnOnce(&mut State) -> T) -> Result<T> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0},
        System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject, INFINITE},
    };
    let base = base()?;
    let name: Vec<u16> = "Global\\eu4-harmony-dev-game-lock\0".encode_utf16().collect();
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if mutex.is_null() {
        return Err("cannot open the claim's mutex".into());
    }
    let waited = unsafe { WaitForSingleObject(mutex, INFINITE) };
    if waited != WAIT_OBJECT_0 && waited != WAIT_ABANDONED {
        unsafe { CloseHandle(mutex) };
        return Err("cannot take the claim's mutex".into());
    }
    let result = (|| {
        let mut state = State { record: base.join("game-lock.json"), queue: base.join("game-queue.json"), held: None, waiting: Vec::new() };
        state.held = read_json(&state.record).as_ref().and_then(Record::from_json);
        state.waiting = read_json(&state.queue).and_then(|v| v.as_array().map(|a| a.iter().filter_map(Waiting::from_json).collect())).unwrap_or_default();
        let (held, waiting) = (state.held.clone(), state.waiting.clone());
        let value = action(&mut state);
        if state.held != held {
            match &state.held {
                Some(h) => write_json(&state.record, &h.to_json())?,
                None => remove_file(&state.record)?,
            }
        }
        if state.waiting != waiting {
            write_json(&state.queue, &serde_json::Value::Array(state.waiting.iter().map(Waiting::to_json).collect()))?;
        }
        Ok(value)
    })();
    unsafe {
        ReleaseMutex(mutex);
        CloseHandle(mutex);
    }
    result
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(value).expect("JSON")).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("cannot remove {}: {e}", path.display())),
        _ => Ok(()),
    }
}

/// The player's user folder, for the game in `game_dir`.
fn user_folder(game_dir: &Path) -> Result<PathBuf> {
    eu4_harmony_client::backup::user_folder(&eu4_harmony_manifest::user_dir(Some(game_dir))?)
}

/// The game holds a claim of `owner`'s; the record if so.
pub fn held_by(owner: &str) -> Result<Record> {
    let held = locked(|s| s.held.clone())?;
    match held {
        Some(h) if h.owner == owner && h.unfinished() => {
            Err(format!("{owner}'s claim is not complete: its backup of the player's files is not done; cargo xtask game claim makes it again"))
        }
        Some(h) if h.owner == owner && h.putting_back.is_some() => {
            Err(format!("{owner}'s claim is being released, or a release of it did not end: cargo xtask game release"))
        }
        Some(h) if h.owner == owner => Ok(h),
        Some(h) => Err(format!("{owner} does not hold the game: {}; cargo xtask game claim", h.describe(now()))),
        None => Err(format!("{owner} does not hold the game; cargo xtask game claim")),
    }
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
        "release" => release(&args).map(|()| 0),
        "renew" => renew(&args).map(|()| 0),
        "watch" => watch(&args),
        "status" => status().map(|()| 0),
        "start" => start(&args).map(|_| 0),
        "stop" => stop(&args).map(|()| 0),
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
        args.value("lease").map_or(Ok(LEASE_MINUTES), |v| v.parse().map_err(|_| format!("--lease {v}: minutes")))?
    };
    let purpose = args.value("purpose").unwrap_or(if asking.player { "playing" } else { "" }).to_string();
    Ok(match claim_as(&asking, &purpose, lease, &args.game_dir()?)? {
        Claimed::Busy => EXIT_BUSY,
        Claimed::Now | Claimed::Already => 0,
    })
}

/// What a claim came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Claimed {
    /// The game is held by this claim now.
    Now,
    /// This owner held it already.
    Already,
    /// Another holds it, and the claim did not wait.
    Busy,
}

/// Claims the game for `owner` with the usual lease, waiting in the queue
/// with `wait`, as `cargo xtask game claim` does.
pub fn take(owner: &str, purpose: &str, wait: bool, game_dir: &Path) -> Result<Claimed> {
    claim_as(&Asking { owner: owner.into(), player: false, now: false, wait }, purpose, LEASE_MINUTES, game_dir)
}

fn claim_as(asking: &Asking, purpose: &str, lease: u64, game_dir: &Path) -> Result<Claimed> {
    let claims = base()?.join("claims");
    let mut said = String::new();
    let mut folder = PathBuf::new();
    loop {
        let seen = seen();
        let found = locked(|s| {
            let found = record::look(asking, &mut s.held, &mut s.waiting, &seen);
            if found == Found::Free {
                let (since, stamp) = stamps();
                folder = claims.join(format!("{}-{stamp}", asking.owner));
                // The record names the claim's folder, and the process making
                // it, before the backup: a claim cut short is known as one.
                let session = !asking.player;
                s.held = Some(Record {
                    owner: asking.owner.clone(),
                    purpose: purpose.to_string(),
                    checkout: repo().display().to_string(),
                    since,
                    renewed: now(),
                    lease_minutes: lease,
                    backup: None,
                    folder: session.then(|| folder.display().to_string()),
                    player: asking.player,
                    taken: None,
                    preparing: session.then(me),
                    putting_back: None,
                    keeper: None,
                    game: None,
                });
            }
            found
        })?;
        match found {
            Found::Held => {
                println!("already held: {}", held_by(&asking.owner)?.describe(now()));
                return Ok(Claimed::Already);
            }
            Found::Busy(who) if !asking.wait => {
                println!("busy: held by {who}");
                return Ok(Claimed::Busy);
            }
            Found::Busy(who) => {
                if who != said {
                    println!("waiting: held by {who}");
                    said = who;
                }
                std::thread::sleep(WAIT_EVERY);
            }
            Found::Take(holder) => {
                println!("the player takes the game from {}: ending the game and releasing its claim", holder.describe(now()));
                put_back_for(&holder, || {
                    game::kill();
                    game::wait_closed()?;
                    put_back(&holder, game_dir, false).map(drop)
                })?;
                // The game is free now, and the player first.
                return claim_as(asking, purpose, lease, game_dir);
            }
            Found::Gone(holder) => {
                if holder.unfinished() {
                    println!("{}: its claim was cut short before its backup was done", holder.describe(now()));
                } else {
                    println!("{} has gone: putting back what its run changed", holder.describe(now()));
                }
                put_back_for(&holder, || {
                    // A holder gone with its game running left it running.
                    if game::running() {
                        if !holder.only_its_game_runs(&self::seen()) {
                            return Err("eu4.exe is running, and not as the gone holder's game: the player may be playing".into());
                        }
                        println!("ending its game");
                        game::kill();
                        game::wait_closed()?;
                    }
                    put_back(&holder, game_dir, false).map(drop)
                })?;
                if holder.owner != asking.owner {
                    println!("took over from {}", holder.describe(now()));
                }
                return claim_as(asking, purpose, lease, game_dir);
            }
            Found::Free => break,
        }
    }
    if asking.player {
        println!("claimed for the player: {}", held_by(&asking.owner)?.describe(now()));
        return Ok(Claimed::Now);
    }
    let me = me();
    let making = |h: &Record| h.owner == asking.owner && h.folder.as_deref() == Some(&*folder.display().to_string()) && h.preparing == Some(me);
    match prepare(&claims, &folder, game_dir) {
        Ok(()) => {
            let held = locked(|s| {
                let h = s.held.as_mut().filter(|h| making(h))?;
                h.backup = Some(folder.join("backup").display().to_string());
                h.preparing = None;
                h.renewed = now();
                Some(h.clone())
            })?
            .ok_or_else(|| format!("lost the claim while backing up; the backup is at {}", folder.display()))?;
            println!("claimed: {}", held.describe(now()));
            println!("the claim's folder: {}", folder.display());
            Ok(Claimed::Now)
        }
        Err(e) => {
            // No claim without its backup: the game is given back, and what
            // was made of the claim's folder goes, a copy of files that
            // nothing changed.
            locked(|s| {
                if s.held.as_ref().is_some_and(making) {
                    s.held = None;
                }
            })?;
            let _ = fs::remove_dir_all(&folder);
            Err(format!("not claimed: {e}; the game is free again"))
        }
    }
}

/// Puts back `holder`'s claim, which this process marked as its own to put
/// back, with `putting_back`; frees the game once all is put back, and
/// leaves the claim to be put back again otherwise.
fn put_back_for(holder: &Record, putting_back: impl FnOnce() -> Result<()>) -> Result<()> {
    let done = putting_back();
    let me = me();
    let ended = locked(|s| record::put_back(&mut s.held, holder, me, done.is_ok()))?;
    done.map_err(|e| format!("{e}; {} still holds the game, its claim not put back", holder.owner))?;
    if !ended {
        return Err(format!("{}'s claim was put back, but its record was changed meanwhile and stays: cargo xtask game status says by whom", holder.owner));
    }
    Ok(())
}

/// Makes the claim's folder, with the player's files and the game folder's
/// framework files in it; removes the `kept\` of claims released long ago.
fn prepare(claims: &Path, folder: &Path, game_dir: &Path) -> Result<()> {
    forget_old(claims);
    let framework = folder.join("framework");
    fs::create_dir_all(&framework).map_err(|e| format!("cannot make {}: {e}", framework.display()))?;
    for file in FRAMEWORK_FILES {
        let from = game_dir.join(file);
        if from.is_file() {
            let to = framework.join(file);
            fs::create_dir_all(to.parent().expect("a file's folder")).map_err(|e| e.to_string())?;
            fs::copy(&from, &to).map_err(|e| format!("cannot copy {}: {e}", from.display()))?;
        }
    }
    eu4_harmony_client::game_not_running()?;
    eu4_harmony_client::backup::backup(&user_folder(game_dir)?, &folder.join("backup"))
}

/// Removes the claims' folders whose `kept\` is older than [`KEEP_DAYS`]
/// and that a claim no longer holds.
fn forget_old(claims: &Path) {
    let Ok(entries) = fs::read_dir(claims) else { return };
    let limit = Duration::from_secs(KEEP_DAYS * 24 * 3600);
    for entry in entries.flatten() {
        let path = entry.path();
        let only_kept = path.join("kept").is_dir() && !path.join("backup").exists() && !path.join("framework").exists();
        let old = fs::metadata(path.join("kept")).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > limit);
        if only_kept && old {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Puts back what a claim's run changed: copies the logs and the run's crash
/// folders into `kept\`, restores the player's files, and the game folder's
/// framework files as `framework\` has them. Returns whether the run
/// crashed. A file not put back fails it, and the claim's folder stays.
fn put_back(holder: &Record, game_dir: &Path, keep_crashes: bool) -> Result<bool> {
    if holder.unfinished() {
        // Nothing ran under a claim whose backup was not done: what it made
        // is part of a copy of files nothing changed, and goes.
        if let Some(folder) = holder.folder.as_deref().map(Path::new).filter(|f| f.exists()) {
            fs::remove_dir_all(folder).map_err(|e| format!("cannot remove {}: {e}", folder.display()))?;
        }
        println!("the claim's backup was not done, so nothing ran under it: nothing to put back");
        return Ok(false);
    }
    let folder = holder.folder.as_ref().map(PathBuf::from);
    let user = eu4_harmony_manifest::user_dir(Some(game_dir))?;
    let mut crashed = false;
    if let Some(folder) = &folder {
        crashed = keep(folder, &user, holder.backup.as_deref().map(Path::new));
    }
    if let Some(backup) = holder.backup.as_deref().filter(|b| Path::new(b).is_dir()) {
        eu4_harmony_client::game_not_running()?;
        eu4_harmony_client::backup::restore(&user_folder(game_dir)?, Path::new(backup), false, keep_crashes)?;
    }
    if let Some(folder) = &folder {
        let framework = folder.join("framework");
        if framework.is_dir() {
            let lost = restore_framework(&framework, game_dir);
            if !lost.is_empty() {
                lost.iter().for_each(|line| println!("NOT RESTORED {line}"));
                return Err(format!("{} file(s) of the game folder not put back; they are in {}", lost.len(), framework.display()));
            }
            fs::remove_dir_all(&framework).map_err(|e| format!("cannot remove {}: {e}", framework.display()))?;
            println!("the game folder's framework files are as they were");
        }
    }
    Ok(crashed)
}

/// Copies the framework's logs, the game's logs and the crash folders the
/// backup did not know into `<folder>\kept\`; whether there was a crash.
fn keep(folder: &Path, user: &Path, backup: Option<&Path>) -> bool {
    let kept = folder.join("kept");
    let _ = copy_tree(&user.join("eu4-harmony").join("logs"), &kept.join("eu4-harmony-logs"));
    let _ = copy_tree(&user.join("logs"), &kept.join("game-logs"));
    let known: Vec<String> =
        backup.and_then(|b| fs::read_to_string(b.join("crashes.txt")).ok()).map(|t| t.lines().map(String::from).collect()).unwrap_or_default();
    let mut crashed = false;
    if let Ok(entries) = fs::read_dir(user.join("crashes")) {
        for entry in entries.flatten().filter(|e| e.path().is_dir()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !known.contains(&name) {
                crashed = true;
                let _ = copy_tree(&entry.path(), &kept.join("crashes").join(&name));
            }
        }
    }
    crashed
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &to.join(entry.file_name()))?;
        } else {
            fs::copy(&path, to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Puts the game folder's framework files back as `framework` has them, one
/// that was not there removed, and compares each; what is not as it was.
fn restore_framework(framework: &Path, game_dir: &Path) -> Vec<String> {
    let mut lost = Vec::new();
    for file in FRAMEWORK_FILES {
        let (saved, there) = (framework.join(file), game_dir.join(file));
        let done = if saved.is_file() {
            fs::create_dir_all(there.parent().expect("a file's folder")).and_then(|()| fs::copy(&saved, &there)).map_err(|e| e.to_string()).and_then(|_| {
                if fs::read(&saved).ok() == fs::read(&there).ok() {
                    Ok(())
                } else {
                    Err("differs after the copy".into())
                }
            })
        } else {
            remove_file(&there)
        };
        if let Err(e) = done {
            lost.push(format!("{}: {e}", there.display()));
        }
    }
    lost
}

fn release(args: &Args) -> Result<()> {
    give_back(&owner(args), &args.game_dir()?, args.flag("keep-crashes"))
}

/// Releases `owner`'s claim, as `cargo xtask game release` does.
pub fn give_back(owner: &str, game_dir: &Path, keep_crashes: bool) -> Result<()> {
    let held = match locked(|s| record::release(owner, &mut s.held, &seen()))? {
        Release::Mine(held) => held,
        Release::Busy(why) => return Err(why),
        Release::NotHeld => return Err(held_by(owner).err().unwrap_or_else(|| format!("{owner} does not hold the game"))),
    };
    let mut crashed = false;
    put_back_for(&held, || {
        if game::running() {
            if held.player {
                return Err("eu4.exe is running; close it first".into());
            }
            println!("ending the game");
            game::kill();
            game::wait_closed()?;
        }
        crashed = put_back(&held, game_dir, keep_crashes)?;
        Ok(())
    })?;
    if held.unfinished() {
        println!("released: the claim was not complete, its backup not done, so nothing was put back; the game is free");
        return Ok(());
    }
    println!("released: the game is free");
    if !held.player {
        let minutes = (now().saturating_sub(since_seconds(&held))) / 60;
        println!("held since {} (about {minutes} min); the game {}", held.since, if crashed { "crashed" } else { "did not crash" });
        if let Some(folder) = &held.folder {
            println!("kept: {}", Path::new(folder).join("kept").display());
        }
    }
    Ok(())
}

/// When the claim began, from its folder's creation, or the lease.
fn since_seconds(held: &Record) -> u64 {
    held.folder
        .as_ref()
        .and_then(|f| fs::metadata(f).ok())
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(held.renewed, |d| d.as_secs())
}

fn renew(args: &Args) -> Result<()> {
    renew_as(&owner(args))
}

/// Starts `owner`'s lease again.
pub fn renew_as(owner: &str) -> Result<()> {
    let me = owner.to_string();
    let held = locked(|s| {
        let h = s.held.as_mut().filter(|h| h.owner == me)?;
        h.renewed = now();
        Some(h.clone())
    })?
    .ok_or_else(|| format!("{me} does not hold the game"))?;
    println!("renewed: {}", held.describe(now()));
    Ok(())
}

/// Exits once the lease is down to `--warn` minutes (5) with
/// [`EXIT_LEASE_ENDING`], once the player takes the game with
/// [`EXIT_PLAYER_TAKES`], and once the claim is not this owner's with 0.
fn watch(args: &Args) -> Result<i32> {
    step_aside();
    let me = owner(args);
    let warn = args.value("warn").map_or(Ok(5), |v| v.parse::<u64>().map_err(|_| format!("--warn {v}: minutes")))?;
    loop {
        let held = keep_going(&me)?;
        let Some(held) = held else {
            println!("{me} no longer holds the game");
            return Ok(0);
        };
        if let Some(taken) = held.taken {
            let left = (taken + record::TAKEN_GRACE_SECONDS).saturating_sub(now());
            println!("the player takes the game in {left} s: keep what the run needs; the game is then ended and the claim released for you");
            return Ok(EXIT_PLAYER_TAKES);
        }
        let left = held.lease_ends().saturating_sub(now());
        if left <= 60 * warn {
            println!("the lease ends in {} min: cargo xtask game renew and watch again to keep the game, or release", left.div_ceil(60));
            return Ok(EXIT_LEASE_ENDING);
        }
        std::thread::sleep(WATCH_EVERY);
    }
}

/// What one look at a claim found, as `watch` looks every few seconds.
#[derive(Debug, PartialEq, Eq)]
pub enum Look {
    /// Held, with this many seconds of the lease left.
    Held(u64),
    /// The player takes the game.
    Taken,
    /// The claim is not this owner's any more.
    Gone,
}

/// One look at `owner`'s claim by the process keeping it going, which keeps
/// the lease going while the game runs.
pub fn look(owner: &str) -> Result<Look> {
    Ok(match keep_going(owner)? {
        None => Look::Gone,
        Some(h) if h.taken.is_some() => Look::Taken,
        Some(h) => Look::Held(h.lease_ends().saturating_sub(now())),
    })
}

/// The look of the process keeping `owner`'s claim going (record::keep); the
/// claim, if it is still `owner`'s.
fn keep_going(owner: &str) -> Result<Option<Record>> {
    let seen = seen();
    locked(|s| record::keep(owner, &mut s.held, &seen, process))
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

fn status() -> Result<()> {
    let seen = seen();
    let (held, mut waiting) = locked(|s| (s.held.clone(), s.waiting.clone()))?;
    record::tidy(&mut waiting, now());
    match &held {
        Some(h) => {
            println!("held by {}", h.describe(now()));
            if let Some(folder) = &h.folder {
                println!("  the claim's folder: {folder}");
            }
            if h.stale(&seen) {
                println!("  the lease has run out: the next claim takes over");
            }
            match h.keeper {
                Some(k) if alive(&k) => println!("  kept going by process {}, which keeps the lease going while the game runs", k.id),
                _ => println!("  no runner or watch keeps it going: the lease counts down while the game runs too"),
            }
            if let Some(taken) = h.taken {
                println!("  the player takes the game in {} s", (taken + record::TAKEN_GRACE_SECONDS).saturating_sub(now()));
            }
            if let Some(p) = h.putting_back {
                let ended = if alive(&p) { "" } else { ", until it ended: the next release or claim puts it back" };
                println!("  process {} puts back what its run changed{ended}", p.id);
            }
        }
        None => println!("free"),
    }
    for (i, w) in waiting.iter().enumerate() {
        println!("waiting {}: {}{}", i + 1, w.owner, if w.player { " (the player)" } else { "" });
    }
    match eu4_harmony_client::game_running() {
        Ok(running) => println!("eu4.exe running: {running}"),
        Err(e) => println!("eu4.exe running: cannot tell, so it counts as running ({e})"),
    }
    let claims = base()?.join("claims");
    if let Ok(entries) = fs::read_dir(&claims) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ours = held.as_ref().and_then(|h| h.folder.as_deref()).is_some_and(|f| Path::new(f) == path);
            if !ours && (path.join("backup").exists() || path.join("framework").exists()) {
                println!("a claim's folder no claim refers to, not put back: {}", path.display());
            }
        }
    }
    let backups = base()?.join("backups");
    if let Ok(entries) = fs::read_dir(&backups) {
        for entry in entries.flatten() {
            let path = entry.path().display().to_string();
            if held.as_ref().and_then(|h| h.backup.as_deref()) != Some(path.as_str()) {
                println!("a backup no claim refers to: {path}");
            }
        }
    }
    Ok(())
}

/// Starts the game hidden, within this owner's claim, and waits for its main
/// menu; a start that does not reach it ends the game. The seconds it took.
pub fn start_game(args: &Args, sound: bool) -> Result<f64> {
    let wait = args
        .value("timeout")
        .map_or(Ok(eu4_harmony_client::start::WAIT), |v| v.parse().map(Duration::from_secs).map_err(|_| format!("--timeout {v}: seconds")))?;
    start_game_as(&owner(args), &args.game_dir()?, sound, wait)
}

/// Starts the game hidden within `owner`'s claim, waiting up to `wait` for
/// its main menu; a start that does not reach it ends the game.
pub fn start_game_as(owner: &str, game_dir: &Path, sound: bool, wait: Duration) -> Result<f64> {
    held_by(owner)?;
    renew_as(owner)?;
    let log = eu4_harmony_manifest::user_dir(Some(game_dir))?.join("eu4-harmony").join("logs").join("core.log");
    let began = std::time::Instant::now();
    let started = eu4_harmony_client::start::start(game_dir, &log, true, sound, wait);
    if let Ok(id) = started {
        let game = process(id);
        locked(|s| {
            if let Some(h) = s.held.as_mut().filter(|h| h.owner == owner) {
                h.game = game;
            }
        })?;
    }
    if started.is_err() && game::running() {
        println!("ending the game, which did not reach its main menu");
        game::kill();
        game::wait_closed()?;
    }
    started.map(|_| began.elapsed().as_secs_f64())
}

/// `game start`: hidden, and with `--visible` shown once at the main menu,
/// as `desktop.show` shows it; the start is a hidden start either way, so
/// that `--sound` chooses the sound and the game can be hidden again.
fn start(args: &Args) -> Result<f64> {
    let took = start_game(args, args.flag("sound"))?;
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

fn stop(args: &Args) -> Result<()> {
    needs_claim(args)?;
    game::kill();
    game::wait_closed()?;
    println!("the game is closed");
    Ok(())
}
