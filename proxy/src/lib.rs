//! `version.dll` proxy (ADR 0011). The Windows loader maps this DLL into the
//! game process because `eu4.exe` imports VERSION.dll and the game directory
//! is searched before System32. Every export is forwarded to the System32
//! version.dll; on process attach the EU4 Harmony core is loaded and
//! bootstrapped. A `version.dll` that was in the game root before (a font
//! plugin's loader, say) is kept as a backup in `eu4-harmony/` and never
//! loaded, so only the framework changes the game's text code.
//!
//! DllMain runs under the loader lock, so it only loads the core and calls its
//! bootstrap export; all further initialization is deferred by the core itself.
use std::{
    ffi::c_void,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HMODULE},
    System::{
        LibraryLoader::{
            DisableThreadLibraryCalls, GetModuleFileNameW, GetProcAddress, LoadLibraryExW,
            LOAD_WITH_ALTERED_SEARCH_PATH,
        },
        SystemInformation::GetSystemDirectoryW,
        SystemServices::DLL_PROCESS_ATTACH,
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

fn report(message: &str) {
    if let Some(dir) = GAME_DIR.get() {
        let logs = dir.join("eu4-harmony").join("logs");
        let _ = fs::create_dir_all(&logs);
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(logs.join("proxy.log")) {
            let _ = writeln!(f, "{message}");
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
    let harmony = dir.join("eu4-harmony");

    // Other programs in the game folder (the launcher's helpers) can pull this
    // DLL in too; they only get forwarding.
    let n = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let host = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    if !host.file_name().is_some_and(|f| f.eq_ignore_ascii_case("eu4.exe")) {
        return;
    }

    let core_path = harmony.join("eu4_harmony_core.dll");
    let core = unsafe { LoadLibraryExW(wide(&core_path).as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
    if core.is_null() {
        report(&format!("failed to load {}: error {}", core_path.display(), unsafe { GetLastError() }));
        return;
    }
    let Some(bootstrap) = (unsafe { GetProcAddress(core, c"eu4_harmony_bootstrap".as_ptr() as *const u8) }) else {
        report("eu4_harmony_core.dll has no eu4_harmony_bootstrap export");
        return;
    };
    let bootstrap: extern "system" fn() -> u32 = unsafe { std::mem::transmute(bootstrap) };
    let code = bootstrap();
    if code != 0 {
        report(&format!("eu4_harmony_bootstrap returned {code}"));
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(instance: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // Never let a panic unwind into the loader.
        let _ = std::panic::catch_unwind(|| attach(instance));
    }
    1
}
