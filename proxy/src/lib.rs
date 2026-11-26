//! `version.dll` proxy (ADR version-proxy-injection). The Windows loader maps
//! this DLL into the game process because `eu4.exe` imports VERSION.dll and the
//! game directory is searched before System32. Every export is forwarded to the
//! System32 version.dll; on process attach the Harmony core is loaded and
//! bootstrapped. A `version.dll` that was in the game root before is kept as a
//! backup in `eu4-harmony/` and never loaded, so only the framework changes the
//! game's text code.
//!
//! The core is the one `-harmony_core=<path>` on the game's command line
//! names, as a test game runs a core of its own (docs/dev-tools.md#test-games); a
//! core named there that does not load ends the game, which would otherwise
//! run without the core it was started for. Else it is the first there is of
//! `eu4-harmony\eu4_harmony_core.dll` beside this DLL, where a developer
//! installs it, and the same file in Harmony's own Workshop item, where
//! players get it and Steam updates it (ADR the-framework-ships-on-the-workshop);
//! without either the game runs unmodified. The proxy logs what goes wrong
//! into `proxy.log` in the user folder's `eu4-harmony\logs\`, beside the
//! core's log.
//!
//! DllMain runs under the loader lock, so it only loads the core and calls its
//! bootstrap export; all further initialization is deferred by the core itself.
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HMODULE},
    System::{
        Environment::GetCommandLineW,
        LibraryLoader::{DisableThreadLibraryCalls, GetModuleFileNameW, GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH},
        SystemInformation::GetSystemDirectoryW,
        SystemServices::DLL_PROCESS_ATTACH,
        Threading::{GetCurrentProcess, TerminateProcess},
    },
};

include!(concat!(env!("OUT_DIR"), "/stubs.rs"));

/// Resolved addresses of the real version.dll exports, indexed like
/// `EXPORT_NAMES`. Read by the assembly stubs; zero means "not resolved yet".
static TABLE: [AtomicUsize; EXPORT_COUNT] = [const { AtomicUsize::new(0) }; EXPORT_COUNT];
/// HMODULE of the DLL we forward to.
static FORWARD: AtomicUsize = AtomicUsize::new(0);
static GAME_DIR: OnceLock<PathBuf> = OnceLock::new();

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain([0]).collect()
}

fn system_version() -> Vec<u16> {
    let mut buf = [0u16; 512];
    let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let mut path = buf[..n.min(buf.len())].to_vec();
    path.extend("\\version.dll".encode_utf16());
    path.push(0);
    path
}

