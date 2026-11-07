//! What the claim reaches of the computer, as one seam: the record under the
//! mutex that guards it, the games' processes and pipes, the claims'
//! folders, the clock, processes by their identity, and what the claim says.
//! [`ThisComputer`] is the real one; the claim's course (`course.rs`) is
//! tested over a recorded one.
//!
//! The record is `%LOCALAPPDATA%\eu4-harmony-dev\claims.json`; the named
//! mutex the claims of the whole game used guards every read and write of it
//! and of `game-lock.json`, their record, which it reads for a claim of a
//! checkout from before test games and writes [`record::old_holder`] into
//! while test games are held. Each claim has a folder under `claims\`
//! (docs/test-games.md): `user\`, the test game's user folder, made from the
//! player's with a junction to their `mod` folder; `core\`, the core and
//! `dev.toml` its start copied; `follow.exe`, which follows its games to
//! their ends into `game-ends.tsv`; and `kept\` after the release.
use super::record::{self, GameEnd, Process, State};
use crate::{built, game, repo, Result};
use eu4_harmony_client::start::Start;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The computer, as the claim reaches it.
pub trait Machine {
    /// Runs `action` on the record, holding the mutex that guards it, and
    /// writes back what it changed.
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
    /// Whether `p` runs still. One that cannot be looked at for another
    /// reason than its being gone runs, as far as the claim goes.
    fn alive(&self, p: &Process) -> bool;
    /// The ids of the `eu4.exe` that run; an error when they cannot be told.
    fn games(&self) -> Result<Vec<u32>>;
    /// The `eu4.exe` serving the pipe `name`, if any.
    fn serving(&self, name: &str) -> Option<u32>;
    /// Ends the game `id` and waits for it to end, a minute at most.
    fn end_game(&self, id: u32) -> Result<()>;

    /// Whether the proxy in `game_dir` loads a test game's core.
    fn proxy_loads_cores(&self, game_dir: &Path) -> bool;
    /// Makes the test game's user folder in the claim's `folder` from the
    /// player's user folder of `game_dir`; removes the `kept\` of claims
    /// released long ago.
    fn make_folder(&self, folder: &Path, game_dir: &Path) -> Result<()>;
    /// Copies this checkout's built core and `dev.toml` into the claim's
    /// `folder`, and the saves `saves` names from the player's; the core's
    /// path.
    fn prepare_start(&self, folder: &Path, game_dir: &Path, saves: &[String]) -> Result<PathBuf>;
    /// Starts the game from `game_dir` as `start` says, waiting up to `wait`
    /// for its main menu, `log` being the framework's log: its process id,
    /// and the seconds it took. A test game that does not reach it is ended.
    fn start_game(&self, game_dir: &Path, log: &Path, start: &Start, wait: Duration) -> Result<(u32, f64)>;
    /// Starts the claim's `follow.exe`, which follows the game `id` and the
    /// games the test game restarts in its place, serving the pipe `pipe`,
    /// to their ends.
    fn follow(&self, folder: &Path, id: u32, pipe: &str) -> Result<()>;
    /// Tells the claim's `follow.exe` that the claim is released, and waits
    /// for it to end, a few seconds at most.
    fn stop_following(&self, folder: &Path);
    /// How the claim's games ended, as its folder records them.
    fn ends(&self, folder: &Path) -> Vec<GameEnd>;
    /// Keeps the test game's logs and crash folders in the claim's `kept\`
    /// and removes the rest of its folder, the junction first; whether there
    /// was a crash folder.
    fn keep_and_remove(&self, folder: &Path) -> Result<bool>;
    /// Removes a claim's folder, if it is there, the junction first.
    fn remove_folder(&self, folder: &Path) -> Result<()>;
    /// The claims' folders no claim in `state` refers to that still have a
    /// test game's files, a line each.
    fn leftovers(&self, state: &State) -> Result<Vec<String>>;

    /// Says `line` to whoever runs the command.
    fn say(&self, line: &str);
}

