//! What the claim reaches of the computer, as one seam: the record under the
//! mutex that guards it, the games' processes and pipes, the claims'
//! folders, the clock, processes by their identity, and what the claim says.
//! [`ThisComputer`] is the real one; the claim's course (`course.rs`) is
//! tested over a recorded one.
//!
//! The record is `%LOCALAPPDATA%\eu4-harmony-dev\claims.json`; the named
//! mutex the claims of the whole game used, which keeps its name so that
//! checkouts a little behind share it still, guards every read and write of
//! it. Each claim has a folder under `claims\`
//! (docs/dev-tools.md#test-games): `user\`, a pure synthetic user folder
//! with clean settings, an empty playset and a private `mod` folder; `core\`,
//! the core and `dev.toml` its start copied; `follow.exe`, which follows its
//! games to their ends into `game-ends.tsv`; and `kept\` after the release.
use super::course::Starting;
use super::record::{GameEnd, Process, State};
use crate::{built, repo, Result};
use eu4_harmony_client::game::{self, Game};
use eu4_harmony_client::start::{Failed, Start};
use eu4_harmony_folders::UserFolder;
use eu4_harmony_manifest::{Descriptor, Playset};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// An owned handle to a newly started test game, carrying its identity and, on
/// a real machine, holding the OS process open until `follow.exe` takes over.
pub trait StartedGame {
    fn process(&self) -> Process;
}

impl StartedGame for Game {
    fn process(&self) -> Process {
        Process { id: self.id(), started: self.started() }
    }
}

impl StartedGame for Process {
    fn process(&self) -> Process {
        *self
    }
}

/// State of `version.dll` in the game folder before claiming a test game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyStatus {
    Ready,
    Missing,
    Outdated,
}

/// The computer, as the claim reaches it.
pub trait Machine {
    type Started: StartedGame;

    /// Runs `action` on the record, holding the mutex that guards it, and
    /// writes back what it changed.
    fn locked<T>(&self, action: impl FnOnce(&mut State) -> T) -> Result<T>;
    /// The folder that holds the claims' folders.
    fn claims(&self) -> Result<PathBuf>;

    /// The local date and time: `yyyy-MM-dd HH:mm`, and `yyyyMMdd-HHmmss`
    /// for a folder's name.
    fn stamps(&self) -> (String, String);
    /// Waits before looking again.
    fn wait(&self, time: Duration);

    /// This process.
    fn me(&self) -> Process;
    /// Whether `p` runs still. One that cannot be looked at for another
    /// reason than its being gone runs, as far as the claim goes.
    fn alive(&self, p: &Process) -> bool;
    /// The ids of the `eu4.exe` that run; an error when they cannot be told.
    fn games(&self) -> Result<Vec<u32>>;
    /// The `eu4.exe` serving the pipe `name`, if any.
    fn serving(&self, name: &str) -> Option<u32>;
    /// Ends the game `id` and waits for it to end, a minute at most.
    fn end_game(&self, id: u32) -> Result<()>;

