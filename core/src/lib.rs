//! EU4 Harmony core. Loaded by the `winmm.dll` proxy during process start;
//! `eu4_harmony_bootstrap` runs under the loader lock, so it only checks the
//! executable and creates hooks. Everything that needs the game's own setup
//! (its file system, the user directory) waits for the game's first
//! `PHYSFS_init`, on the game thread.
pub type Result<T> = std::result::Result<T, String>;

#[macro_use]
pub mod log;
pub mod baseline;
pub mod ext;
pub mod game;
pub mod hooks;
pub mod lang;
pub mod loc;
pub mod map;
pub mod memory;
pub mod mods;
pub mod notice;
pub mod paths;
pub mod pdx;
pub mod physfs;
pub mod report;
pub mod saves;
pub mod splice;
pub mod text;
pub mod ui;

use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Mutex, OnceLock,
};
use windows_sys::Win32::System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId};

static GAME_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);
/// The running executable matches a registered baseline; game hooks are live.
pub(crate) static BASELINE_OK: AtomicBool = AtomicBool::new(false);
static BASELINE_ERROR: Mutex<Option<String>> = Mutex::new(None);
static MEMORY: OnceLock<memory::Memory> = OnceLock::new();

pub fn thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}

/// The thread that started the process, which runs the game's UI and updates.
pub fn main_thread() -> u32 {
    MAIN_THREAD.load(Ordering::Relaxed)
}

/// Runs `f`, logging instead of unwinding into game code.
pub fn guarded(what: &str, f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        log!("panic contained in {what}");
    }
}

fn dependencies() -> [&'static [baseline::Dependency]; 7] {
    [game::DEPENDENCIES, game::world::DEPENDENCIES, game::orders::DEPENDENCIES, game::events::DEPENDENCIES, ui::DEPENDENCIES, map::DEPENDENCIES, saves::DEPENDENCIES]
}

