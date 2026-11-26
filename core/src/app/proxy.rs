//! The `version.dll` proxy that loaded this core, as the core sees it: the
//! interface it offers (ADR the-framework-ships-on-the-workshop) and automatic
//! synchronization from the framework folder that holds this core (ADR
//! auto-update-workshop-proxy).
//!
//! When this core was loaded from an `eu4-harmony\` folder outside the game
//! directory (never for a local/manual install or a test game), startup
//! compares `<game>\version.dll` with the companion `version.dll` beside
//! `eu4-harmony\`, stages the newer copy as `version.dll.new-<...>`, renames
//! the mapped `version.dll` aside as `version.dll.old-<...>` (Windows permits
//! renaming a mapped DLL), moves the staged file into place, shows a brief
//! restarting notice, and restarts `eu4.exe` immediately so the new proxy takes
//! effect right away. The next startup removes the retired file once the old
//! process has exited.
use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Once, OnceLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod updating;

/// The oldest proxy interface this core starts with. A proxy without the
/// export is 0, and still loads this core: what interface 1 added, finding
/// the core in Harmony's Workshop item, is the proxy's own business, and a
/// core an older proxy loaded was installed beside it. Raised when the core
/// comes to rely on something only a newer proxy offers.
pub const OLDEST: u32 = 0;

/// Environment variable set on the restarted child `eu4.exe` so that a restart
/// loop is impossible even if the proxy on disk still differs after restarting.
const RESTARTED_ENV: &str = "EU4_HARMONY_PROXY_UPDATED";

static ALREADY_RESTARTED: OnceLock<bool> = OnceLock::new();
static NEXT_STAMP: AtomicU64 = AtomicU64::new(0);

/// The interface the proxy in `game_dir` offers: 0 for one without
/// `eu4_harmony_proxy_interface`; none when no proxy is loaded from there,
/// as when another program loaded this core.
pub fn interface(game_dir: &Path) -> Option<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    let path: Vec<u16> = game_dir.join("version.dll").as_os_str().encode_wide().chain([0]).collect();
    let module = unsafe { GetModuleHandleW(path.as_ptr()) };
    if module.is_null() {
        return None;
    }
    let Some(export) = (unsafe { GetProcAddress(module, c"eu4_harmony_proxy_interface".as_ptr() as *const u8) }) else {
        return Some(0);
    };
    let export: extern "system" fn() -> u32 = unsafe { std::mem::transmute(export) };
    Some(export())
}

/// Whether this core starts with a proxy of interface `found`; the reason
/// for the startup report when it does not.
#[expect(clippy::absurd_extreme_comparisons, reason = "OLDEST is 0 until the core needs more of the proxy, and the check is there for that day")]
pub fn check(found: Option<u32>) -> Result<(), String> {
    match found {
        Some(found) if found < OLDEST => Err(format!(
            "the version.dll in the game folder is too old for this core (its interface is {found}, the core needs {OLDEST} or later); copy version.dll from Harmony's Workshop folder into the game folder again"
        )),
        _ => Ok(()),
    }
}

/// Result of [`sync_and_restart`] when the process continues instead of exiting
/// for a restart or graceful shutdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// No update was needed (up to date, skipped, or not an external install).
    Unchanged,
    /// `version.dll` was updated on disk, but starting the replacement
    /// `eu4.exe` failed.
    UpdatedRestartFailed,
    /// Updating `version.dll` failed, or the restart loop guard stopped it.
    Failed,
}

/// Runs the early proxy check at most once, from `main` step 3 (`hidden::suppress`)
/// before Steam, worker pools or the game window are created.
pub(crate) fn sync_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Some(dir) = crate::base::game_dir() {
            let _ = sync_and_restart(dir, None);
        }
    });
}