    /// State of the framework proxy in `game_dir`.
    fn proxy_status(&self, game_dir: &Path) -> ProxyStatus;
    /// Makes the test game's pure synthetic user folder in the claim's
    /// `folder`; removes the `kept\` of claims released long ago.
    fn make_folder(&self, folder: &Path) -> Result<()>;
    /// Copies this checkout's built core and `dev.toml`, or the options
    /// `starting` gives in its place, into the claim's `folder`, and the
    /// saves it names from explicit paths or `%LOCALAPPDATA%\eu4-harmony-dev\saves\`;
    /// the core's path.
    fn prepare_start(&self, folder: &Path, starting: &Starting) -> Result<PathBuf>;
    /// Starts the game from `game_dir` as `start` says, waiting up to `wait`
    /// for its main menu, `log` being the framework's log: its owned game
    /// handle and the seconds it took. A test game that does not reach it is ended.
    fn start_game(&self, game_dir: &Path, log: &Path, start: &Start, wait: Duration) -> std::result::Result<(Self::Started, f64), Failed>;
    /// Starts the claim's `follow.exe`, holding `started` open until the
    /// follower holds the process too, so concurrent scenario starts never
    /// clobber one another's handle or lose an early exit code.
    fn follow(&self, folder: &Path, started: Self::Started, pipe: &str) -> Result<()>;
    /// Tells the claim's `follow.exe` that the claim is released, and waits
    /// for it to end, a few seconds at most.
    fn stop_following(&self, folder: &Path);
    /// How the claim's games ended, as its folder records them.
    fn ends(&self, folder: &Path) -> Vec<GameEnd>;
    /// Records in the claim's `folder` that `game` ended with `code`, seen
    /// now: a game that ended before its main menu, which no `follow.exe`
    /// followed.
    fn record_end(&self, folder: &Path, game: Process, code: u32);
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

/// Minimal clean game settings for isolated test games (1920x1080 borderless
/// English with compressed autosaves; never fullscreen so hidden starts work).
const CLEAN_GAME_SETTINGS: &str = "\
force_pow2_textures=no
language=\"l_english\"
compress_autosave=yes
graphics={
\tsize={
\t\tx=1920
\t\ty=1080
\t}
\tmin_gui={
\t\tx=1280
\t\ty=720
\t}
\trefreshRate=60
\tfullScreen=no
\tborderless=yes
}
";

/// Default framework settings matching `core/src/settings.rs`, ensuring
/// `eu4-harmony/settings.txt` always exists from the first start.
const CLEAN_HARMONY_SETTINGS: &str = "\
language=\"english\"
map_outline=yes
return_without_restart=yes
save_ironman_before_load=yes
achievements_outside_ironman=no
achievements_with_changed_checksum=no
music_loudness=yes
music_leveling=no
";
/// A `kept\` older than this goes at the next claim.
const KEEP_DAYS: u64 = 7;
/// The record of how a claim's games ended, a line each.
const ENDS: &str = "game-ends.tsv";
/// What the release leaves for `follow.exe`, which ends when it sees it.
const RELEASED: &str = "released";
/// How long `follow.exe` waits for the game a test game restarts in its
/// place to open the pipe.
const RESTART_WAIT: Duration = Duration::from_secs(90);
/// What `follow.exe` leaves once it holds the game it follows.
const FOLLOWING: &str = "following";
/// How long a start waits for `follow.exe` to hold the game.
const TAKING_OVER: Duration = Duration::from_secs(10);

/// This computer.
pub struct ThisComputer;

impl Machine for ThisComputer {
    type Started = Game;
    fn locked<T>(&self, action: impl FnOnce(&mut State) -> T) -> Result<T> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0},
            System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject, INFINITE},
        };
        let base = crate::dev_dir()?;
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
            let record = base.join("claims.json");
            let mut state = read_json(&record).as_ref().map(State::from_json).unwrap_or_default();
            let before = state.clone();
            let value = action(&mut state);
            if state != before {
                write_json(&record, &state.to_json())?;
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
        Ok(crate::dev_dir()?.join("claims"))
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
        process(std::process::id()).unwrap_or(Process { id: std::process::id(), started: 0 })
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
        game::end_game(id)
    }

    fn proxy_status(&self, game_dir: &Path) -> ProxyStatus {
        let proxy = game_dir.join("version.dll");
        if !proxy.is_file() {
            ProxyStatus::Missing
        } else if eu4_harmony_client::start::knows(&proxy, "-harmony_core") {
            ProxyStatus::Ready
        } else {
            ProxyStatus::Outdated
        }
    }

    fn make_folder(&self, folder: &Path) -> Result<()> {
        forget_old(&self.claims()?);
        make_user_folder(&folder.join("user"))
    }

    fn prepare_start(&self, folder: &Path, starting: &Starting) -> Result<PathBuf> {
        let core = folder.join("core");
        fs::create_dir_all(&core).map_err(|e| format!("cannot make {}: {e}", core.display()))?;
        let dll = core.join("eu4_harmony_core.dll");
        let build = starting.build.clone().unwrap_or_else(built);
        copy(&build.join("eu4_harmony_core.dll"), &dll)?;
        for helper in ["eu4_harmony_crash_dumps.dll", "eu4-harmony-dump-writer.exe"] {
            if build.join(helper).is_file() {
                copy(&build.join(helper), &core.join(helper))?;
            } else {
                remove_file(&core.join(helper))?;
            }
        }
        let options = repo().join("dev.toml");
        if let Some(text) = &starting.options {
            fs::write(core.join("dev.toml"), text).map_err(|e| format!("cannot write {}: {e}", core.join("dev.toml").display()))?;
        } else if options.is_file() {
            copy(&options, &core.join("dev.toml"))?;
        } else {
            remove_file(&core.join("dev.toml"))?;
        }
        let dev_saves = crate::saves::saves_dir()?;
        copy_saves(&dev_saves, &UserFolder::new(folder.join("user")).save_games(), &starting.saves)?;
        place_test_mods(folder, &starting.mods)?;
        Ok(dll)
    }

    fn start_game(&self, game_dir: &Path, log: &Path, start: &Start, wait: Duration) -> std::result::Result<(Game, f64), Failed> {
        let began = std::time::Instant::now();
        let game = eu4_harmony_client::start::start(game_dir, log, start, wait)?;
        Ok((game, began.elapsed().as_secs_f64()))
    }

    fn follow(&self, folder: &Path, started: Game, pipe: &str) -> Result<()> {
        let id = started.id();
        // One follower a claim: the one of a game that ended may still be
        // looking for the game a restart starts, and would follow this one
        // too.
        self.stop_following(folder);
        let follower = folder.join("follow.exe");
        let _ = fs::remove_file(folder.join(RELEASED));
        let me = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
        let same_binary = match (fs::metadata(&follower), fs::metadata(&me)) {
            (Ok(f), Ok(m)) => f.len() == m.len() && f.modified().ok() == m.modified().ok(),
            _ => false,
        };
        if !same_binary {
            copy(&me, &follower)?;
        }
        let _ = fs::remove_file(folder.join(FOLLOWING));
        let words = ["game".to_string(), "follow".into(), folder.display().to_string(), id.to_string(), pipe.to_string()];
        spawn_detached(&follower, &words)?;
        // Hold this start's `Game` handle locally until `follow.exe` holds the
        // process too, without sharing a process-wide slot across workers.
        let began = std::time::Instant::now();
        while began.elapsed() < TAKING_OVER && !folder.join(FOLLOWING).exists() {
            std::thread::sleep(Duration::from_millis(50));
        }
        drop(started);
        Ok(())
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

    fn record_end(&self, folder: &Path, game: Process, code: u32) {
        write_end(folder, &GameEnd { game, seen: local_time(), code });
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
        eu4_harmony_folders::windows_crash_callback::forget(&folder.join("core"))?;
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
        eu4_harmony_folders::windows_crash_callback::forget(&folder.join("core"))?;
        unlink_mods(&user)?;
        for dir in [user, folder.join("core")] {
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
            }
        }
        for file in ["follow.exe", RELEASED, ENDS, FOLLOWING] {
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

/// Makes a pure synthetic user folder `user` with clean settings, an empty
/// playset and a private marked `mod` folder, reading nothing from the host
/// player's user folder.
fn make_user_folder(user: &Path) -> Result<()> {
    let harmony = user.join("eu4-harmony");
    fs::create_dir_all(&harmony).map_err(|e| format!("cannot make {}: {e}", user.display()))?;
    fs::write(user.join("settings.txt"), CLEAN_GAME_SETTINGS).map_err(|e| format!("cannot write {}: {e}", user.join("settings.txt").display()))?;
    fs::write(harmony.join("settings.txt"), CLEAN_HARMONY_SETTINGS).map_err(|e| format!("cannot write {}: {e}", harmony.join("settings.txt").display()))?;
    Playset::default().write(&user.join("dlc_load.json"))?;
    init_private_mods(&user.join("mod"))?;
    Ok(())
}

/// Ownership marker inside a claim's private `user\mod` directory, so removal
/// refuses any non-claim `mod` folder that is not an older claim's junction.
const CLAIM_MODS_MARKER: &str = ".harmony-claim-mods";

fn init_private_mods(mods: &Path) -> Result<PathBuf> {
    fs::create_dir_all(mods).map_err(|e| format!("cannot make {}: {e}", mods.display()))?;
    let marker = mods.join(CLAIM_MODS_MARKER);
    fs::write(&marker, "").map_err(|e| format!("cannot write {}: {e}", marker.display()))?;
    Ok(mods.to_path_buf())
}

fn plain_path(path: &Path) -> PathBuf {
    match path.canonicalize() {
        Ok(full) => {
            let text = full.to_string_lossy();
            PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_string())
        }
        Err(_) => path.to_path_buf(),
    }
}

/// Unlinks an older claim's `user\mod` junction or verifies that a plain `mod`
/// directory carries [`CLAIM_MODS_MARKER`], refusing to touch any unmarked
/// `mod` folder before removal.
fn unlink_mods(user: &Path) -> Result<()> {
    let link = user.join("mod");
    match fs::symlink_metadata(&link) {
        Ok(m) if m.file_type().is_symlink() || is_junction(&link) => {
            fs::remove_dir(&link).map_err(|e| format!("cannot remove the junction {}: {e}", link.display()))
        }
        Ok(_) if link.join(CLAIM_MODS_MARKER).is_file() => Ok(()),
        Ok(_) => Err(format!("{} is no junction or claim mod folder; nothing of the claim's folder was removed", link.display())),
        Err(_) => Ok(()),
    }
}

/// Whether `path` is a reparse point, as an older claim's junction is.
fn is_junction(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// Copies each save of `saves` (resolved as an explicit file path or inside
/// `dev_saves`) into `to`.
fn copy_saves(dev_saves: &Path, to: &Path, saves: &[String]) -> Result<()> {
    if saves.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(to).map_err(|e| format!("cannot make {}: {e}", to.display()))?;
    for spec in saves {
        let file = crate::saves::resolve_save(spec, dev_saves)?;
        let name = file.file_name().ok_or_else(|| format!("{} names no file", file.display()))?;
        copy(&file, &to.join(name))?;
    }
    Ok(())
}

/// The folder of a claim's test mods, beside its user folder.
const TEST_MODS: &str = "test-mods";

/// Ensures `folder\user\mod` is the claim's own marked private directory,
/// migrating an older claim's junction if encountered and refusing an unmarked
/// plain directory.
fn private_mods(folder: &Path) -> Result<PathBuf> {
    let user = folder.join("user");
    let mods = user.join("mod");
    match fs::symlink_metadata(&mods) {
        Ok(m) if m.file_type().is_symlink() || is_junction(&mods) => {
            let original = plain_path(&mods);
            let parent = original.parent().ok_or("the original mod registry has no user folder")?.to_path_buf();
            let mut copied = Vec::new();
            for entry in fs::read_dir(&original).map_err(|e| format!("cannot read the original mod registry: {e}"))? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("mod")) && entry.file_type().is_ok_and(|t| t.is_file()) {
                    let text = fs::read_to_string(entry.path()).map_err(|e| format!("cannot read {}: {e}", entry.path().display()))?;
                    copied.push((entry.file_name(), Descriptor::relocate(&text, &parent)?));
                }
            }
            fs::remove_dir(&mods).map_err(|e| format!("cannot remove the junction {}: {e}", mods.display()))?;
            init_private_mods(&mods)?;
            for (name, relocated) in copied {
                fs::write(mods.join(name), relocated).map_err(|e| format!("cannot copy a private mod descriptor: {e}"))?;
            }
            Ok(mods)
        }
        Ok(_) if mods.join(CLAIM_MODS_MARKER).is_file() => Ok(mods),
        Ok(_) => Err(format!("{} is no claim mod folder; its registry was not changed", mods.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => init_private_mods(&mods),
        Err(e) => Err(format!("cannot inspect {}: {e}", mods.display())),
    }
}

fn place_test_mods(folder: &Path, mods: &[PathBuf]) -> Result<()> {
    let dir = folder.join(TEST_MODS);
    let user = folder.join("user");
    let registry = if !mods.is_empty() || user.join("mod").exists() { Some(private_mods(folder)?) } else { None };
    const PREFIX: &str = "eu4harmony-test-";
    if let Some(registry) = &registry {
        for entry in fs::read_dir(registry).map_err(|e| format!("cannot read the private mod folder: {e}"))? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().to_string_lossy().starts_with(PREFIX) && entry.file_type().is_ok_and(|t| t.is_file()) {
                remove_file(&entry.path())?;
            }
        }
    }
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
    }
    let playset_path = user.join("dlc_load.json");
    let mut playset = Playset::read_or_default(&playset_path);
    // Keep test games purely isolated: every start enables only the test mods
    // explicitly given, never host or leftover mods.
    playset.retain(|_| false);
    for source in mods {
        let name = source.file_name().ok_or_else(|| format!("{} names no folder", source.display()))?;
        let to = dir.join(name);
        copy_tree(source, &to).map_err(|e| format!("cannot copy the test mod {} to {}: {e}", source.display(), to.display()))?;
        let own = fs::read_to_string(source.join("descriptor.mod")).map_err(|e| format!("the test mod {} has no descriptor.mod: {e}", source.display()))?;
        let descriptor_name = format!("{PREFIX}{}.mod", name.to_string_lossy());
        let descriptor = registry.as_ref().expect("test mods have a private mod folder").join(&descriptor_name);
        let text = Descriptor::with_path(&own, &plain_path(&to));
        fs::write(&descriptor, text).map_err(|e| format!("cannot write {}: {e}", descriptor.display()))?;
        playset.enable(format!("mod/{descriptor_name}"));
    }
    playset.write(&playset_path)
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
    let (user, kept) = (UserFolder::new(folder.join("user")), folder.join("kept"));
    let _ = copy_tree(&user.logs(), &kept.join("eu4-harmony-logs"));
    let _ = copy_tree(&user.game_logs(), &kept.join("game-logs"));
    let _ = copy_tree(&user.harmony_dumps(), &kept.join("harmony-dumps"));
    let _ = copy_tree(&user.crashes(), &kept.join("crashes"));
    if folder.join(ENDS).is_file() {
        let _ = fs::create_dir_all(&kept).and_then(|()| fs::copy(folder.join(ENDS), kept.join(ENDS)));
    }
    user.crash_folders().is_ok_and(|crashes| !crashes.is_empty())
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

/// The local time, `HH:MM:SS`.
fn local_time() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
}