fn forward_module() -> HMODULE {
    let current = FORWARD.load(Ordering::Acquire);
    if current != 0 {
        return current as HMODULE;
    }
    let path = system_version();
    let module = unsafe { LoadLibraryExW(path.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
    if module.is_null() {
        report(&format!("LoadLibrary of forward target failed: {}", unsafe { GetLastError() }));
        return std::ptr::null_mut();
    }
    match FORWARD.compare_exchange(0, module as usize, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => module,
        Err(existing) => existing as HMODULE,
    }
}

/// Called by a stub whose table slot is still empty. Returns the address to jump to.
extern "C" fn resolve(index: u32) -> usize {
    let index = index as usize;
    let cached = TABLE[index].load(Ordering::Acquire);
    if cached != 0 {
        return cached;
    }
    let module = forward_module();
    let name = EXPORT_NAMES[index];
    let address = if module.is_null() {
        0
    } else if let Some(ordinal) = name.strip_prefix('#') {
        let ordinal: usize = ordinal.parse().unwrap_or(0);
        unsafe { GetProcAddress(module, ordinal as *const u8) }.map_or(0, |f| f as usize)
    } else {
        let name = format!("{name}\0");
        unsafe { GetProcAddress(module, name.as_ptr()) }.map_or(0, |f| f as usize)
    };
    let address = if address == 0 {
        report(&format!("export {name} not found in forward target"));
        missing_export as *const () as usize
    } else {
        address
    };
    TABLE[index].store(address, Ordering::Release);
    address
}

/// Stand-in for an export the forward target lacks: returns 0 (FALSE / NULL).
extern "C" fn missing_export() -> usize {
    0
}

/// What the proxy offers the core besides loading it, which a core reads
/// through [`eu4_harmony_proxy_interface`]. Raised when that changes, so
/// that a core needing more than an older proxy offers can tell the player
/// to copy `version.dll` again: Steam updates the core, never this file.
/// 1: the proxy that finds the core in Harmony's Workshop item. A proxy
/// without the export is 0.
const INTERFACE: u32 = 1;

/// The interface this proxy offers the core. A core reads it from the
/// module, so it can tell an old proxy (no export, 0) from a new one.
#[no_mangle]
pub extern "system" fn eu4_harmony_proxy_interface() -> u32 {
    INTERFACE
}

/// Writes a line to `proxy.log`, in the user folder's `eu4-harmony\logs\`
/// beside the core's log, or in the game folder's when the user folder
/// cannot be found.
fn report(message: &str) {
    static LOGS: OnceLock<Option<PathBuf>> = OnceLock::new();
    let logs = LOGS.get_or_init(|| {
        let game = GAME_DIR.get()?;
        let base = eu4_harmony_folders::user_dir_of(&command_line(), Some(game)).unwrap_or_else(|_| game.clone());
        Some(base.join("eu4-harmony").join("logs"))
    });
    static JOURNAL: Mutex<Option<eu4_harmony_logging::Journal>> = Mutex::new(None);
    if let Some(logs) = logs {
        // Never wait on another thread while under the Windows loader lock.
        if let Ok(mut journal) = JOURNAL.try_lock() {
            if journal.is_none() {
                *journal = eu4_harmony_logging::Journal::open(logs, eu4_harmony_logging::PROXY).ok();
            }
            if let Some(journal) = journal.as_mut() {
                let _ = journal.write(eu4_harmony_logging::Kind::Critical, message);
            }
        }
    }
}

fn attach(instance: HMODULE) {
    unsafe { DisableThreadLibraryCalls(instance) };
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(instance, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let own = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    let Some(dir) = own.parent().map(Path::to_path_buf) else { return };
    let _ = GAME_DIR.set(dir.clone());

    // Other programs in the game folder (the launcher's helpers) can pull this
    // DLL in too; they only get forwarding.
    let n = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let host = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    if !host.file_name().is_some_and(|f| f.eq_ignore_ascii_case("eu4.exe")) {
        return;
    }

    let named = switch(&command_line(), "harmony_core").map(PathBuf::from);
    let core_path = match &named {
        Some(path) => path.clone(),
        None => {
            let places = eu4_harmony_folders::core_places(&dir, eu4_harmony_folders::PUBLISHED_FILE_ID);
            match places.iter().find(|p| p.is_file()) {
                Some(found) => found.clone(),
                None => {
                    let looked: Vec<String> = places.iter().map(|p| p.display().to_string()).collect();
                    report(&format!("no core found, so the game runs without Harmony; looked at {}", looked.join(", ")));
                    return;
                }
            }
        }
    };
    let core = unsafe { LoadLibraryExW(wide(&core_path).as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
    if core.is_null() {
        report(&format!("failed to load {}: error {}", core_path.display(), unsafe { GetLastError() }));
        if named.is_some() {
            report("ending the game: the core -harmony_core names did not load");
            unsafe { TerminateProcess(GetCurrentProcess(), 3) };
        }
        return;
    }
    let Some(bootstrap) = (unsafe { GetProcAddress(core, c"eu4_harmony_bootstrap".as_ptr() as *const u8) }) else {
        report(&format!("{} has no eu4_harmony_bootstrap export", core_path.display()));
        return;
    };
    let bootstrap: extern "system" fn() -> u32 = unsafe { std::mem::transmute(bootstrap) };
    let code = bootstrap();
    if code != 0 {
        report(&format!("eu4_harmony_bootstrap of {} returned {code}", core_path.display()));
    }
}

/// The game's command line.
fn command_line() -> String {
    unsafe {
        let line = GetCommandLineW();
        let len = (0..).take_while(|&i| *line.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(line, len))
    }
}

fn switch(line: &str, name: &str) -> Option<String> {
    eu4_harmony_folders::switch_value(line, &format!("-{name}")).filter(|v| !v.is_empty())
}

#[no_mangle]
pub extern "system" fn DllMain(instance: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // Never let a panic unwind into the loader.
        let _ = std::panic::catch_unwind(|| attach(instance));
    }
    1
}

#[cfg(test)]
mod tests {
    /// The core is named as the framework reads its other switches.
    #[test]
    fn the_core_named_is_read_as_the_folders_crate_reads_a_switch() {
        assert_eq!(super::switch(r"eu4.exe -harmony_core=D:\two.dll", "harmony_core").as_deref(), Some(r"D:\two.dll"));
        assert_eq!(super::switch("eu4.exe -harmony_core= -debug", "harmony_core"), None);
    }

    /// The interface a core reads from the proxy.
    #[test]
    fn the_proxy_offers_its_interface() {
        assert_eq!(super::eu4_harmony_proxy_interface(), super::INTERFACE);
        const { assert!(super::INTERFACE >= 1, "a proxy without the export is 0") };
    }

    /// The names and ordinals the DLL at `path` exports, read from its file.
    fn exports(path: &std::path::Path) -> Vec<(String, u32)> {
        let file = std::fs::read(path).unwrap();
        let u16_at = |at: usize| u16::from_le_bytes([file[at], file[at + 1]]) as usize;
        let u32_at = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap()) as usize;
        let pe = u32_at(0x3c);
        let optional = pe + 24;
        let sections = optional + u16_at(pe + 20);
        // Where an RVA lies in the file.
        let at = |rva: usize| {
            (0..u16_at(pe + 6))
                .map(|i| sections + 40 * i)
                .find(|&s| (u32_at(s + 12)..u32_at(s + 12) + u32_at(s + 16)).contains(&rva))
                .map(|s| rva - u32_at(s + 12) + u32_at(s + 20))
                .unwrap()
        };
        let directory = at(u32_at(optional + 112));
        let (base, names, ordinals) = (u32_at(directory + 16), at(u32_at(directory + 32)), at(u32_at(directory + 36)));
        (0..u32_at(directory + 24))
            .map(|i| {
                let name = at(u32_at(names + 4 * i));
                let end = file[name..].iter().position(|&b| b == 0).unwrap();
                (String::from_utf8_lossy(&file[name..name + end]).into_owned(), (base + u16_at(ordinals + 2 * i)) as u32)
            })
            .collect()
    }

    /// The proxy forwards exactly what Windows' own version.dll exports.
    #[test]
    fn the_export_list_is_version_dll_s() {
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let mut real = exports(&std::path::Path::new(&system).join("System32").join("version.dll"));
        real.sort();
        let mut listed: Vec<(String, u32)> = include_str!("../version-exports.txt")
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with("# "))
            .map(|l| {
                let mut parts = l.split_whitespace();
                (parts.next().unwrap().to_string(), parts.next().unwrap().parse().unwrap())
            })
            .collect();
        listed.sort();
        assert_eq!(listed, real);
    }
}
