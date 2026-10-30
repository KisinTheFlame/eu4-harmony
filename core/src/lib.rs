//! EU4 Harmony core. Loaded by the `version.dll` proxy during process start;
//! `eu4_harmony_bootstrap` runs under the loader lock, so it only checks the
//! executable and installs every dependency point, all or none (ADR 0012).
//! Everything that needs the game's own setup (its file system, the user
//! directory) waits for the game's first `PHYSFS_init`, on the game thread.
//! When startup fails nothing is hooked; a thread of its own writes the report
//! and tells the player.
pub type Result<T> = std::result::Result<T, String>;

#[macro_use]
pub mod log;
#[macro_use]
pub mod stub;
#[cfg(any(test, feature = "analysis"))]
pub mod analysis;
pub mod baseline;
pub mod ext;
pub mod framework_ui;
pub mod game;
pub mod game_string;
pub mod heap;
pub mod hooks;
pub mod hosts;
pub mod icu;
pub mod lang;
pub mod loc;
pub mod map;
pub mod memory;
pub mod mods;
pub mod notice;
pub mod objects;
pub mod paths;
pub mod pdx;
pub mod physfs;
pub mod pipe;
pub mod points;
pub mod remote;
pub mod report;
pub mod safe_point;
pub mod saves;
pub mod settings;
pub mod splice;
#[cfg(feature = "trace")]
mod trace;
pub mod ui;
pub mod unicode;
pub mod wording;

use std::sync::{
    atomic::{AtomicU32, Ordering},
    OnceLock,
};
use windows_sys::Win32::System::{
    LibraryLoader::GetModuleHandleW,
    Threading::{GetCurrentProcess, GetCurrentThreadId, TerminateProcess},
};

static GAME_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
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
/// carry on from: it is logged, the player is told, and the game ends
/// (ADR 0012).
pub fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(payload) => {
            let text = payload.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| payload.downcast_ref::<String>().cloned());
            fatal(&format!("{what}: {}", text.unwrap_or_else(|| "panic".into())))
        }
    }
}

/// Ends the game after an unexpected error in framework code (ADR 0012).
pub(crate) fn fatal(detail: &str) -> ! {
    log!("fatal error, ending the game: {detail}");
    notice::tell_and_wait(&notice::Message::new("EU4HARMONY_FATAL").arg("DETAIL", detail));
    unsafe { TerminateProcess(GetCurrentProcess(), 3) };
    unreachable!("the process was terminated")
}

/// Every dependency point, in one declaration list.
pub(crate) fn points() -> Vec<points::Point> {
    let mut all = physfs::points();
    all.extend(objects::points());
    all.extend(game_string::points());
    all.extend(game::points());
    all.extend(ui::points());
    all.extend(map::points());
    all.extend(saves::points());
    all.extend(unicode::points());
    all.extend(wording::points());
    all.extend(framework_ui::hook::points());
    all
}