fn install_game_hooks(m: memory::Memory) -> Result<()> {
    game::events::install(m)?;
    ui::install(m)?;
    map::install(m)?;
    saves::install(m)?;
    Ok(())
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
    match baseline::check(&m, &dependencies()) {
        Ok(name) => match install_game_hooks(m) {
            Ok(()) => {
                log!("baseline {name} recognized; game hooks created");
                BASELINE_OK.store(true, Ordering::Release);
            }
            Err(e) => *BASELINE_ERROR.lock().unwrap() = Some(format!("game hooks failed: {e}")),
        },
        Err(e) => *BASELINE_ERROR.lock().unwrap() = Some(e),
    }
    if let Some(e) = BASELINE_ERROR.lock().unwrap().as_ref() {
        log!("executable not supported, extensions will not run: {e}");
    }
    physfs::install()?;
    hooks::enable_all()?;
    log!("hooks enabled");
    Ok(())
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
    let logs = user_dir.join("eu4-harmony").join("logs");
    if let Err(e) = log::open(&logs.join("core.log")) {
        open_fallback_log();
        log!("cannot open {}: {e}", logs.join("core.log").display());
    }
    report::open(logs.join("report.txt"));
    report::line(format!("EU4 Harmony core {} (api {}), started {}", env!("CARGO_PKG_VERSION"), eu4_harmony_abi::API_VERSION, log::now()));
    report::line(format!("game: {}", game_dir.display()));
    report::line(format!("user dir: {}", user_dir.display()));
    log!("user dir {}", user_dir.display());
    let language = lang::decide(&user_dir);
    report::line(format!("language: {} ({}); the game runs in {}", language.player.id(), language.reason, language.game.id()));
    let supported = BASELINE_OK.load(Ordering::Acquire);
    report::line(match BASELINE_ERROR.lock().unwrap().as_ref() {
        None => "executable: supported".to_string(),
        Some(e) => format!("executable: NOT supported, no extension runs: {e}"),
    });
    let memory = MEMORY.get().copied().filter(|_| supported);
    if let Some(m) = memory {
        if let Err(e) = saves::initialize(m, user_dir.join("eu4-harmony").join("saves")) {
            log!("save data: {e}");
        }
    }
    let (mods, notes) = match mods::discover(&user_dir) {
        Ok(v) => v,
        Err(e) => {
            log!("mod discovery failed: {e}");
            report::line(format!("mods: discovery failed: {e}"));
            report::write();
            return;
        }
    };
    for note in &notes {
        log!("discovery: {}", note.text);
        if let Some(message) = &note.notice {
            notice::post(&format!("skipped:{}", note.text), message.clone());
        }
    }
    for m in &mods {
        log!("mod {:?} id={:?} api={} dir={} extension={:?} ui={}", m.descriptor.name, m.manifest.id, m.manifest.api_version, m.dir.display(), m.manifest.extension, m.manifest.ui.len());
    }
    if !supported {
        notice::fall_back("the executable is not supported");
    }
    if !supported && mods.iter().any(|m| m.manifest.extension.is_some() || !m.manifest.ui.is_empty()) {
        let reason = BASELINE_ERROR.lock().unwrap().clone().unwrap_or_default();
        notice::post("baseline", notice::Message::new("EU4HARMONY_UNSUPPORTED_EXE").arg("DETAIL", reason));
    }
    let outcomes = ext::load(memory, &mods, &logs);
    let indices = ext::extension_indices(&mods);
    let included = |i: usize| supported && outcomes[i].included;
    let (mut plan, ui_notes) = splice::build(&mods, included);
    for note in &ui_notes {
        log!("ui: {note}");
    }
    let taking_part: Vec<_> = mods.iter().enumerate().filter(|(i, _)| included(*i)).map(|(_, m)| m).collect();
    let loc_notes = loc::install(&taking_part, &mut plan);
    for note in &loc_notes {
        log!("localisation: {note}");
    }
    let mod_elements = plan.elements.len();
    let notices = if !supported {
        "message boxes".to_string()
    } else if let Err(e) = notice::splice(&mut plan) {
        notice::fall_back(&format!("notice window: {e}"));
        format!("message boxes ({e})")
    } else {
        "notice window in the main menu and the top bar".to_string()
    };
    log!("splice plan ready: {mod_elements} mod element(s), {} button slot(s); notices: {notices}", plan.slots);
    report::line(format!("\nMods with an EU4 Harmony manifest, in playset order: {}", mods.len()));
    for (m, outcome) in mods.iter().zip(&outcomes) {
        report::line(format!("  {} (id {}, api {}): {}; {} UI fragment(s)", m.descriptor.name, m.manifest.id, m.manifest.api_version, outcome.status, m.manifest.ui.len()));
        report::line(format!("    {}", m.dir.display()));
    }
    let all_notes = notes.iter().map(|n| format!("discovery: {}", n.text)).chain(ui_notes.iter().map(|n| format!("ui: {n}"))).chain(loc_notes.iter().map(|n| format!("localisation: {n}")));
    for note in all_notes {
        report::line(format!("  {note}"));
    }
    report::line(format!("UI: {mod_elements} mod element(s) spliced, {} button-area slot(s)", plan.slots));
    report::line(format!("notices: {notices}"));
    report::write();
    let extension = |owner: &splice::Owner| match owner {
        splice::Owner::Mod(i) => indices.get(i).copied(),
        splice::Owner::Framework => None,
    };
    ui::set_elements(plan.elements.iter().map(|e| (e.clone(), extension(&e.owner))).collect());
    physfs::set_plan(plan);
}

/// Called by the proxy from its DllMain. Returns 0 on success.
#[no_mangle]
pub extern "system" fn eu4_harmony_bootstrap() -> u32 {
    match std::panic::catch_unwind(bootstrap) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            log!("bootstrap failed: {e}");
            open_fallback_log();
            1
        }
        Err(_) => 2,
    }
}
