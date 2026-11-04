//! `cargo xtask game`: the claim (GLOSSARY), one holder of this computer's
//! game at a time, and starting and stopping the game within it
//! (docs/game-testing.md).
//!
//! The record of who holds the game is `%LOCALAPPDATA%\eu4-harmony-dev\
//! game-lock.json`, as the game-lock skill's script keeps it, the queue
//! `game-queue.json` beside it; a named mutex, the script's, guards every
//! read and write of both. Each claim has a folder of its own under
//! `claims\`: `backup\` (the player's files, by `eu4-harmony backup`) and
//! `framework\` (the game folder's framework files and `dev.toml`, each as
//! it was) while it is held, `kept\` (the framework's and the game's logs and
//! the run's crash folders) after it is released.
mod record;

use crate::{cargo, game, repo, Args, Result};
use record::{Asking, Found, Record, Waiting};
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
fn owner(args: &Args) -> String {
    if args.flag("player") {
        return "player".into();
    }
    args.value("owner").map(String::from).unwrap_or_else(|| repo().file_name().map_or("checkout".into(), |n| n.to_string_lossy().into_owned()))
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

/// The checkout's own command line tool, built by cargo as needed.
fn tool(words: &[&str], game_dir: &Path) -> Result<()> {
    let mut cli = cargo();
    cli.args(["run", "--release", "--quiet", "-p", "eu4-harmony-cli", "--"]).args(words).arg("--game-dir").arg(game_dir);
    crate::run(cli, &format!("eu4-harmony {}", words.first().copied().unwrap_or_default()))
}

/// The game holds a claim of `owner`'s; the record if so.
pub fn held_by(owner: &str) -> Result<Record> {
    let held = locked(|s| s.held.clone())?;
    match held {
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
    let args = Args::parse(rest, &["wait", "player", "now", "sound", "keep-crashes"], &["purpose", "owner", "lease", "warn", "timeout", "game-dir"])?;
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
    let mut said = String::new();
    let (gone, took) = loop {
        let running = game::running();
        let found = locked(|s| {
            let found = record::look(&asking, &mut s.held, &mut s.waiting, now(), running);
            if let Found::Free(_) = found {
                let (since, _) = stamps();
                s.held = Some(Record {
                    owner: asking.owner.clone(),
                    purpose: purpose.clone(),
                    checkout: repo().display().to_string(),
                    since,
                    renewed: now(),
                    lease_minutes: lease,
                    backup: None,
                    folder: None,
                    player: asking.player,
                    taken: None,
                });
            }
            found
        })?;
        match found {
            Found::Held => {
                println!("already held: {}", held_by(&asking.owner)?.describe(now()));
                return Ok(0);
            }
            Found::Busy(who) if !asking.wait => {
                println!("busy: held by {who}");
                return Ok(EXIT_BUSY);
            }
            Found::Busy(who) => {
                if who != said {
                    println!("waiting: held by {who}");
                    said = who;
                }
                std::thread::sleep(WAIT_EVERY);
            }
            Found::Take(holder) => break (Some(holder), true),
            Found::Free(gone) => break (gone, false),
        }
    };
    let game_dir = args.game_dir()?;
    if let Some(holder) = gone {
        if took {
            println!("the player takes the game from {}: ending the game and releasing its claim", holder.describe(now()));
            game::kill();
            wait_closed()?;
            put_back(&holder, &game_dir, false)?;
            locked(|s| {
                if s.held.as_ref().is_some_and(|h| h.owner == holder.owner) {
                    s.held = None;
                }
            })?;
            // The game is free now, and the player first.
            return claim(args);
        }
        println!("took over from {}", holder.describe(now()));
        put_back(&holder, &game_dir, false)?;
    }
    if asking.player {
        println!("claimed for the player: {}", held_by(&asking.owner)?.describe(now()));
        return Ok(0);
    }
    match prepare(&asking.owner, &game_dir) {
        Ok(folder) => {
            let held = locked(|s| {
                let h = s.held.as_mut().filter(|h| h.owner == asking.owner)?;
                h.backup = Some(folder.join("backup").display().to_string());
                h.folder = Some(folder.display().to_string());
                h.renewed = now();
                Some(h.clone())
            })?
            .ok_or_else(|| format!("lost the claim while backing up; the backup is at {}", folder.display()))?;
            println!("claimed: {}", held.describe(now()));
            println!("the claim's folder: {}", folder.display());
            Ok(0)
        }
        Err(e) => {
            // No claim without its backup: the game is given back.
            locked(|s| {
                if s.held.as_ref().is_some_and(|h| h.owner == asking.owner) {
                    s.held = None;
                }
            })?;
            Err(format!("not claimed: {e}; the game is free again"))
        }
    }
}

/// Makes the claim's folder, with the player's files and the game folder's
/// framework files in it; removes the `kept\` of claims released long ago.
fn prepare(owner: &str, game_dir: &Path) -> Result<PathBuf> {
    let claims = base()?.join("claims");
    forget_old(&claims);
    let (_, stamp) = stamps();
    let folder = claims.join(format!("{owner}-{stamp}"));
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
    tool(&["backup", &folder.join("backup").display().to_string()], game_dir)?;
    Ok(folder)
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

/// Waits for `eu4.exe` to end, a minute at most.
fn wait_closed() -> Result<()> {
    for _ in 0..120 {
        if !game::running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err("eu4.exe did not end".into())
}

/// Puts back what a claim's run changed: copies the logs and the run's crash
/// folders into `kept\`, restores the player's files, and the game folder's
/// framework files as `framework\` has them. Returns whether the run
/// crashed. A file not put back fails it, and the claim's folder stays.
fn put_back(holder: &Record, game_dir: &Path, keep_crashes: bool) -> Result<bool> {
    let folder = holder.folder.as_ref().map(PathBuf::from);
    let user = eu4_harmony_manifest::user_dir(Some(game_dir))?;
    let mut crashed = false;
    if let Some(folder) = &folder {
        crashed = keep(folder, &user, holder.backup.as_deref().map(Path::new));
    }
    if let Some(backup) = holder.backup.as_deref().filter(|b| Path::new(b).is_dir()) {
        let mut words = vec!["restore", backup];
        if keep_crashes {
            words.push("--keep-crashes");
        }
        tool(&words, game_dir)?;
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
    let me = owner(args);
    let held = held_by(&me)?;
    let game_dir = args.game_dir()?;
    if game::running() {
        if held.player {
            return Err("eu4.exe is running; close it first".into());
        }
        println!("ending the game");
        game::kill();
        wait_closed()?;
    }
    let crashed = put_back(&held, &game_dir, args.flag("keep-crashes"))?;
    locked(|s| {
        if s.held.as_ref().is_some_and(|h| h.owner == me) {
            s.held = None;
        }
    })?;
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
    let me = owner(args);
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
        let running = game::running();
        let held = locked(|s| {
            let h = s.held.as_mut().filter(|h| h.owner == me)?;
            if running {
                h.renewed = now();
            }
            Some(h.clone())
        })?;
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
    let running = game::running();
    let (held, mut waiting) = locked(|s| (s.held.clone(), s.waiting.clone()))?;
    record::tidy(&mut waiting, now());
    match &held {
        Some(h) => {
            println!("held by {}", h.describe(now()));
            if let Some(folder) = &h.folder {
                println!("  the claim's folder: {folder}");
            }
            if h.stale(now(), running) {
                println!("  the lease has run out: the next claim takes over");
            }
            if let Some(taken) = h.taken {
                println!("  the player takes the game in {} s", (taken + record::TAKEN_GRACE_SECONDS).saturating_sub(now()));
            }
        }
        None => println!("free"),
    }
    for (i, w) in waiting.iter().enumerate() {
        println!("waiting {}: {}{}", i + 1, w.owner, if w.player { " (the player)" } else { "" });
    }
    println!("eu4.exe running: {running}");
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
    needs_claim(args)?;
    renew(args)?;
    let game_dir = args.game_dir()?;
    let timeout = args.value("timeout").unwrap_or("300").to_string();
    let mut words = vec!["start", "--hidden", "--timeout", &timeout];
    if sound {
        words.push("--sound");
    }
    let began = std::time::Instant::now();
    let started = tool(&words, &game_dir);
    if started.is_err() && game::running() {
        println!("ending the game, which did not reach its main menu");
        game::kill();
        wait_closed()?;
    }
    started.map(|()| began.elapsed().as_secs_f64())
}

fn start(args: &Args) -> Result<f64> {
    start_game(args, args.flag("sound"))
}

fn stop(args: &Args) -> Result<()> {
    needs_claim(args)?;
    game::kill();
    wait_closed()?;
    println!("the game is closed");
    Ok(())
}