fn start(m: memory::Memory) -> Result<String> {
    let baseline = baseline::recognize(&m)?;
    icu::load()?;
    // Lets the failure path be tried on a supported game: run eu4.exe with it set.
    if std::env::var_os("EU4HARMONY_FAIL_STARTUP").is_some() {
        return Err("EU4HARMONY_FAIL_STARTUP is set".into());
    }
    let all = points();
    #[cfg(feature = "trace")]
    let all: Vec<_> = {
        let observers = all.iter().filter(|p| matches!(p.action, points::Action::Observe { .. })).count();
        let room = hooks::observer_slots().saturating_sub(observers);
        all.into_iter().chain(trace::points(&m, room)).collect()
    };
    let installed = points::install(m, &all)?;
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

fn bootstrap() -> Result<()> {
    MAIN_THREAD.store(thread_id(), Ordering::Relaxed);
    let game_dir = paths::game_dir()?;
    // The log stays buffered until the user directory is known (deferred_init).
    log!("eu4-harmony core {} (api {}) bootstrap in {}", env!("CARGO_PKG_VERSION"), eu4_harmony_abi::API_VERSION, game_dir.display());
    let _ = GAME_DIR.set(game_dir);
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
    let logs = user_dir.join("eu4-harmony").join("logs");
    if let Err(e) = log::open(&logs.join("core.log")) {
        open_fallback_log();
        log!("cannot open {}: {e}", logs.join("core.log").display());
    }
    report::open(logs.join("report.txt"));
    logs
}

/// The report's first lines: this core, and the game folder when it is known.
fn report_header(game_dir: Option<&std::path::Path>) {
    report::line(format!("EU4 Harmony core {} (api {}), started {}", env!("CARGO_PKG_VERSION"), eu4_harmony_abi::API_VERSION, log::now()));
    if let Some(dir) = game_dir {
        report::line(format!("game: {}", dir.display()));
    }
}

/// Startup failed and nothing was hooked: writes the log and report and tells
/// the player, from a thread of its own since the loader lock is held here.
fn fail_startup(reason: String) {
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
        if let Some(dir) = &user_dir {
            lang::decide(dir);
        }
        // A changed game can fail every point; the report keeps the full list.
        const SHOWN: usize = 8;
        let lines: Vec<&str> = reason.lines().collect();
        let detail = match lines.len().checked_sub(SHOWN) {
            Some(more) if more > 0 => format!("{}\n… ({more} more in eu4-harmony/logs/report.txt)", lines[..SHOWN].join("\n")),
            _ => reason.clone(),
        };
        notice::tell_and_wait(&notice::Message::new("EU4HARMONY_STARTUP_FAILED").arg("DETAIL", detail));
    });
}

/// A font chosen in the settings that is no longer installed: the default
/// draws instead, and the player is told once (ADR 0012). A weight its font
/// no longer has draws in the one below it, which only the log tells.
fn tell_missing_fonts(chosen: &settings::Settings) {
    let Ok(fonts) = unicode::system_font::SystemFonts::new() else { return };
    for font in chosen.missing_fonts(|family| fonts.has_family(family)) {
        log!("settings: the font {font:?} is not installed; {} draws instead", settings::DEFAULT_FONT);
        notice::post(&format!("font:{font}"), notice::Message::new("EU4HARMONY_FONT_MISSING").arg("FONT", font).arg("DEFAULT", settings::DEFAULT_FONT));
    }
    for face in [&chosen.body_font, &chosen.heading_font, &chosen.map_font] {
        if let Some(drawn) = fonts.drawn(face).filter(|drawn| drawn != face) {
            log!("settings: the font {:?} has no weight {}; {} draws instead", face.family, face.weight, drawn.weight);
        }
    }
}

/// The mods of the playset with an EU4 Harmony manifest, in its order, and
/// what was skipped, which the player is told of; None when the playset
/// cannot be read.
fn discover(user_dir: &std::path::Path) -> Option<(Vec<mods::DiscoveredMod>, Vec<mods::Skipped>)> {
    let (mods, notes) = match mods::discover(user_dir) {
        Ok(v) => v,
        Err(e) => {
            log!("mod discovery failed: {e}");
            report::line(format!("mods: discovery failed: {e}"));
            report::write();
            return None;
        }
    };
    for note in &notes {
        log!("discovery: {}", note.text);
        if let Some(message) = &note.notice {
            notice::post(&format!("skipped:{}", note.text), message.clone());
        }
    }
    // Lets the notice window be tried without a broken mod: run eu4.exe with it set.
    if std::env::var_os("EU4HARMONY_TEST_NOTICE").is_some() {
        for number in ["1", "2"] {
            notice::post(&format!("test:{number}"), notice::Message::new("EU4HARMONY_TEST_NOTICE").arg("NUMBER", number));
        }
    }
    for m in &mods {
        log!(
            "mod {:?} id={:?} api={} dir={} extension={:?} ui={}",
            m.descriptor.name,
            m.manifest.id,
            m.manifest.api_version,
            m.dir.display(),
            m.manifest.extension,
            m.manifest.ui.len()
        );
    }
    Some((mods, notes))
}