/// Checks whether `<game>\version.dll` differs from the companion `version.dll`
/// beside this core's `eu4-harmony\` folder; if so, swaps it safely on disk,
/// shows a brief restarting notice, and restarts `eu4.exe` immediately.
///
/// Must never be called under the Windows loader lock.
pub(crate) fn sync_and_restart(game_dir: &Path, user_dir: Option<&Path>) -> Outcome {
    if interface(game_dir).is_none() {
        return Outcome::Unchanged;
    }
    let Ok(core_dir) = crate::base::paths::core_dir() else {
        return Outcome::Unchanged;
    };
    let command_line = crate::base::paths::command_line();
    let Some(source_dir) = source_dir_for_core(game_dir, &core_dir, command_line) else {
        return Outcome::Unchanged;
    };
    let mut host = LiveHost { game_dir, user_dir, command_line, resolved_user: None };
    run_sync(game_dir, &source_dir, already_restarted(), &mut host)
}

/// Result of inspecting `<game>\version.dll` against `<source_dir>\version.dll`
/// before modifying disk.
#[derive(Debug, PartialEq, Eq)]
enum SyncPlan {
    UpToDate,
    Skipped(&'static str),
    AlreadyRestarted,
    NeedsUpdate { source_bytes: Vec<u8> },
}

/// Private internal seam separating the proxy update state machine from OS
/// process/UI/log side effects, with two adapters: [`LiveHost`] in production
/// and a fake in unit tests.
trait Host {
    fn guard_swap<T>(&mut self, swap: impl FnOnce() -> T) -> (T, bool);
    fn prepare_feedback(&mut self);
    /// Shows the restart notice (unless hidden), then spawns the replacement
    /// `eu4.exe` while the notice is visible. Returns `None` if a shutdown
    /// signal arrived before spawning.
    fn restart_with_notice(&mut self) -> Option<Result<(), String>>;
    fn exit_cleanly(&mut self);
}

struct LiveHost<'a> {
    game_dir: &'a Path,
    user_dir: Option<&'a Path>,
    command_line: &'a str,
    resolved_user: Option<PathBuf>,
}

impl Host for LiveHost<'_> {
    fn guard_swap<T>(&mut self, swap: impl FnOnce() -> T) -> (T, bool) {
        updating::guard_swap(swap)
    }

    fn prepare_feedback(&mut self) {
        // Resolve the user directory only once an update actually happened:
        // normal starts never touch settings or logs before `startup::run`.
        let user = self.user_dir.map(Path::to_path_buf).or_else(|| crate::base::paths::user_dir(self.game_dir).ok());
        if !crate::base::log::is_open() {
            crate::base::reopen_log(user.as_deref());
        }
        if let Some(dir) = user.as_deref() {
            crate::base::lang::decide(dir, crate::base::settings::read(dir).language);
        }
        self.resolved_user = user;
    }

    fn restart_with_notice(&mut self) -> Option<Result<(), String>> {
        let game_dir = self.game_dir;
        let command_line = self.command_line;
        let resolved_user = self.resolved_user.as_deref();
        updating::with_restart_notice(|| {
            log!("proxy: restarting eu4.exe to load the updated version.dll");
            let exe = crate::base::paths::current_exe()?;
            // Release log leases before spawning the replacement process so it
            // can claim the primary `core.log` name directly.
            crate::base::log::close();
            if let Err(e) = spawn_restarted_game(game_dir, &exe, command_line) {
                crate::base::reopen_log(resolved_user);
                return Err(e);
            }
            Ok(())
        })
    }

    fn exit_cleanly(&mut self) {
        crate::base::log::close();
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        unsafe { TerminateProcess(GetCurrentProcess(), 0) };
        unreachable!("the process was terminated");
    }
}

