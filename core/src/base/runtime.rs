//! Process-wide runtime primitives shared across the core: error type,
//! panic guard, thread identities, bootstrap timing, folders, window handle,
//! hidden-start flags, and session counter mirror.
#[cfg(not(test))]
use super::notice;
use super::{log, report};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering},
    OnceLock,
};
#[cfg(not(test))]
use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
use windows_sys::Win32::{Foundation::HWND, System::Threading::GetCurrentThreadId};

pub type Result<T> = std::result::Result<T, String>;

/// What this build is, appended after the version and API version in logs and
/// the startup report; `None` for a player build.
pub(crate) fn build_label() -> Option<&'static str> {
    #[cfg(feature = "dev")]
    return Some("developer build");
    #[cfg(all(feature = "dev-test-games", not(feature = "dev")))]
    return Some("test games' pipes");
    #[cfg(not(feature = "dev-test-games"))]
    return None;
}

/// Marks a function the game's code calls, which runs under [`guarded`].
pub(crate) use eu4_harmony_core_macros::called_by_game;

static GAME_DIR: OnceLock<PathBuf> = OnceLock::new();
pub(crate) static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);
/// Thread ID of the framework's drawing thread, 0 while none is running.
pub(crate) static DRAWING_THREAD: AtomicU32 = AtomicU32::new(0);
/// When the proxy called the bootstrap, as the game's process began.
static BOOTSTRAP: OnceLock<std::time::Instant> = OnceLock::new();
/// The game's Win32 window handle, once attached.
static GAME_WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Whether `-harmony_hidden` was passed on the game's command line.
static HIDDEN_START: AtomicBool = AtomicBool::new(false);
/// Whether the game's window is currently hidden.
static HIDDEN_NOW: AtomicBool = AtomicBool::new(false);
/// Mirror of the active session number for base-level services such as notices.
static SESSION_NUMBER: AtomicU64 = AtomicU64::new(0);

/// Records the game folder discovered at bootstrap.
pub(crate) fn set_game_dir(dir: PathBuf) {
    let _ = GAME_DIR.set(dir);
}

/// The game folder, once the proxy has told it.
pub(crate) fn game_dir() -> Option<&'static Path> {
    GAME_DIR.get().map(PathBuf::as_path)
}

/// Records the instant bootstrap began.
pub(crate) fn mark_bootstrap() {
    let _ = BOOTSTRAP.set(std::time::Instant::now());
}

/// The time since the framework's bootstrap, which the log's times of the
/// game's loading count from.
pub(crate) fn since_bootstrap() -> std::time::Duration {
    BOOTSTRAP.get_or_init(std::time::Instant::now).elapsed()
}

pub(crate) fn thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}

/// The thread that started the process, which runs the game's UI and updates.
pub(crate) fn main_thread() -> u32 {
    MAIN_THREAD.load(Ordering::Relaxed)
}

/// Records the active drawing thread's OS thread ID (0 when stopped).
pub(crate) fn set_drawing_thread(id: u32) {
    DRAWING_THREAD.store(id, Ordering::Release);
}

/// Whether the current thread is the framework's drawing thread.
pub(crate) fn is_drawing_thread() -> bool {
    let id = DRAWING_THREAD.load(Ordering::Acquire);
    id != 0 && thread_id() == id
}

/// Records the game's Win32 window handle once the window procedure attaches.
pub(crate) fn set_game_window(window: HWND) {
    GAME_WINDOW.store(window as isize, Ordering::Release);
}

/// The game's Win32 window handle, once attached.
pub(crate) fn game_window() -> Option<HWND> {
    let window = GAME_WINDOW.load(Ordering::Acquire);
    (window != 0).then_some(window as HWND)
}

/// Records whether `-harmony_hidden` was present on startup.
pub(crate) fn set_hidden_start(hidden: bool) {
    HIDDEN_START.store(hidden, Ordering::Relaxed);
}

/// Whether the game was started with `-harmony_hidden`.
pub(crate) fn hidden_start() -> bool {
    HIDDEN_START.load(Ordering::Relaxed)
}

