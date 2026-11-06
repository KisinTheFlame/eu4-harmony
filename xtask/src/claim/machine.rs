//! What the claim reaches of the computer, as one seam: the record and the
//! queue under the mutex that guards them, the game's processes, the
//! player's files and the game folder's framework files backed up and put
//! back, the clock, processes by their identity, and what the claim says.
//! [`ThisComputer`] is the real one; the claim's course (`course.rs`) is
//! tested over a recorded one.
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
use super::record::{Process, Record, Waiting};
use crate::{game, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The record and the queue, as the mutex guards them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub held: Option<Record>,
    pub waiting: Vec<Waiting>,
}

/// What putting back the game folder's framework files came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Framework {
    /// The claim's folder has no copy of them: nothing to put back.
    NoCopy,
    /// Each is as it was, and the copy is gone.
    PutBack,
    /// These are not as they were, a line each; the copy stays.
    Lost(Vec<String>),
}

/// The computer, as the claim reaches it.
pub trait Machine {
    /// Runs `action` on the record and the queue, holding the mutex that
    /// guards them, and writes back what it changed.
    fn locked<T>(&self, action: impl FnOnce(&mut State) -> T) -> Result<T>;
    /// The folder that holds the claims' folders.
    fn claims(&self) -> Result<PathBuf>;

    /// Seconds since 1970.
    fn now(&self) -> u64;
    /// The local date and time: `yyyy-MM-dd HH:mm`, and `yyyyMMdd-HHmmss`
    /// for a folder's name.
    fn stamps(&self) -> (String, String);
    /// Waits before looking again.
    fn wait(&self, time: Duration);

    /// This process.
    fn me(&self) -> Process;
    /// The process `id` is now, with when it started; none when no process
    /// has that id.
    fn process(&self, id: u32) -> Option<Process>;
    /// Whether `p` runs still: a process has its id, started when it did,
    /// and has not ended. One that cannot be looked at for another reason
    /// than its being gone runs, as far as the claim goes.
    fn alive(&self, p: &Process) -> bool;

    /// The ids of the `eu4.exe` that run; an error when they cannot be told.
    fn games(&self) -> Result<Vec<u32>>;
    /// Ends every `eu4.exe` at once.
    fn kill_game(&self);
    /// Waits for `eu4.exe` to end, a minute at most.
    fn wait_closed(&self) -> Result<()>;

    /// Makes the claim's `folder`, with the player's files and the game
    /// folder's framework files in it; removes the `kept\` of claims
    /// released long ago.
    fn back_up(&self, folder: &Path, game_dir: &Path) -> Result<()>;
    /// Removes a claim's folder, if it is there.
    fn remove_folder(&self, folder: &Path) -> Result<()>;
    /// Copies the framework's logs, the game's logs and the crash folders
    /// `backup` did not know into the claim's `kept\`, when it has a folder;
    /// whether there was a crash.
    fn keep_logs(&self, folder: Option<&Path>, backup: Option<&Path>, game_dir: &Path) -> Result<bool>;
    /// Restores the player's files from `backup`, if it is there, removing
    /// the run's crash folders unless `keep_crashes`.
    fn restore_player_files(&self, backup: &Path, game_dir: &Path, keep_crashes: bool) -> Result<()>;
    /// Puts the game folder's framework files back as the claim's `folder`
    /// has them, and compares each.
    fn restore_framework_files(&self, folder: &Path, game_dir: &Path) -> Result<Framework>;
    /// When a claim's folder was made, in seconds since 1970.
    fn made_at(&self, folder: &Path) -> Option<u64>;
    /// The claims' folders and the backups no claim refers to, a line each,
    /// besides `held`'s.
    fn leftovers(&self, held: Option<&Record>) -> Result<Vec<String>>;

    /// Says `line` to whoever runs the command.
    fn say(&self, line: &str);
}

/// The files of the game folder a run may change, by their path in it.
const FRAMEWORK_FILES: [&str; 5] =
    ["version.dll", "eu4-harmony\\eu4_harmony_core.dll", "eu4-harmony\\eu4-harmony.exe", "eu4-harmony\\dev.toml", "eu4-harmony\\backup\\version.dll"];
/// A `kept\` older than this goes at the next claim.
const KEEP_DAYS: u64 = 7;

/// This computer.
pub struct ThisComputer;

