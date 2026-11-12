//! `version.dll` proxy (ADR version-proxy-injection). The Windows loader maps
//! this DLL into the game process because `eu4.exe` imports VERSION.dll and the
//! game directory is searched before System32. Every export is forwarded to the
//! System32 version.dll; on process attach the Harmony core is loaded and
//! bootstrapped. A `version.dll` that was in the game root before is kept as a
//! backup in `eu4-harmony/` and never loaded, so only the framework changes the
//! game's text code.
//!
//! The core is the one `-harmony_core=<path>` on the game's command line
//! names, as a test game runs a core of its own (docs/plans/test-games.md); a
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
    fs,
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

/// Harmony's published file id on the Steam Workshop, from
/// `workshop/item.toml`; 0 until the item is created.
const PUBLISHED_FILE_ID: u64 = include!(concat!(env!("OUT_DIR"), "/published_file_id.rs"));
/// The game's Steam app id, under which the Workshop keeps its items.
const APP_ID: &str = "236850";

/// Writes a line to `proxy.log`, in the user folder's `eu4-harmony\logs\`
/// beside the core's log, or in the game folder's when the user folder
/// cannot be found.
fn report(message: &str) {
    static LOGS: OnceLock<Option<PathBuf>> = OnceLock::new();
    let logs = LOGS.get_or_init(|| {
        let game = GAME_DIR.get()?;
        let base = user_dir_of(&command_line(), game, documents().as_deref()).unwrap_or_else(|| game.clone());
        Some(base.join("eu4-harmony").join("logs"))
    });
    static JOURNAL: Mutex<Option<eu4_harmony_logging::Journal>> = Mutex::new(None);
    if let Some(logs) = logs {
        // Never wait on another thread while under the Windows loader lock.
        if let Ok(mut journal) = JOURNAL.try_lock() {
            if journal.is_none() {
                *journal = eu4_harmony_logging::Journal::open(logs, "proxy").ok();
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
            let places = core_places(&dir, PUBLISHED_FILE_ID);
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

/// Where the core is looked for when no `-harmony_core` names one, in order:
/// beside this DLL, as `cargo xtask install` puts it, then in Harmony's own
/// Workshop item, when the game folder is in a Steam library and the item's
/// id is known. Never in another item: any mod could ship a file of the
/// core's name, and the core runs with every right in the game
/// (ADR the-framework-ships-on-the-workshop).
fn core_places(game_dir: &Path, published_file_id: u64) -> Vec<PathBuf> {
    let mut places = vec![game_dir.join("eu4-harmony").join("eu4_harmony_core.dll")];
    if let Some(item) = workshop_item(game_dir, published_file_id) {
        places.push(item.join("eu4-harmony").join("eu4_harmony_core.dll"));
    }
    places
}

/// The folder Steam keeps the Workshop item `published_file_id` in, for the
/// game in `game_dir`: `<library>\steamapps\workshop\content\236850\<id>`
/// when the game is `<library>\steamapps\common\<game>`; none while the id is
/// 0, the item not yet created.
fn workshop_item(game_dir: &Path, published_file_id: u64) -> Option<PathBuf> {
    if published_file_id == 0 {
        return None;
    }
    let common = game_dir.parent()?;
    let steamapps = common.parent()?;
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(name));
    (named(common, "common") && named(steamapps, "steamapps"))
        .then(|| steamapps.join("workshop").join("content").join(APP_ID).join(published_file_id.to_string()))
}

/// The game's command line.
fn command_line() -> String {
    unsafe {
        let line = GetCommandLineW();
        let len = (0..).take_while(|&i| *line.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(line, len))
    }
}

/// The value of the switch `-<name>=<value>` among the command line's words,
/// the last when there are several, none when it is empty, split as
/// `eu4_harmony_folders::switch_value` splits them; the proxy does not link
/// the folders crate, so as to load nothing but the core.
fn switch(line: &str, name: &str) -> Option<String> {
    let mut words = Vec::new();
    let (mut word, mut quoted, mut any) = (String::new(), false, false);
    for c in line.chars() {
        match c {
            '"' => (quoted, any) = (!quoted, true),
            ' ' | '\t' if !quoted => {
                if any {
                    words.push(std::mem::take(&mut word));
                }
                any = false;
            }
            _ => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
        .into_iter()
        .rev()
        .find_map(|w| {
            let (switch, value) = w.split_once('=')?;
            let switch = switch.strip_prefix("--").or_else(|| switch.strip_prefix('-'))?;
            switch.eq_ignore_ascii_case(name).then(|| value.to_string())
        })
        .filter(|v| !v.is_empty())
}

/// The game's user folder, as `eu4_harmony_folders::user_dir_of` finds it:
/// the folder `-userdir` names, else the one `userdir.txt` in the game
/// folder names, else `Documents\Paradox Interactive\Europa Universalis IV`,
/// `documents` being the Documents folder when it is known.
fn user_dir_of(line: &str, game_dir: &Path, documents: Option<&Path>) -> Option<PathBuf> {
    let relative = |path: PathBuf| if path.is_absolute() { path } else { game_dir.join(path) };
    if let Some(named) = switch(line, "userdir") {
        return Some(relative(PathBuf::from(named)));
    }
    if let Ok(text) = fs::read_to_string(game_dir.join("userdir.txt")) {
        let text = text.trim_start_matches('\u{feff}').trim();
        if !text.is_empty() {
            return Some(relative(PathBuf::from(text)));
        }
    }
    Some(documents?.join("Paradox Interactive").join("Europa Universalis IV"))
}

/// The Windows user's Documents folder, read from the registry, where the
/// shell keeps it: asking the shell would load it into the process under the
/// loader lock, where this runs.
fn documents() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders\0".encode_utf16().collect();
    let value: Vec<u16> = "Personal\0".encode_utf16().collect();
    let mut buf = vec![0u16; 32768];
    let mut size = (buf.len() * 2) as u32;
    // A REG_EXPAND_SZ value comes back expanded, as a REG_SZ.
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size) };
    if status != 0 {
        return std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Documents"));
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
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
        for line in [
            r#""E:\Steam\Europa Universalis IV\eu4.exe" -harmony_hidden "-harmony_core=C:\a b\core.dll" -userdir=D:\u"#,
            r"eu4.exe -HARMONY_CORE=D:\one.dll -harmony_core=D:\two.dll",
            "eu4.exe -harmony_core= -debug",
            r"eu4.exe -harmony_corex=D:\x.dll",
            r#"--E:\eu4.exe --harmony_core="E:\a b\core.dll" --harmony_hidden -skiplauncher"#,
            r"eu4.exe harmony_core=D:\x.dll ---harmony_core=D:\y.dll",
            "eu4.exe",
        ] {
            let expected = eu4_harmony_folders::switch_value(line, "-harmony_core").filter(|v| !v.is_empty());
            assert_eq!(super::switch(line, "harmony_core"), expected, "{line}");
        }
        assert_eq!(super::switch(r"eu4.exe -harmony_core=D:\two.dll", "harmony_core").unwrap(), r"D:\two.dll");
    }

    /// The core is looked for beside the proxy, then in Harmony's own
    /// Workshop item of the library that holds the game, and nowhere else.
    #[test]
    fn the_core_is_looked_for_beside_the_proxy_then_in_harmony_s_item() {
        use std::path::{Path, PathBuf};
        let game = Path::new(r"E:\SteamLibrary\steamapps\common\Europa Universalis IV");
        assert_eq!(
            super::core_places(game, 3_123_456_789),
            [
                PathBuf::from(r"E:\SteamLibrary\steamapps\common\Europa Universalis IV\eu4-harmony\eu4_harmony_core.dll"),
                PathBuf::from(r"E:\SteamLibrary\steamapps\workshop\content\236850\3123456789\eu4-harmony\eu4_harmony_core.dll"),
            ]
        );
        let upper = Path::new(r"D:\Steam\SteamApps\Common\eu4");
        assert_eq!(super::workshop_item(upper, 7), Some(PathBuf::from(r"D:\Steam\SteamApps\workshop\content\236850\7")));
        // The item not created yet, or a game folder outside a Steam library.
        assert_eq!(super::core_places(game, 0).len(), 1);
        assert_eq!(super::workshop_item(Path::new(r"D:\Games\eu4"), 7), None);
        assert_eq!(super::workshop_item(Path::new(r"D:\steamapps\eu4"), 7), None);
        assert_eq!(super::workshop_item(Path::new(r"C:\"), 7), None);
    }

    /// The proxy's log goes into the user folder the framework's other
    /// programs find.
    #[test]
    fn the_user_folder_is_the_one_the_folders_crate_finds() {
        let game = std::env::temp_dir().join(format!("eu4h-proxy-userdir-{}", std::process::id()));
        std::fs::create_dir_all(&game).unwrap();
        let documents = super::documents();
        for (userdir_txt, line) in [
            (None, "eu4.exe"),
            (None, r"eu4.exe -userdir=D:\u"),
            (None, r#"eu4.exe "--userdir=rel dir""#),
            (Some("\u{feff}  D:\\named\n"), "eu4.exe"),
            (Some("named"), "eu4.exe"),
            (Some("   "), "eu4.exe"),
            (Some(r"D:\named"), r"eu4.exe -userdir=D:\wins"),
        ] {
            match userdir_txt {
                Some(text) => std::fs::write(game.join("userdir.txt"), text).unwrap(),
                None => {
                    let _ = std::fs::remove_file(game.join("userdir.txt"));
                }
            }
            let expected = eu4_harmony_folders::user_dir_of(line, Some(&game)).unwrap();
            assert_eq!(super::user_dir_of(line, &game, documents.as_deref()), Some(expected), "{line} {userdir_txt:?}");
        }
        std::fs::remove_dir_all(&game).unwrap();
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