/// Records whether the game's window is hidden right now.
pub(crate) fn set_hidden(hidden: bool) {
    HIDDEN_NOW.store(hidden, Ordering::Relaxed);
}

/// Atomically updates the hidden flag and returns its previous value.
pub(crate) fn swap_hidden(hidden: bool) -> bool {
    HIDDEN_NOW.swap(hidden, Ordering::Relaxed)
}

/// Whether the game's window is hidden right now.
pub(crate) fn is_hidden() -> bool {
    HIDDEN_NOW.load(Ordering::Relaxed)
}

/// Updates the session number mirror used by notices.
pub(crate) fn set_session_number(number: u64) {
    SESSION_NUMBER.store(number, Ordering::Relaxed);
}

/// The current session number (0 before any session starts).
pub(crate) fn session_number() -> u64 {
    SESSION_NUMBER.load(Ordering::Relaxed)
}

/// Runs framework code. A panic there is a framework bug, not something to
/// carry on from: its thread and backtrace are logged as it happens
/// (`log_panics`), then the player is told and the game ends ([`fatal`];
/// ADR framework-succeeds-or-fails-as-a-whole). Every function the game
/// calls runs under it, so no panic crosses into the game's code.
pub(crate) fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(payload) => {
            let text = payload.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| payload.downcast_ref::<String>().cloned());
            fatal(&format!("{what}: {}", text.unwrap_or_else(|| "panic".into())))
        }
    }
}

/// Ends the game after an unexpected error in framework code
/// (ADR framework-succeeds-or-fails-as-a-whole).
#[cfg(not(test))]
pub(crate) fn fatal(detail: &str) -> ! {
    log!("fatal error, ending the game: {detail}");
    notice::tell_and_wait(&notice::Message::new("EU4HARMONY_FATAL").arg("DETAIL", detail));
    unsafe { TerminateProcess(GetCurrentProcess(), 3) };
    unreachable!("the process was terminated")
}

/// In a test, the fatal path panics with the detail instead, so a test can
/// see that a panic reached it.
#[cfg(test)]
pub(crate) fn fatal(detail: &str) -> ! {
    panic!("fatal: {detail}")
}

/// Logs every panic in framework code as it happens, with its thread and
/// backtrace, which only the panic itself still has: the first step of the
/// one way a panic goes. [`guarded`], around every function the game calls
/// (the observer seam and `#[called_by_game]`), then catches it, and
/// [`fatal`] tells the player and ends the game.
pub(crate) fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::write_within(
            format_args!(
                "panic on thread {} ({}): {info}\n{}",
                thread.name().unwrap_or("of the game's"),
                thread_id(),
                std::backtrace::Backtrace::force_capture()
            ),
            std::time::Duration::from_secs(1),
        );
        default(info);
    }));
}

/// Log file in the game folder, used only when the user directory cannot be
/// determined, so a failure is still written down somewhere.
pub(crate) fn open_fallback_log() {
    if !log::is_open() {
        if let Some(dir) = GAME_DIR.get() {
            let _ = log::open(&dir.join("eu4-harmony").join("logs").join("core.log"));
        }
    }
}

/// Opens the log and the report in the user directory's `eu4-harmony\logs`,
/// or the fallback log when the folder cannot be written; returns the folder.
pub(crate) fn open_logs(user_dir: &Path) -> PathBuf {
    let user = eu4_harmony_folders::UserFolder::new(user_dir);
    if let Err(e) = log::open(&user.core_log()) {
        open_fallback_log();
        log!("cannot open {}: {e}", user.core_log().display());
    }
    report::open(user.report());
    user.logs()
}

/// Reopens only the framework log after [`log::close`], without rotating
/// `report.txt` again.
pub(crate) fn reopen_log(user_dir: Option<&Path>) {
    match user_dir {
        Some(dir) => {
            let user = eu4_harmony_folders::UserFolder::new(dir);
            if log::open(&user.core_log()).is_err() {
                open_fallback_log();
            }
        }
        None => open_fallback_log(),
    }
}
