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
pub mod map;
pub mod memory;
pub mod mods;
pub mod paths;
pub mod pdx;
pub mod physfs;
pub mod saves;
pub mod splice;
pub mod text;
pub mod ui;

use std::collections::HashSet;
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

/// Tells the player about a problem once per `key`, without blocking the game.
pub fn notice(key: &str, text: &str) {
    static SHOWN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    log!("notice: {}", text.replace('\n', " "));
    if !SHOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).insert(key.to_string()) {
        return;
    }
    let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
    std::thread::spawn(move || {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND};
        let title: Vec<u16> = "EU4 Harmony\0".encode_utf16().collect();
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONWARNING | MB_SETFOREGROUND) };
    });
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

fn bootstrap() -> Result<()> {
    MAIN_THREAD.store(thread_id(), Ordering::Relaxed);
    let game_dir = paths::game_dir()?;
    log::open(&game_dir.join("eu4-harmony").join("logs").join("core.log")).map_err(|e| format!("open log: {e}"))?;
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
    let logs = game_dir.join("eu4-harmony").join("logs");
    let user_dir = match paths::user_dir(&game_dir) {
        Ok(d) => d,
        Err(e) => {
            log!("user dir: {e}");
            return;
        }
    };
    log!("user dir {}", user_dir.display());
    let supported = BASELINE_OK.load(Ordering::Acquire);
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
            return;
        }
    };
    for note in notes {
        log!("discovery: {note}");
    }
    for m in &mods {
        log!("mod {:?} id={:?} api={} dir={} extension={:?} ui={}", m.descriptor.name, m.manifest.id, m.manifest.api_version, m.dir.display(), m.manifest.extension, m.manifest.ui.len());
    }
    if !supported && mods.iter().any(|m| m.manifest.extension.is_some() || !m.manifest.ui.is_empty()) {
        let reason = BASELINE_ERROR.lock().unwrap().clone().unwrap_or_default();
        notice("baseline", &format!("This eu4.exe is not supported by EU4 Harmony, so mods built on it are inactive.\n\n{reason}"));
    }
    let included = ext::load(memory, &mods, &logs);
    let indices = ext::extension_indices(&mods);
    let (plan, notes) = splice::build(&mods, |i| supported && included[i]);
    for note in notes {
        log!("ui: {note}");
    }
    log!("splice plan ready: {} element(s), {} button slot(s)", plan.elements.len(), plan.slots);
    ui::set_elements(plan.elements.iter().map(|e| (e.clone(), indices.get(&e.owner).copied())).collect());
    physfs::set_plan(plan);
}

/// Called by the proxy from its DllMain. Returns 0 on success.
#[no_mangle]
pub extern "system" fn eu4_harmony_bootstrap() -> u32 {
    match std::panic::catch_unwind(bootstrap) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            log!("bootstrap failed: {e}");
            1
        }
        Err(_) => 2,
    }
}
