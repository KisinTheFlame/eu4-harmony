//! EU4 Harmony core. Loaded by the `version.dll` proxy during process start;
//! `eu4_harmony_bootstrap` runs under the loader lock, so it only checks the
//! executable and installs every dependency point, all or none
//! (ADR framework-succeeds-or-fails-as-a-whole). Everything that needs the
//! game's own setup (its file system, the user directory) waits for the game's
//! first `PHYSFS_init`, on the game thread. When startup fails nothing is
//! hooked; a thread of its own writes the report and tells the player.
// The capability groups' list declares every group's modules inside its
// macro, so a module's own macros, such as a test's large `json!`, expand
// deeper with each group the list holds.
#![recursion_limit = "256"]
pub type Result<T> = std::result::Result<T, String>;

/// Marks a function the game's code calls, which runs under [`guarded`].
pub(crate) use eu4_harmony_core_macros::called_by_game;

/// A developer build's test switch, `test_switch!(crash)`: as dev.toml's
/// `[test-switches]` has it, and off in a build without the test switches,
/// which has no dev.toml to read. The speed-ups' switches are read through
/// `speed`.
macro_rules! test_switch {
    ($switch:ident) => {{
        #[cfg(feature = "dev-test-switches")]
        let on = crate::dev::options().test_switches.$switch;
        #[cfg(not(feature = "dev-test-switches"))]
        let on = false;
        on
    }};
}

#[macro_use]
pub mod log;
#[macro_use]
pub mod stub;
pub mod achievements;
#[cfg(feature = "dev-test-switches")]
mod agents_order;
#[cfg(any(test, feature = "analysis"))]
pub mod analysis;
mod army_search;
pub mod baseline;
mod bound_element;
pub mod catalogue;
#[cfg(feature = "dev-heap-census")]
pub mod census;
#[cfg(feature = "dev-command-log")]
mod command_log;
pub mod containers;
mod crash;
#[cfg(feature = "dev-crossings")]
mod crossings;
#[cfg(any(test, dev_build))]
pub mod dev;
pub mod drawing;
pub mod files;
pub mod framework_ui;
pub mod game;
pub mod game_heap;
pub mod game_string;
pub mod heap;
pub mod hidden;
pub mod hooks;
pub mod hosts;
pub mod icu;
pub mod lang;
pub mod loading;
pub mod loc;
pub mod lua;
mod main_thread;
pub mod map;
pub mod memory;
#[cfg(feature = "dev-memory")]
mod memory_account;
mod modifier_sums;
pub mod mods;
pub mod notice;
pub mod objects;
pub mod paths;
pub mod pdx;
pub mod physfs;
pub mod pipe;
pub mod points;
mod proxy;
pub mod remote;
pub mod repairs;
pub mod report;
pub mod safe_point;
pub mod saves;
pub mod saving;
pub mod sessions;
pub mod settings;
pub mod sound;
pub mod speed;
pub mod splice;
mod stack;
mod startup;
#[cfg(feature = "dev-call-trace")]
mod trace;
mod troubleshooting;
pub mod ui;
pub mod unicode;
pub mod windows;
pub mod wording;
#[cfg(feature = "dev-write-watch")]
mod write_watch;
pub mod zip;

use std::sync::{
    atomic::{AtomicU32, Ordering},
    OnceLock,
};
use windows_sys::Win32::System::{
    LibraryLoader::GetModuleHandleW,
    Threading::{GetCurrentProcess, GetCurrentThreadId, TerminateProcess},
};

static GAME_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

/// The game folder, once the proxy has told it.
pub(crate) fn game_dir() -> Option<&'static std::path::Path> {
    GAME_DIR.get().map(std::path::PathBuf::as_path)
}
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);
static MEMORY: OnceLock<memory::Memory> = OnceLock::new();
/// What startup installed, for the report.
static STARTUP: OnceLock<String> = OnceLock::new();

pub fn thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}

/// The thread that started the process, which runs the game's UI and updates.
pub fn main_thread() -> u32 {
    MAIN_THREAD.load(Ordering::Relaxed)
}

/// The game image, once startup has begun.
pub(crate) fn memory() -> Option<&'static memory::Memory> {
    MEMORY.get()
}

/// The game image, for code only hooks run: startup records it before it
/// installs them.
pub(crate) fn hooked_memory() -> memory::Memory {
    *MEMORY.get().expect("hooks run after startup")
}

/// Runs framework code. A panic there is a framework bug, not something to
/// carry on from: its thread and backtrace are logged as it happens
/// ([`log_panics`]), then the player is told and the game ends ([`fatal`];
/// ADR framework-succeeds-or-fails-as-a-whole). Every function the game
/// calls runs under it, so no panic crosses into the game's code.
pub fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> T {
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

fn start(m: memory::Memory) -> Result<String> {
    let baseline = baseline::recognize(&m)?;
    icu::load()?;
    #[cfg(dev_build)]
    dev::load(&paths::core_dir()?)?;
    if test_switch!(fail_startup) {
        return Err("fail-startup is on in dev.toml's [test-switches]".into());
    }
    speed::check_switches()?;
    let installed = points::install(m, &points::this_build::installed(m))?;
    #[cfg(feature = "dev-command-log")]
    command_log::start();
    Ok(format!("baseline {baseline}: {} dependency points match; {} hooks, {} writes", installed.checked, installed.hooks, installed.writes))
}

/// Log file in the game folder, used only when the user directory cannot be
/// determined, so a failure is still written down somewhere.
fn open_fallback_log() {
    if !log::is_open() {
        if let Some(dir) = GAME_DIR.get() {
            let _ = log::open(&dir.join("eu4-harmony").join("logs").join("core.log"));
        }
    }
}

/// Logs every panic in framework code as it happens, with its thread and
/// backtrace, which only the panic itself still has: the first step of the
/// one way a panic goes. [`guarded`], around every function the game calls
/// (the observer seam and `#[called_by_game]`), then catches it, and
/// [`fatal`] tells the player and ends the game.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log!("panic on thread {} ({}): {info}\n{}", thread.name().unwrap_or("of the game's"), thread_id(), std::backtrace::Backtrace::force_capture());
        default(info);
    }));
}