/// Core proxy synchronization state machine shared by production and tests.
fn run_sync(game_dir: &Path, source_dir: &Path, already_restarted: bool, host: &mut impl Host) -> Outcome {
    let source_bytes = match plan_proxy_sync(game_dir, source_dir, already_restarted) {
        Ok(SyncPlan::UpToDate) => return Outcome::Unchanged,
        Ok(SyncPlan::Skipped(why)) => {
            log!("proxy: skipped automatic version.dll update: {why}");
            return Outcome::Unchanged;
        }
        Ok(SyncPlan::AlreadyRestarted) => {
            log!("proxy: version.dll still differs from the framework folder after an automatic restart; not restarting again");
            return Outcome::Failed;
        }
        Err(e) => {
            log!("proxy: could not update version.dll: {e}");
            return Outcome::Failed;
        }
        Ok(SyncPlan::NeedsUpdate { source_bytes }) => source_bytes,
    };

    let (replaced, shutdown_during_swap) = host.guard_swap(|| replace_proxy_file(game_dir, &source_bytes));
    let retired = match replaced {
        Ok(retired) => retired,
        Err(e) => {
            log!("proxy: could not update version.dll: {e}");
            return Outcome::Failed;
        }
    };
    log!(
        "proxy: updated {} from {} (previous proxy moved to {})",
        game_dir.join("version.dll").display(),
        source_dir.join("version.dll").display(),
        retired.display()
    );
    host.prepare_feedback();

    if shutdown_during_swap {
        log!("proxy: system or process shutdown was requested during version.dll update; exiting cleanly without restarting");
        host.exit_cleanly();
        return Outcome::Unchanged;
    }

    match host.restart_with_notice() {
        None => {
            log!("proxy: system or process shutdown was requested before restarting; exiting cleanly without restarting");
            host.exit_cleanly();
            Outcome::Unchanged
        }
        Some(Ok(())) => {
            host.exit_cleanly();
            Outcome::Unchanged
        }
        Some(Err(e)) => {
            log!("proxy: updated version.dll, but could not restart eu4.exe: {e}");
            Outcome::UpdatedRestartFailed
        }
    }
}

/// Returns the framework folder containing the companion `version.dll`
/// (`<core_dir>\..`) when this core lives in an `eu4-harmony\` folder outside
/// `game_dir`, without depending on any Workshop mod id or Steam layout.
/// Excludes test games (`-harmony_core` / `-harmony_test_game`) and local or
/// manual installs inside `<game_dir>\eu4-harmony\`.
fn source_dir_for_core(game_dir: &Path, core_dir: &Path, command_line: &str) -> Option<PathBuf> {
    if eu4_harmony_folders::switch_value(command_line, "-harmony_core").is_some()
        || eu4_harmony_folders::switch_value(command_line, "-harmony_test_game").is_some()
    {
        return None;
    }
    if !core_dir.file_name().is_some_and(|name| name.eq_ignore_ascii_case(eu4_harmony_folders::HARMONY)) {
        return None;
    }
    let source_dir = core_dir.parent()?;
    (!crate::base::paths::same_path(source_dir, game_dir)).then(|| source_dir.to_path_buf())
}

/// Reads and clears [`RESTARTED_ENV`] once per process, so a later in-game
/// restart initiated by `eu4.exe` itself does not inherit a stale guard.
fn already_restarted() -> bool {
    *ALREADY_RESTARTED.get_or_init(|| {
        let seen = std::env::var_os(RESTARTED_ENV).is_some_and(|v| !v.is_empty());
        if seen {
            set_env(RESTARTED_ENV, None);
        }
        seen
    })
}

fn set_env(name: &str, value: Option<&str>) {
    use windows_sys::Win32::System::Environment::SetEnvironmentVariableW;
    let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let value = value.map(|v| v.encode_utf16().chain([0]).collect::<Vec<u16>>());
    let ptr = value.as_ref().map_or(std::ptr::null(), Vec::as_ptr);
    unsafe { SetEnvironmentVariableW(name.as_ptr(), ptr) };
}

/// Removes stale `version.dll.old-*` and `version.dll.new-*` files in
/// `game_dir` that no running process maps. Right after a proxy-update
/// restart, briefly retries locked files in case the previous `eu4.exe` is
/// still finishing OS teardown.
fn cleanup_stale_proxies(game_dir: &Path, retry_locked: bool) -> usize {
    let attempts = if retry_locked { 6 } else { 1 };
    let mut removed = 0;
    for attempt in 0..attempts {
        let Ok(entries) = fs::read_dir(game_dir) else { break };
        let mut remaining = 0;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(eu4_harmony_folders::OLD_PROXY_PREFIX) || name.starts_with(eu4_harmony_folders::NEW_PROXY_PREFIX) {
                if fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                } else {
                    remaining += 1;
                }
            }
        }
        if remaining == 0 || attempt + 1 == attempts {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    removed
}