/// The report's part on the mods: each one and what came of it, then every
/// note from discovering, splicing and their texts.
fn report_mods(mods: &[mods::DiscoveredMod], outcomes: &[ext::LoadOutcome], notes: &[String], plan: &splice::Plan) {
    report::line(format!("\nMods with an EU4 Harmony manifest, in playset order: {}", mods.len()));
    for (m, outcome) in mods.iter().zip(outcomes) {
        report::line(format!(
            "  {} (id {}, api {}): {}; {} UI fragment(s)",
            m.descriptor.name,
            m.manifest.id,
            m.manifest.api_version,
            outcome.status,
            m.manifest.ui.len()
        ));
        report::line(format!("    {}", m.dir.display()));
    }
    for note in notes {
        report::line(format!("  {note}"));
    }
    report::line(format!("UI: {} mod element(s) spliced, {} button-area slot(s)", plan.elements.len(), plan.slots));
    report::write();
}

/// Runs once, on the game thread, from the first PhysFS call.
pub(crate) fn deferred_init() {
    let game_dir = GAME_DIR.get().cloned().unwrap_or_default();
    let user_dir = match paths::user_dir(&game_dir) {
        Ok(d) => d,
        Err(e) => {
            log!("user dir: {e}");
            open_fallback_log();
            return;
        }
    };
    let logs = open_logs(&user_dir);
    report_header(Some(&game_dir));
    report::line(format!("user dir: {}", user_dir.display()));
    log!("user dir {}", user_dir.display());
    let language = lang::decide(&user_dir);
    let chosen = settings::load(&user_dir);
    log!("settings: {chosen:?}");
    tell_missing_fonts(&chosen);
    report::line(format!("language: {} ({}); the game runs in {}", language.player.id(), language.reason, language.game.id()));
    report::line(format!("startup: {}", STARTUP.get().map_or("?", String::as_str)));
    // Remote control is always on (ADR 0017).
    pipe::start();
    if let Some(m) = MEMORY.get().copied() {
        if let Err(e) = saves::initialize(m, user_dir.join("eu4-harmony").join("saves")) {
            log!("save data: {e}");
        }
    }
    let Some((mods, discovery_notes)) = discover(&user_dir) else { return };
    let outcomes = ext::load(&mods, &logs);
    let indices = ext::extension_indices(&mods);
    let included = |i: usize| outcomes[i].included;
    let (mut plan, ui_notes) = splice::build(&mods, included);
    for note in &ui_notes {
        log!("ui: {note}");
    }
    let taking_part: Vec<_> = mods.iter().enumerate().filter(|(i, _)| included(*i)).map(|(_, m)| m).collect();
    let loc_notes = loc::install(language, &taking_part, &mut plan);
    for note in &loc_notes {
        log!("localisation: {note}");
    }
    log!("splice plan ready: {} mod element(s), {} button slot(s)", plan.elements.len(), plan.slots);
    let notes: Vec<String> = discovery_notes
        .iter()
        .map(|n| format!("discovery: {}", n.text))
        .chain(ui_notes.iter().map(|n| format!("ui: {n}")))
        .chain(loc_notes.iter().map(|n| format!("localisation: {n}")))
        .collect();
    report_mods(&mods, &outcomes, &notes, &plan);
    ui::set_elements(plan.elements.iter().map(|e| (e.clone(), indices.get(&e.owner).copied())).collect());
    physfs::set_plan(plan);
}

/// Called by the proxy from its DllMain. Returns 0 on success.
#[no_mangle]
pub extern "system" fn eu4_harmony_bootstrap() -> u32 {
    match std::panic::catch_unwind(bootstrap) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            log!("startup failed: {e}");
            fail_startup(e);
            1
        }
        Err(_) => {
            log!("startup failed: panic");
            fail_startup("panic during startup".into());
            2
        }
    }
}