/// Why the startup failed, for the report, and what the player is told.
struct Failure {
    reason: String,
    /// The notice, when it is not the startup failure notice with the reason.
    notice: Option<notice::Message>,
}

impl From<String> for Failure {
    fn from(reason: String) -> Self {
        Self { reason, notice: None }
    }
}

fn bootstrap() -> std::result::Result<(), Failure> {
    log_panics();
    MAIN_THREAD.store(thread_id(), Ordering::Relaxed);
    let game_dir = paths::game_dir()?;
    // The log stays buffered until the user directory is known (deferred_init).
    log!("eu4-harmony core {}{} bootstrap in {}", env!("CARGO_PKG_VERSION"), build(), game_dir.display());
    hidden::read_switches();
    let found = proxy::interface(&game_dir);
    let _ = GAME_DIR.set(game_dir);
    log!("proxy interface: {}", found.map_or_else(|| "no proxy in the game folder".to_string(), |i| i.to_string()));
    proxy::check(found).map_err(|reason| Failure { reason, notice: Some(notice::Message::new("EU4HARMONY_PROXY_OUTDATED")) })?;
    let m = memory::Memory { base: unsafe { GetModuleHandleW(std::ptr::null()) } as usize };
    let _ = MEMORY.set(m);
    let summary = start(m)?;
    log!("startup: {summary}");
    log!("memory at startup: {}", heap::summary());
    let _ = STARTUP.set(summary);
    Ok(())
}

/// Opens the log and the report in the user directory's `eu4-harmony\logs`,
/// or the fallback log when the folder cannot be written; returns the folder.
fn open_logs(user_dir: &std::path::Path) -> std::path::PathBuf {
    let user = eu4_harmony_folders::UserFolder::new(user_dir);
    if let Err(e) = log::open(&user.core_log()) {
        open_fallback_log();
        log!("cannot open {}: {e}", user.core_log().display());
    }
    report::open(user.report());
    user.logs()
}

/// What this core is, after its version: its api, and in a developer build
/// that it is one and its tools.
fn build() -> String {
    let api = format!(" (api {})", eu4_harmony_remote::API_VERSION);
    #[cfg(dev_build)]
    let api = format!("{api}, {}", dev::build());
    api
}

/// The report's first lines: this core, and the game folder when it is known;
/// in a developer build, its tools' options.
fn report_header(game_dir: Option<&std::path::Path>) {
    report::line(format!("Harmony {}{}, started {}", env!("CARGO_PKG_VERSION"), build(), log::now()));
    if let Some(dir) = game_dir {
        report::line(format!("game: {}", dir.display()));
    }
    #[cfg(dev_build)]
    for line in dev::report_lines() {
        report::line(line);
    }
    for line in speed::report_lines() {
        report::line(line);
    }
}

/// Startup failed and nothing was hooked: writes the log and report and tells
/// the player, from a thread of its own since the loader lock is held here.
/// A hidden start ends the game instead: without the framework it could be
/// neither used nor hidden.
fn fail_startup(reason: String, told: Option<notice::Message>) {
    let game_dir = GAME_DIR.get().cloned();
    std::thread::spawn(move || {
        let user_dir = game_dir.as_deref().and_then(|d| paths::user_dir(d).ok());
        match &user_dir {
            Some(dir) => {
                open_logs(dir);
            }
            None => open_fallback_log(),
        }
        report_header(game_dir.as_deref());
        report::line(format!("startup FAILED; nothing was hooked and no extension runs:\n{reason}"));
        report::write();
        if hidden::hidden_start() {
            log!("startup failed in a hidden start, ending the game: {}", reason.replace('\n', "; "));
            unsafe { TerminateProcess(GetCurrentProcess(), 3) };
        }
        if let Some(dir) = &user_dir {
            lang::decide(dir, settings::read(dir).language);
        }
        // A changed game can fail every point; the report keeps the full list.
        const SHOWN: usize = 8;
        let lines: Vec<&str> = reason.lines().collect();
        let detail = match lines.len().checked_sub(SHOWN) {
            Some(more) if more > 0 => format!("{}\n… ({more} more in eu4-harmony/logs/report.txt)", lines[..SHOWN].join("\n")),
            _ => reason.clone(),
        };
        notice::tell_and_wait(&told.unwrap_or_else(|| notice::Message::new("EU4HARMONY_STARTUP_FAILED").arg("DETAIL", detail)));
    });
}

/// Runs once, on the game thread, from the first PhysFS call: the rest of
/// startup (`startup`).
pub(crate) fn deferred_init() {
    startup::run(GAME_DIR.get().cloned().unwrap_or_default(), STARTUP.get().map_or("?", String::as_str));
}

/// Called by the proxy from its DllMain. Returns 0 on success.
#[no_mangle]
pub extern "system" fn eu4_harmony_bootstrap() -> u32 {
    match std::panic::catch_unwind(bootstrap) {
        Ok(Ok(())) => 0,
        Ok(Err(Failure { reason, notice })) => {
            log!("startup failed: {reason}");
            fail_startup(reason, notice);
            1
        }
        Err(_) => {
            log!("startup failed: panic");
            fail_startup("panic during startup".into(), None);
            2
        }
    }
}