/// Cleans up retired/staged proxy files and checks whether
/// `<game_dir>\version.dll` needs to be replaced with `<source_dir>\version.dll`.
fn plan_proxy_sync(game_dir: &Path, source_dir: &Path, already_restarted: bool) -> Result<SyncPlan, String> {
    let removed = cleanup_stale_proxies(game_dir, already_restarted);
    if removed > 0 {
        log!("proxy: removed {removed} retired/staged version.dll file(s) from {}", game_dir.display());
    }
    let source_proxy = source_dir.join("version.dll");
    let game_proxy = game_dir.join("version.dll");
    if !source_proxy.is_file() {
        return Ok(SyncPlan::Skipped("framework folder has no version.dll"));
    }
    if !game_proxy.is_file() {
        return Ok(SyncPlan::Skipped("game folder has no version.dll"));
    }
    let source_bytes = fs::read(&source_proxy).map_err(|e| format!("cannot read {}: {e}", source_proxy.display()))?;
    if !eu4_harmony_folders::is_harmony_proxy(&source_bytes) {
        return Err(format!("{} is not Harmony's version.dll", source_proxy.display()));
    }
    let game_bytes = fs::read(&game_proxy).map_err(|e| format!("cannot read {}: {e}", game_proxy.display()))?;
    if game_bytes == source_bytes {
        return Ok(SyncPlan::UpToDate);
    }
    if !eu4_harmony_folders::is_harmony_proxy(&game_bytes) {
        return Err(format!("{} is not Harmony's version.dll; refusing to replace it", game_proxy.display()));
    }
    if already_restarted {
        return Ok(SyncPlan::AlreadyRestarted);
    }
    Ok(SyncPlan::NeedsUpdate { source_bytes })
}

/// Stages `source_bytes` beside `<game_dir>\version.dll`, moves the loaded
/// proxy aside as `version.dll.old-<...>`, and renames the staged file into
/// place (rolling back if the final rename fails).
fn replace_proxy_file(game_dir: &Path, source_bytes: &[u8]) -> Result<PathBuf, String> {
    let game_proxy = game_dir.join("version.dll");
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let seq = NEXT_STAMP.fetch_add(1, Ordering::Relaxed);
    let stamp = format!("{}-{nanos}-{seq}", std::process::id());
    let staged = game_dir.join(format!("{}{stamp}", eu4_harmony_folders::NEW_PROXY_PREFIX));
    let retired = game_dir.join(format!("{}{stamp}", eu4_harmony_folders::OLD_PROXY_PREFIX));

    // Write the new proxy beside the live one first so a disk-full or I/O
    // failure never leaves the game folder without version.dll.
    let write_staged = || -> std::io::Result<()> {
        let mut file = OpenOptions::new().create_new(true).write(true).open(&staged)?;
        file.write_all(source_bytes)?;
        file.flush()
    };
    if let Err(e) = write_staged() {
        let _ = fs::remove_file(&staged);
        return Err(format!("cannot stage {}: {e}", staged.display()));
    }
    if let Err(e) = fs::rename(&game_proxy, &retired) {
        let _ = fs::remove_file(&staged);
        return Err(format!("cannot move {} to {}: {e}", game_proxy.display(), retired.display()));
    }
    if let Err(e) = fs::rename(&staged, &game_proxy) {
        let rollback = match fs::rename(&retired, &game_proxy) {
            Ok(()) => "restored the previous version.dll".to_string(),
            Err(rb) => format!("and could not restore {}: {rb}", retired.display()),
        };
        let _ = fs::remove_file(&staged);
        return Err(format!("cannot move {} into place: {e} ({rollback})", staged.display()));
    }
    Ok(retired)
}