/// The player's files a test game's user folder starts with, those of them
/// that are there.
const USER_FILES: [&str; 9] = [
    "settings.txt",
    "dlc_load.json",
    "dlc_signature",
    "game_data.json",
    "gameplaysettings.txt",
    "messagetypes_custom.txt",
    "pdx_settings.txt",
    "shortcuts.txt",
    "eu4-harmony\\settings.txt",
];
/// A `kept\` older than this goes at the next claim.
const KEEP_DAYS: u64 = 7;
/// The record of how a claim's games ended, a line each.
const ENDS: &str = "game-ends.tsv";
/// What the release leaves for `follow.exe`, which ends when it sees it.
const RELEASED: &str = "released";
/// How long `follow.exe` waits for the game a test game restarts in its
/// place to open the pipe.
const RESTART_WAIT: Duration = Duration::from_secs(90);

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
            return Err("cannot open the claims' mutex".into());
        }
        let waited = unsafe { WaitForSingleObject(mutex, INFINITE) };
        if waited != WAIT_OBJECT_0 && waited != WAIT_ABANDONED {
            unsafe { CloseHandle(mutex) };
            return Err("cannot take the claims' mutex".into());
        }
        let result = (|| {
            let (record, old) = (base.join("claims.json"), base.join("game-lock.json"));
            let old_record = read_json(&old);
            let mut state = read_json(&record).as_ref().map(State::from_json).unwrap_or_default();
            state.old = old_record.as_ref().and_then(record::old_claim);
            let before = state.clone();
            let value = action(&mut state);
            if state != before {
                write_json(&record, &state.to_json())?;
            }
            // The holder checkouts from before test games wait for, while
            // any is held; never over a claim of theirs.
            if state.old.is_none() {
                match (state.claims.is_empty(), old_record.is_some()) {
                    (false, false) => write_json(&old, &record::old_holder(self.now(), &self.stamps().0))?,
                    (true, true) => remove_file(&old)?,
                    _ => {}
                }
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
        process(id)
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
        eu4_harmony_client::game::processes()
    }

    fn serving(&self, name: &str) -> Option<u32> {
        serving(name)
    }

    fn end_game(&self, id: u32) -> Result<()> {
        game::kill_process(id);
        for _ in 0..120 {
            if !self.games()?.contains(&id) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(format!("eu4.exe {id} did not end"))
    }

    fn proxy_loads_cores(&self, game_dir: &Path) -> bool {
        eu4_harmony_client::start::knows(&game_dir.join("version.dll"), "-harmony_core")
    }

    fn make_folder(&self, folder: &Path, game_dir: &Path) -> Result<()> {
        forget_old(&self.claims()?);
        let player = eu4_harmony_folders::user_dir(Some(game_dir))?;
        make_user_folder(&player, &folder.join("user"))
    }

    fn prepare_start(&self, folder: &Path, game_dir: &Path, saves: &[String]) -> Result<PathBuf> {
        let core = folder.join("core");
        fs::create_dir_all(&core).map_err(|e| format!("cannot make {}: {e}", core.display()))?;
        let dll = core.join("eu4_harmony_core.dll");
        copy(&built().join("eu4_harmony_core.dll"), &dll)?;
        let options = repo().join("dev.toml");
        if options.is_file() {
            copy(&options, &core.join("dev.toml"))?;
        } else {
            remove_file(&core.join("dev.toml"))?;
        }
        let player = eu4_harmony_folders::user_dir(Some(game_dir))?;
        copy_saves(&player.join("save games"), &folder.join("user").join("save games"), saves)?;
        Ok(dll)
    }

    fn start_game(&self, game_dir: &Path, log: &Path, start: &Start, wait: Duration) -> Result<(u32, f64)> {
        let began = std::time::Instant::now();
        let id = eu4_harmony_client::start::start(game_dir, log, start, wait)?;
        Ok((id, began.elapsed().as_secs_f64()))
    }

    fn follow(&self, folder: &Path, id: u32, pipe: &str) -> Result<()> {
        // One follower a claim: the one of a game that ended may still be
        // looking for the game a restart starts, and would follow this one
        // too.
        self.stop_following(folder);
        let follower = folder.join("follow.exe");
        let _ = fs::remove_file(folder.join(RELEASED));
        let me = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
        if fs::read(&follower).ok() != fs::read(&me).ok() {
            copy(&me, &follower)?;
        }
        let words = ["game".to_string(), "follow".into(), folder.display().to_string(), id.to_string(), pipe.to_string()];
        spawn_detached(&follower, &words)
    }

    fn stop_following(&self, folder: &Path) {
        let _ = fs::write(folder.join(RELEASED), "");
        let follower = folder.join("follow.exe");
        for _ in 0..30 {
            // A program that runs cannot be removed.
            if !follower.exists() || fs::remove_file(&follower).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn ends(&self, folder: &Path) -> Vec<GameEnd> {
        ends_in(folder)
    }

    fn keep_and_remove(&self, folder: &Path) -> Result<bool> {
        let crashed = keep(folder);
        self.remove_rest(folder)?;
        Ok(crashed)
    }

    fn remove_folder(&self, folder: &Path) -> Result<()> {
        if !folder.exists() {
            return Ok(());
        }
        unlink_mods(&folder.join("user"))?;
        fs::remove_dir_all(folder).map_err(|e| format!("cannot remove {}: {e}", folder.display()))
    }

    fn leftovers(&self, state: &State) -> Result<Vec<String>> {
        let mut lines = Vec::new();
        if let Ok(entries) = fs::read_dir(self.claims()?) {
            for entry in entries.flatten() {
                let path = entry.path();
                let held = state.claims.iter().any(|c| Path::new(&c.folder) == path);
                if !held && (path.join("user").exists() || path.join("core").exists()) {
                    lines.push(format!("a claim's folder no claim refers to, with a test game's files: {}", path.display()));
                }
            }
        }
        Ok(lines)
    }

    fn say(&self, line: &str) {
        println!("{line}");
    }
}

impl ThisComputer {
    /// Removes what the release does not keep of the claim's folder.
    fn remove_rest(&self, folder: &Path) -> Result<()> {
        let user = folder.join("user");
        unlink_mods(&user)?;
        for dir in [user, folder.join("core")] {
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
            }
        }
        for file in ["follow.exe", RELEASED, ENDS] {
            let _ = fs::remove_file(folder.join(file));
        }
        Ok(())
    }
}

/// The process `id` is now, with when it started.
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

/// The `eu4.exe` serving the pipe `name`, if any.
fn serving(name: &str) -> Option<u32> {
    use eu4_harmony_client::pipe;
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    pipe::connect(name, pipe::GAME, deadline).ok()?.process().ok()
}

/// The folder of the record and the claims' folders:
/// `%LOCALAPPDATA%\eu4-harmony-dev`, or the one `EU4_HARMONY_DEV_DIR` names,
/// for trying the claims apart from the computer's own record.
fn base() -> Result<PathBuf> {
    let base = match std::env::var_os("EU4_HARMONY_DEV_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => backslashed(Path::new(&dir)),
        None => PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?).join("eu4-harmony-dev"),
    };
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

fn copy(from: &Path, to: &Path) -> Result<()> {
    fs::copy(from, to).map(drop).map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))
}

/// Makes a test game's user folder `user` from the player's `player`: the
/// files it starts with, copied, and `mod` a junction to theirs.
fn make_user_folder(player: &Path, user: &Path) -> Result<()> {
    fs::create_dir_all(user.join("eu4-harmony")).map_err(|e| format!("cannot make {}: {e}", user.display()))?;
    for file in USER_FILES {
        let from = player.join(file);
        if from.is_file() {
            copy(&from, &user.join(file))?;
        }
    }
    let mods = player.join("mod");
    if mods.is_dir() {
        junction(&user.join("mod"), &mods)?;
    }
    Ok(())
}

/// Makes `link` a junction to the folder `target`.
fn junction(link: &Path, target: &Path) -> Result<()> {
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
    let made = std::process::Command::new(system.join("cmd.exe"))
        .arg("/c")
        .arg("mklink")
        .arg("/J")
        .arg(backslashed(link))
        .arg(backslashed(target))
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("cannot run mklink: {e}"))?;
    made.success().then_some(()).ok_or_else(|| format!("cannot make the junction {} to {}", link.display(), target.display()))
}