/// Adds how a game ended to the claim's `game-ends.tsv`.
fn write_end(folder: &Path, end: &GameEnd) {
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(folder.join(ENDS)) {
        let _ = writeln!(file, "{}", end.to_line());
    }
}

trait FollowEnv {
    type Handle;
    fn open(&mut self, id: u32) -> Option<(Self::Handle, u64)>;
    fn note_following(&mut self, id: u32);
    fn wait_exit(&mut self, handle: Self::Handle) -> Option<u32>;
    fn record_end(&mut self, game: Process, code: u32);
    fn next_restarted(&mut self, previous_id: u32) -> Option<u32>;
}

struct FolderFollow<'a> {
    folder: &'a Path,
    pipe: &'a str,
}

impl FollowEnv for FolderFollow<'_> {
    type Handle = Game;

    fn open(&mut self, id: u32) -> Option<(Game, u64)> {
        let held = Game::open(id)?;
        let started = held.started();
        Some((held, started))
    }

    fn note_following(&mut self, id: u32) {
        let _ = fs::write(self.folder.join(FOLLOWING), id.to_string());
    }

    fn wait_exit(&mut self, handle: Game) -> Option<u32> {
        handle.wait()
    }

    fn record_end(&mut self, game: Process, code: u32) {
        write_end(self.folder, &GameEnd { game, seen: local_time(), code });
    }

    fn next_restarted(&mut self, previous_id: u32) -> Option<u32> {
        let began = std::time::Instant::now();
        while began.elapsed() < RESTART_WAIT && !self.folder.join(RELEASED).exists() {
            if let Some(again) = serving(self.pipe).filter(|&again| again != previous_id) {
                return Some(again);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        None
    }
}

fn run_follower<E: FollowEnv>(env: &mut E, first: u32) {
    let mut next = Some(first);
    while let Some(id) = next.take() {
        let Some((held, started)) = env.open(id) else { return };
        env.note_following(id);
        let Some(code) = env.wait_exit(held) else { return };
        env.record_end(Process { id, started }, code);
        // A restart ends the game with 0; one ended by these commands, or a
        // crash, starts nothing in its place.
        if code == 0 {
            next = env.next_restarted(id);
        }
    }
}

/// `follow.exe`'s work, `cargo xtask game follow <folder> <id> <pipe>`:
/// follows the game `id` to its end and writes how it ended into the
/// claim's `game-ends.tsv`; then, when it ended with 0, as a restart does,
/// and unless the claim is released, follows the game the test game
/// restarts in its place, the one that opens its pipe within
/// [`RESTART_WAIT`].
pub fn follow_games(folder: &Path, first: u32, pipe: &str) -> Result<()> {
    run_follower(&mut FolderFollow { folder, pipe }, first);
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

    /// Placing test mods replaces `enabled_mods` with only those test mods;
    /// starting with none leaves `enabled_mods` empty.
    #[test]
    fn test_mods_join_the_test_game_s_playset_and_leave_it() {
        let s = Scratch::new("test-mods");
        s.write("claim\\user\\dlc_load.json", r#"{"enabled_mods":["mod/ugc_1.mod"],"disabled_dlcs":["dlc/d1.dlc"]}"#);
        s.write("mods\\qa-steady\\descriptor.mod", "\u{feff}name=\"QA steady\"\npath=\"somewhere\"\narchive=\"old.zip\"\n");
        s.write("mods\\qa-steady\\eu4-harmony\\manifest.toml", "id = \"qa-steady\"");
        let folder = s.0.join("claim");
        place_test_mods(&folder, &[s.0.join("mods\\qa-steady")]).unwrap();
        let descriptor = folder.join("user\\mod\\eu4harmony-test-qa-steady.mod");
        let text = fs::read_to_string(&descriptor).unwrap();
        assert!(text.contains("name=\"QA steady\"") && !text.contains("somewhere") && !text.contains("old.zip"), "{text}");
        assert!(text.contains(&format!("path=\"{}\"", plain_path(&folder.join("test-mods\\qa-steady")).display().to_string().replace('\\', "/"))), "{text}");
        assert!(folder.join("test-mods\\qa-steady\\eu4-harmony\\manifest.toml").is_file());
        let loaded = |folder: &Path| read_json(&folder.join("user\\dlc_load.json")).unwrap();
        assert_eq!(loaded(&folder)["enabled_mods"], serde_json::json!(["mod/eu4harmony-test-qa-steady.mod"]));
        assert_eq!(loaded(&folder)["disabled_dlcs"], serde_json::json!(["dlc/d1.dlc"]));
        assert!(!folder.join("mod-registry").exists());
        place_test_mods(&folder, &[]).unwrap();
        assert_eq!(loaded(&folder)["enabled_mods"], serde_json::json!([]));
        assert!(!folder.join("test-mods").exists());
        assert!(!descriptor.exists());
    }

    /// A claim's user folder starts from a pure synthetic baseline without
    /// touching any host files; save resolution reads only explicit paths or
    /// the development saves folder; and releasing keeps logs and crash
    /// folders while removing `user` and `core`.
    #[test]
    fn a_test_game_s_folder_is_made_pure_and_removed_leaving_kept_logs() {
        let s = Scratch::new("folder");
        let folder = s.0.join("claim");
        make_user_folder(&folder.join("user")).unwrap();
        let user = folder.join("user");
        let game_settings = fs::read_to_string(user.join("settings.txt")).unwrap();
        assert!(
            game_settings.contains("language=\"l_english\"")
                && game_settings.contains("fullScreen=no")
                && game_settings.contains("borderless=yes")
                && game_settings.contains("compress_autosave=yes")
                && game_settings.contains("x=1920")
                && game_settings.contains("y=1080"),
            "{game_settings}"
        );
        let harmony_settings = fs::read_to_string(user.join("eu4-harmony\\settings.txt")).unwrap();
        assert!(
            harmony_settings.contains("language=\"english\"")
                && harmony_settings.contains("achievements_outside_ironman=no")
                && harmony_settings.contains("achievements_with_changed_checksum=no"),
            "{harmony_settings}"
        );
        let dlc_load = read_json(&user.join("dlc_load.json")).unwrap();
        assert_eq!(dlc_load["enabled_mods"], serde_json::json!([]));
        assert!(user.join("mod").join(CLAIM_MODS_MARKER).is_file());
        assert_eq!(fs::read_dir(user.join("mod")).unwrap().count(), 1);

        let dev_saves = s.0.join("dev\\saves");
        s.write("dev\\saves\\observer-1700.1.1.eu4", "a developer's save");
        let explicit = s.write("outside\\custom.eu4", "an explicit save");
        copy_saves(&dev_saves, &user.join("save games"), &[explicit.display().to_string(), "observer-1700.1.1".into()]).unwrap();
        assert_eq!(fs::read_to_string(user.join("save games\\custom.eu4")).unwrap(), "an explicit save");
        assert_eq!(fs::read_to_string(user.join("save games\\observer-1700.1.1.eu4")).unwrap(), "a developer's save");
        let missing = copy_saves(&dev_saves, &user.join("save games"), &["test-regular-1444".into()]).unwrap_err();
        assert!(missing.contains("cargo xtask saves fetch"), "{missing}");

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
    }

    /// An unmarked `mod` folder that is no junction stops removal and mutation:
    /// it might be the player's own user folder.
    #[test]
    fn an_unmarked_mod_folder_that_is_no_junction_is_never_removed_or_changed() {
        let s = Scratch::new("no-junction");
        s.write("claim\\user\\mod\\a.mod", "a descriptor");
        assert!(private_mods(&s.0.join("claim")).unwrap_err().contains("no claim mod folder"));
        assert!(ThisComputer.remove_folder(&s.0.join("claim")).unwrap_err().contains("no junction or claim mod folder"));
        assert!(s.0.join("claim\\user\\mod\\a.mod").is_file());
    }

    #[test]
    fn follower_records_restart_chains_and_stops_on_non_zero_exit_or_release() {
        #[derive(Default)]
        struct Scripted {
            runs: std::collections::VecDeque<(u32, u64, u32)>,
            restarts: std::collections::VecDeque<Option<u32>>,
            following: Vec<u32>,
            recorded: Vec<(Process, u32)>,
        }
        impl FollowEnv for Scripted {
            type Handle = u32;
            fn open(&mut self, id: u32) -> Option<(u32, u64)> {
                let &(expected, started, _) = self.runs.front()?;
                (expected == id).then_some((id, started))
            }
            fn note_following(&mut self, id: u32) {
                self.following.push(id);
            }
            fn wait_exit(&mut self, id: u32) -> Option<u32> {
                let (expected, _, code) = self.runs.pop_front()?;
                (expected == id).then_some(code)
            }
            fn record_end(&mut self, game: Process, code: u32) {
                self.recorded.push((game, code));
            }
            fn next_restarted(&mut self, previous: u32) -> Option<u32> {
                while let Some(candidate) = self.restarts.pop_front() {
                    let id = candidate?;
                    if id != previous {
                        return Some(id);
                    }
                }
                None
            }
        }

        // Restart (0) -> same PID ignored -> new PID (200) -> crash (0xc0000005) -> stops without polling restarts.
        let mut env = Scripted {
            runs: [(100, 10, 0), (200, 20, 0xc000_0005), (300, 30, 1)].into(),
            restarts: [Some(100), Some(200), Some(300)].into(),
            ..Default::default()
        };
        run_follower(&mut env, 100);
        assert_eq!(env.following, [100, 200]);
        assert_eq!(env.recorded, [(Process { id: 100, started: 10 }, 0), (Process { id: 200, started: 20 }, 0xc000_0005)]);

        // Released during restart wait (None) ends cleanly after recording exit 0.
        let mut released = Scripted { runs: [(100, 10, 0)].into(), restarts: [None].into(), ..Default::default() };
        run_follower(&mut released, 100);
        assert_eq!(released.recorded, [(Process { id: 100, started: 10 }, 0)]);
    }
}