/// Spawns a replacement `eu4.exe` with the current command line, working in
/// `game_dir` and carrying [`RESTARTED_ENV`] so it cannot loop.
fn spawn_restarted_game(game_dir: &Path, exe: &Path, command_line: &str) -> Result<u32, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW},
    };
    let wide_path = |path: &Path| path.as_os_str().encode_wide().chain([0]).collect::<Vec<u16>>();
    let exe_w = wide_path(exe);
    let dir_w = wide_path(game_dir);
    let mut line_w: Vec<u16> = command_line.encode_utf16().chain([0]).collect();
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    set_env(RESTARTED_ENV, Some("1"));
    let made = unsafe {
        CreateProcessW(exe_w.as_ptr(), line_w.as_mut_ptr(), std::ptr::null(), std::ptr::null(), 0, 0, std::ptr::null(), dir_w.as_ptr(), &startup, &mut info)
    };
    if made == 0 {
        let error = std::io::Error::last_os_error();
        set_env(RESTARTED_ENV, None);
        return Err(format!("CreateProcessW({}) failed: {error}", exe.display()));
    }
    unsafe {
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
    }
    Ok(info.dwProcessId)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::FreeLibrary,
        System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_AS_IMAGE_RESOURCE},
    };

    #[derive(Default)]
    struct FakeHost {
        shutdown_during_swap: bool,
        shutdown_during_notice: bool,
        spawn_error: Option<String>,
        prepared_feedback: bool,
        showed_notice: bool,
        spawned: bool,
        exited: bool,
    }

    impl Host for FakeHost {
        fn guard_swap<T>(&mut self, swap: impl FnOnce() -> T) -> (T, bool) {
            (swap(), self.shutdown_during_swap)
        }

        fn prepare_feedback(&mut self) {
            self.prepared_feedback = true;
        }

        fn restart_with_notice(&mut self) -> Option<Result<(), String>> {
            self.showed_notice = true;
            if self.shutdown_during_notice {
                return None;
            }
            if let Some(err) = &self.spawn_error {
                return Some(Err(err.clone()));
            }
            self.spawned = true;
            Some(Ok(()))
        }

        fn exit_cleanly(&mut self) {
            self.exited = true;
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let seq = NEXT_STAMP.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("eu4h-proxy-sync-{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A loadable PE image (Windows' own `version.dll`) with trailing bytes
    /// carrying Harmony's marker, so Windows can map it and `is_harmony_proxy`
    /// recognizes it.
    fn fake_harmony_proxy(tag: &[u8]) -> Vec<u8> {
        let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into())).join("System32").join("version.dll");
        let mut bytes = fs::read(system).unwrap();
        bytes.extend_from_slice(tag);
        bytes.extend_from_slice(b"eu4_harmony_bootstrap");
        bytes
    }

    fn map_dll(path: &Path) -> *mut std::ffi::c_void {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let module = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_AS_IMAGE_RESOURCE) };
        assert!(!module.is_null(), "cannot map {}", path.display());
        module
    }

    fn retired_files(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(eu4_harmony_folders::OLD_PROXY_PREFIX)))
            .collect()
    }

    /// A proxy older than the core needs stops the startup; one as new or
    /// newer, or none found, does not.
    #[test]
    fn a_proxy_older_than_the_core_needs_stops_the_startup() {
        assert!(check(None).is_ok());
        assert!(check(Some(OLDEST)).is_ok());
        assert!(check(Some(OLDEST + 1)).is_ok());
        if let Some(older) = OLDEST.checked_sub(1) {
            let reason = check(Some(older)).unwrap_err();
            assert!(reason.contains("version.dll"), "{reason}");
        }
    }

    /// The test program loaded no proxy from its folder.
    #[test]
    fn no_proxy_is_found_where_none_was_loaded() {
        assert_eq!(interface(&std::env::temp_dir()), None);
    }

    /// Only a core loaded from an `eu4-harmony\` folder outside `game_dir`
    /// triggers automatic proxy synchronization, regardless of any Workshop
    /// mod id; local installs, arbitrary build folders, and test games do not.
    #[test]
    fn source_detection_excludes_local_installs_and_test_games_without_depending_on_mod_id() {
        let root = scratch("detect");
        let game = root.join("game");
        let external = root.join("any-workshop-or-external-folder");
        let external_core_dir = external.join("eu4-harmony");
        let local_core_dir = game.join("eu4-harmony");
        let target_dir = root.join("target").join("release");
        fs::create_dir_all(&external_core_dir).unwrap();
        fs::create_dir_all(&local_core_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();

        assert_eq!(source_dir_for_core(&game, &external_core_dir, "eu4.exe").as_deref(), Some(external.as_path()));
        // Case differences on Windows still match.
        let upper = PathBuf::from(external_core_dir.to_string_lossy().to_uppercase());
        assert_eq!(source_dir_for_core(&game, &upper, "eu4.exe").map(|p| crate::base::paths::same_path(&p, &external)), Some(true));
        // Local/manual install does not trigger sync.
        assert_eq!(source_dir_for_core(&game, &local_core_dir, "eu4.exe"), None);
        // Arbitrary build output folder (not `eu4-harmony\`) does not trigger sync.
        assert_eq!(source_dir_for_core(&game, &target_dir, "eu4.exe"), None);
        // Test-game switches never trigger sync.
        assert_eq!(source_dir_for_core(&game, &external_core_dir, r"eu4.exe -harmony_core=D:\c.dll"), None);
        assert_eq!(source_dir_for_core(&game, &external_core_dir, "eu4.exe -harmony_test_game=qa"), None);
        let _ = fs::remove_dir_all(root);
    }

    /// A mapped (in-use) `version.dll` is renamed aside, replaced with the
    /// newer bytes, triggers notice + restart + clean exit, and is cleaned up
    /// on the following startup once the old mapping is released.
    #[test]
    fn replaces_a_loaded_proxy_restarts_and_cleans_up_after_release() {
        let root = scratch("replace");
        let game = root.join("game");
        let source = root.join("source");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&source).unwrap();

        let old_bytes = fake_harmony_proxy(b"old-proxy-v1");
        let new_bytes = fake_harmony_proxy(b"new-proxy-v2");
        fs::write(game.join("version.dll"), &old_bytes).unwrap();
        fs::write(source.join("version.dll"), &new_bytes).unwrap();
        fs::write(game.join(format!("{}stale", eu4_harmony_folders::NEW_PROXY_PREFIX)), b"leftover").unwrap();

        let loaded = map_dll(&game.join("version.dll"));
        assert!(fs::remove_file(game.join("version.dll")).is_err(), "mapped DLL cannot be deleted directly");

        let mut host = FakeHost::default();
        assert_eq!(run_sync(&game, &source, false, &mut host), Outcome::Unchanged);
        assert!((host.prepared_feedback, host.showed_notice, host.spawned, host.exited) == (true, true, true, true));
        assert_eq!(fs::read(game.join("version.dll")).unwrap(), new_bytes);
        assert_eq!(retired_files(&game).len(), 1, "retired loaded DLL remains on disk while mapped");
        assert!(!game.join(format!("{}stale", eu4_harmony_folders::NEW_PROXY_PREFIX)).exists());

        // While the old mapping is still held, another run sees matching bytes and does nothing.
        let mut second = FakeHost::default();
        assert_eq!(run_sync(&game, &source, false, &mut second), Outcome::Unchanged);
        assert!(!second.showed_notice && !second.spawned && !second.exited);
        assert_eq!(retired_files(&game).len(), 1);

        // Once the old process exits (mapping released), the restarted startup removes the retired file.
        unsafe { FreeLibrary(loaded) };
        let mut restarted = FakeHost::default();
        assert_eq!(run_sync(&game, &source, true, &mut restarted), Outcome::Unchanged);
        assert!(!restarted.showed_notice && !restarted.spawned && !restarted.exited);
        assert!(retired_files(&game).is_empty(), "restarted startup removes the retired proxy");
        let _ = fs::remove_dir_all(root);
    }

    /// Graceful shutdown arriving during the swap or during the restart notice
    /// keeps the updated file and exits cleanly without spawning a new game.
    #[test]
    fn shutdown_during_swap_or_notice_exits_without_restarting() {
        for (during_swap, during_notice) in [(true, false), (false, true)] {
            let root = scratch("shutdown");
            let (game, source) = (root.join("game"), root.join("source"));
            fs::create_dir_all(&game).unwrap();
            fs::create_dir_all(&source).unwrap();
            let old_bytes = fake_harmony_proxy(b"v1");
            let new_bytes = fake_harmony_proxy(b"v2");
            fs::write(game.join("version.dll"), &old_bytes).unwrap();
            fs::write(source.join("version.dll"), &new_bytes).unwrap();

            let mut host = FakeHost { shutdown_during_swap: during_swap, shutdown_during_notice: during_notice, ..Default::default() };
            assert_eq!(run_sync(&game, &source, false, &mut host), Outcome::Unchanged);
            assert_eq!(fs::read(game.join("version.dll")).unwrap(), new_bytes, "updated proxy stays on disk");
            assert!(host.exited && !host.spawned, "exits cleanly without spawning a new game");
            assert_eq!(host.showed_notice, !during_swap);
            let _ = fs::remove_dir_all(root);
        }
    }

    /// If spawning the replacement process fails after the file was swapped,
    /// `run_sync` reports `UpdatedRestartFailed` without terminating.
    #[test]
    fn spawn_failure_reports_updated_restart_failed() {
        let root = scratch("spawn-fail");
        let (game, source) = (root.join("game"), root.join("source"));
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(game.join("version.dll"), fake_harmony_proxy(b"v1")).unwrap();
        fs::write(source.join("version.dll"), fake_harmony_proxy(b"v2")).unwrap();

        let mut host = FakeHost { spawn_error: Some("denied".into()), ..Default::default() };
        assert_eq!(run_sync(&game, &source, false, &mut host), Outcome::UpdatedRestartFailed);
        assert!(host.showed_notice && !host.spawned && !host.exited);
        let _ = fs::remove_dir_all(root);
    }

    /// If `version.dll` still differs right after a restart, the loop guard
    /// stops a second restart.
    #[test]
    fn loop_guard_prevents_repeated_restarts() {
        let root = scratch("loop-guard");
        let (game, source) = (root.join("game"), root.join("source"));
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&source).unwrap();

        let old_bytes = fake_harmony_proxy(b"v1");
        fs::write(game.join("version.dll"), &old_bytes).unwrap();
        fs::write(source.join("version.dll"), fake_harmony_proxy(b"v2")).unwrap();

        let mut host = FakeHost::default();
        assert_eq!(run_sync(&game, &source, true, &mut host), Outcome::Failed);
        assert!(!host.showed_notice && !host.spawned && !host.exited);
        assert_eq!(fs::read(game.join("version.dll")).unwrap(), old_bytes);
        let _ = fs::remove_dir_all(root);
    }

    /// Foreign or corrupted `version.dll` files are never overwritten or copied.
    #[test]
    fn refuses_non_harmony_proxy_files() {
        let root = scratch("foreign");
        let (game, source) = (root.join("game"), root.join("source"));
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&source).unwrap();

        let harmony = fake_harmony_proxy(b"harmony");
        fs::write(game.join("version.dll"), &harmony).unwrap();
        fs::write(source.join("version.dll"), b"not a harmony proxy").unwrap();
        let mut host = FakeHost::default();
        assert_eq!(run_sync(&game, &source, false, &mut host), Outcome::Failed);
        assert!(!host.spawned && !host.exited);
        assert_eq!(fs::read(game.join("version.dll")).unwrap(), harmony);

        fs::write(game.join("version.dll"), b"foreign proxy").unwrap();
        fs::write(source.join("version.dll"), &harmony).unwrap();
        let mut host = FakeHost::default();
        assert_eq!(run_sync(&game, &source, false, &mut host), Outcome::Failed);
        assert!(!host.spawned && !host.exited);
        assert_eq!(fs::read(game.join("version.dll")).unwrap(), b"foreign proxy");
        let _ = fs::remove_dir_all(root);
    }
}