/// `path` with Windows' own separators, which `mklink` and the game's
/// command line read.
fn backslashed(path: &Path) -> PathBuf {
    PathBuf::from(path.display().to_string().replace('/', "\\"))
}

/// Removes the junction `user\mod`, never what it leads to, before the
/// folder is removed.
fn unlink_mods(user: &Path) -> Result<()> {
    let link = user.join("mod");
    match fs::symlink_metadata(&link) {
        Ok(m) if m.file_type().is_symlink() || is_junction(&link) => {
            fs::remove_dir(&link).map_err(|e| format!("cannot remove the junction {}: {e}", link.display()))
        }
        Ok(_) => Err(format!("{} is no junction; nothing of the claim's folder was removed", link.display())),
        Err(_) => Ok(()),
    }
}

/// Whether `path` is a reparse point, as a junction is.
fn is_junction(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// Copies each save of `saves`, by its name as `menu.saves` gives it or its
/// file's, from `from` into `to`.
fn copy_saves(from: &Path, to: &Path, saves: &[String]) -> Result<()> {
    if saves.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(to).map_err(|e| format!("cannot make {}: {e}", to.display()))?;
    for save in saves {
        let file = [from.join(save), from.join(format!("{save}.eu4"))].into_iter().find(|f| f.is_file());
        let file = file.ok_or_else(|| format!("no save {save:?} in {}", from.display()))?;
        copy(&file, &to.join(file.file_name().expect("a file")))?;
    }
    Ok(())
}

/// Removes the claims' folders whose `kept\` is older than [`KEEP_DAYS`]
/// and that hold no test game's files.
fn forget_old(claims: &Path) {
    let Ok(entries) = fs::read_dir(claims) else { return };
    let limit = Duration::from_secs(KEEP_DAYS * 24 * 3600);
    for entry in entries.flatten() {
        let path = entry.path();
        let only_kept = path.join("kept").is_dir() && !path.join("user").exists() && !path.join("core").exists();
        let old = fs::metadata(path.join("kept")).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > limit);
        if only_kept && old {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Copies the test game's framework logs, game logs and crash folders, and
/// the record of its games' ends, into the claim's `kept\`; whether there
/// was a crash folder.
fn keep(folder: &Path) -> bool {
    let (user, kept) = (folder.join("user"), folder.join("kept"));
    let _ = copy_tree(&user.join("eu4-harmony").join("logs"), &kept.join("eu4-harmony-logs"));
    let _ = copy_tree(&user.join("logs"), &kept.join("game-logs"));
    let _ = copy_tree(&user.join("crashes"), &kept.join("crashes"));
    if folder.join(ENDS).is_file() {
        let _ = fs::create_dir_all(&kept).and_then(|()| fs::copy(folder.join(ENDS), kept.join(ENDS)));
    }
    fs::read_dir(user.join("crashes")).is_ok_and(|mut entries| entries.any(|e| e.is_ok_and(|e| e.path().is_dir())))
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

/// How the games of the claim in `folder` ended, as `follow.exe` wrote them.
fn ends_in(folder: &Path) -> Vec<GameEnd> {
    fs::read_to_string(folder.join(ENDS)).map(|text| text.lines().filter_map(GameEnd::from_line).collect()).unwrap_or_default()
}

/// Starts `program` with `words`, with no console and none of this
/// process's handles: a handle of a pipe this command's output goes to would
/// keep whoever reads it waiting until the program ends.
fn spawn_detached(program: &Path, words: &[String]) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION, STARTUPINFOW},
    };
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let quoted = std::iter::once(program.display().to_string()).chain(words.iter().cloned()).map(|w| format!("\"{w}\"")).collect::<Vec<_>>().join(" ");
    let (mut line, exe) = (wide(&quoted), wide(&program.display().to_string()));
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let made = unsafe {
        CreateProcessW(
            exe.as_ptr(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            DETACHED_PROCESS,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut info,
        )
    };
    if made == 0 {
        return Err(format!("cannot start {}: {}", program.display(), std::io::Error::last_os_error()));
    }
    unsafe {
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
    }
    Ok(())
}

/// `follow.exe`'s work, `cargo xtask game follow <folder> <id> <pipe>`:
/// follows the game `id` to its end and writes how it ended into the
/// claim's `game-ends.tsv`; then, when it ended with 0, as a restart does,
/// and unless the claim is released, follows the game the test game
/// restarts in its place, the one that opens its pipe within
/// [`RESTART_WAIT`].
pub fn follow_games(folder: &Path, first: u32, pipe: &str) -> Result<()> {
    use std::io::Write;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::{
            SystemInformation::GetLocalTime,
            Threading::{GetExitCodeProcess, OpenProcess, WaitForSingleObject, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE},
        },
    };
    let mut next = Some(first);
    while let Some(id) = next.take() {
        let Some(game) = process(id) else { return Ok(()) };
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, id) };
        if handle.is_null() {
            return Ok(());
        }
        let mut code = 0;
        let ended = unsafe { WaitForSingleObject(handle, INFINITE) == WAIT_OBJECT_0 && GetExitCodeProcess(handle, &mut code) != 0 };
        unsafe { CloseHandle(handle) };
        if !ended {
            return Ok(());
        }
        let seen = unsafe {
            let mut t = std::mem::zeroed();
            GetLocalTime(&mut t);
            format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
        };
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(folder.join(ENDS)) {
            let _ = writeln!(file, "{}", GameEnd { game, seen, code }.to_line());
        }
        // A restart ends the game with 0; one ended by these commands, or a
        // crash, starts nothing in its place.
        if code != 0 {
            break;
        }
        let began = std::time::Instant::now();
        while began.elapsed() < RESTART_WAIT && !folder.join(RELEASED).exists() {
            if let Some(again) = serving(pipe).filter(|&again| again != id) {
                next = Some(again);
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for a test, empty, which goes when it is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("eu4h-xtask-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, path: &str, text: &str) -> PathBuf {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = unlink_mods(&self.0.join("claim").join("user"));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// The test game's user folder starts with the player's settings and
    /// playset and reaches their mods through a junction; the release keeps
    /// its logs and crash folders and removes the rest, the player's mods
    /// untouched.
    #[test]
    fn a_test_game_s_folder_is_made_from_the_player_s_and_removed_leaving_theirs() {
        let s = Scratch::new("folder");
        s.write("player\\settings.txt", "the player's settings");
        s.write("player\\dlc_load.json", "the playset");
        s.write("player\\eu4-harmony\\settings.txt", "the language");
        s.write("player\\mod\\a.mod", "a descriptor");
        s.write("player\\save games\\England-1.eu4", "a save");
        s.write("player\\save games\\other.eu4", "another");
        s.write("player\\pdx_login_cache.txt", "not copied");
        let (player, folder) = (s.0.join("player"), s.0.join("claim"));
        make_user_folder(&player, &folder.join("user")).unwrap();
        let user = folder.join("user");
        assert_eq!(fs::read_to_string(user.join("settings.txt")).unwrap(), "the player's settings");
        assert_eq!(fs::read_to_string(user.join("eu4-harmony\\settings.txt")).unwrap(), "the language");
        assert_eq!(fs::read_to_string(user.join("mod\\a.mod")).unwrap(), "a descriptor");
        assert!(!user.join("pdx_login_cache.txt").exists());
        copy_saves(&player.join("save games"), &user.join("save games"), &["England-1".into()]).unwrap();
        assert!(user.join("save games\\England-1.eu4").is_file() && !user.join("save games\\other.eu4").exists());
        assert!(copy_saves(&player.join("save games"), &user.join("save games"), &["none".into()]).unwrap_err().contains("no save \"none\""));

        s.write("claim\\user\\eu4-harmony\\logs\\core.log", "the core's log");
        s.write("claim\\user\\crashes\\eu4_1\\exception.txt", "a crash");
        s.write("claim\\core\\eu4_harmony_core.dll", "the core");
        s.write("claim\\game-ends.tsv", "1\t2\t03:00:00\t0x1\n");
        assert!(ThisComputer.keep_and_remove(&folder).unwrap());
        assert!(!user.exists() && !folder.join("core").exists());
        let kept = folder.join("kept");
        assert_eq!(fs::read_to_string(kept.join("eu4-harmony-logs\\core.log")).unwrap(), "the core's log");
        assert!(kept.join("crashes\\eu4_1\\exception.txt").is_file());
        assert_eq!(fs::read_to_string(kept.join("game-ends.tsv")).unwrap(), "1\t2\t03:00:00\t0x1\n");
        assert_eq!(fs::read_to_string(player.join("mod\\a.mod")).unwrap(), "a descriptor");
    }

    /// A `mod` that is a folder and no junction stops the removal: it might
    /// be the player's.
    #[test]
    fn a_mod_folder_that_is_no_junction_is_never_removed() {
        let s = Scratch::new("no-junction");
        s.write("claim\\user\\mod\\a.mod", "a descriptor");
        assert!(ThisComputer.remove_folder(&s.0.join("claim")).unwrap_err().contains("no junction"));
        assert!(s.0.join("claim\\user\\mod\\a.mod").is_file());
    }
}