impl Machine for ThisComputer {
    fn locked<T>(&self, action: impl FnOnce(&mut State) -> T) -> Result<T> {
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
            let (record, queue) = (base.join("game-lock.json"), base.join("game-queue.json"));
            let mut state = State {
                held: read_json(&record).as_ref().and_then(Record::from_json),
                waiting: read_json(&queue).and_then(|v| v.as_array().map(|a| a.iter().filter_map(Waiting::from_json).collect())).unwrap_or_default(),
            };
            let before = state.clone();
            let value = action(&mut state);
            if state.held != before.held {
                match &state.held {
                    Some(h) => write_json(&record, &h.to_json())?,
                    None => remove_file(&record)?,
                }
            }
            if state.waiting != before.waiting {
                write_json(&queue, &serde_json::Value::Array(state.waiting.iter().map(Waiting::to_json).collect()))?;
            }
            Ok(value)
        })();
        unsafe {
            ReleaseMutex(mutex);
            CloseHandle(mutex);
        }
        result
    }

    fn claims(&self) -> Result<PathBuf> {
        Ok(base()?.join("claims"))
    }

    fn now(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
    }

    fn stamps(&self) -> (String, String) {
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

    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }

    fn me(&self) -> Process {
        self.process(std::process::id()).unwrap_or(Process { id: std::process::id(), started: 0 })
    }

    fn process(&self, id: u32) -> Option<Process> {
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

    fn alive(&self, p: &Process) -> bool {
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

    fn games(&self) -> Result<Vec<u32>> {
        eu4_harmony_client::game_processes()
    }

    fn kill_game(&self) {
        game::kill();
    }

    fn wait_closed(&self) -> Result<()> {
        game::wait_closed()
    }

    fn back_up(&self, folder: &Path, game_dir: &Path) -> Result<()> {
        forget_old(&self.claims()?);
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

    fn remove_folder(&self, folder: &Path) -> Result<()> {
        if !folder.exists() {
            return Ok(());
        }
        fs::remove_dir_all(folder).map_err(|e| format!("cannot remove {}: {e}", folder.display()))
    }

    fn keep_logs(&self, folder: Option<&Path>, backup: Option<&Path>, game_dir: &Path) -> Result<bool> {
        let user = eu4_harmony_manifest::user_dir(Some(game_dir))?;
        Ok(folder.is_some_and(|folder| keep(folder, &user, backup)))
    }

    fn restore_player_files(&self, backup: &Path, game_dir: &Path, keep_crashes: bool) -> Result<()> {
        if !backup.is_dir() {
            return Ok(());
        }
        eu4_harmony_client::game_not_running()?;
        eu4_harmony_client::backup::restore(&user_folder(game_dir)?, backup, false, keep_crashes)
    }

    fn restore_framework_files(&self, folder: &Path, game_dir: &Path) -> Result<Framework> {
        let framework = folder.join("framework");
        if !framework.is_dir() {
            return Ok(Framework::NoCopy);
        }
        let lost = restore_framework(&framework, game_dir);
        if !lost.is_empty() {
            return Ok(Framework::Lost(lost));
        }
        fs::remove_dir_all(&framework).map_err(|e| format!("cannot remove {}: {e}", framework.display()))?;
        Ok(Framework::PutBack)
    }

    fn made_at(&self, folder: &Path) -> Option<u64> {
        let made = fs::metadata(folder).ok()?.created().ok()?;
        made.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
    }

    fn leftovers(&self, held: Option<&Record>) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        if let Ok(entries) = fs::read_dir(self.claims()?) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ours = held.and_then(|h| h.folder.as_deref()).is_some_and(|f| Path::new(f) == path);
                if !ours && (path.join("backup").exists() || path.join("framework").exists()) {
                    lines.push(format!("a claim's folder no claim refers to, not put back: {}", path.display()));
                }
            }
        }
        if let Ok(entries) = fs::read_dir(base()?.join("backups")) {
            for entry in entries.flatten() {
                let path = entry.path().display().to_string();
                if held.and_then(|h| h.backup.as_deref()) != Some(path.as_str()) {
                    lines.push(format!("a backup no claim refers to: {path}"));
                }
            }
        }
        Ok(lines)
    }

    fn say(&self, line: &str) {
        println!("{line}");
    }
}

fn base() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let base = PathBuf::from(local).join("eu4-harmony-dev");
    fs::create_dir_all(&base).map_err(|e| format!("cannot make {}: {e}", base.display()))?;
    Ok(base)
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
