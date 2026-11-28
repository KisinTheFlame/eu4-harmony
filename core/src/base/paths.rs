//! Game and user directories, resolved the way the game itself does, the
//! game's command line, and the folder this core was loaded from.
use crate::base::Result;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

/// Full path to the running `eu4.exe`.
pub fn current_exe() -> Result<PathBuf> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return Err("GetModuleFileNameW failed or was truncated".into());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// Directory containing the running `eu4.exe`.
pub fn game_dir() -> Result<PathBuf> {
    let exe = current_exe()?;
    exe.parent().map(Path::to_path_buf).ok_or_else(|| "exe has no parent directory".into())
}

/// Canonicalized, lower-cased Windows path without `\\?\` prefix or trailing
/// separators, for case-insensitive path comparisons.
pub fn normalize_path(path: &Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.trim_end_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase()
}

/// Whether `a` and `b` refer to the same Windows path.
pub fn same_path(a: &Path, b: &Path) -> bool {
    normalize_path(a) == normalize_path(b)
}

/// The game's data directory: the folder `-userdir` on its command line
/// names, as the game takes it, else the one the command line tool finds too
/// ([`eu4_harmony_folders::user_dir`]).
pub fn user_dir(game_dir: &Path) -> Result<PathBuf> {
    eu4_harmony_folders::user_dir_of(command_line(), Some(game_dir))
}

/// The game's command line, read once.
pub fn command_line() -> &'static str {
    static LINE: OnceLock<String> = OnceLock::new();
    LINE.get_or_init(|| {
        use windows_sys::Win32::System::Environment::GetCommandLineW;
        unsafe {
            let line = GetCommandLineW();
            let len = (0..).take_while(|&i| *line.add(i) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(line, len))
        }
    })
}

/// The actual loaded core's DLL, including a test game's file named by
/// `-harmony_core` (docs/dev-tools.md#test-games).
pub fn core_file() -> Result<PathBuf> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};
    let mut module = std::ptr::null_mut();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    if unsafe { GetModuleHandleExW(flags, core_file as *const () as *const u16, &mut module) } == 0 {
        return Err("GetModuleHandleExW of the core failed".into());
    }
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return Err("GetModuleFileNameW of the core failed or was truncated".into());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// The folder containing the actual loaded core, not a guessed install path.
pub fn core_dir() -> Result<PathBuf> {
    let dll = core_file()?;
    dll.parent().map(Path::to_path_buf).ok_or_else(|| "the core has no folder".into())
}

/// Which distribution channel this running core was loaded from.
pub fn install_channel() -> eu4_harmony_folders::InstallChannel {
    static CHANNEL: OnceLock<eu4_harmony_folders::InstallChannel> = OnceLock::new();
    if let Some(&channel) = CHANNEL.get() {
        return channel;
    }
    let Some(game) = crate::base::game_dir() else {
        return eu4_harmony_folders::InstallChannel::Other;
    };
    let channel = core_dir().map_or(eu4_harmony_folders::InstallChannel::Other, |core| eu4_harmony_folders::install_channel(game, &core));
    *CHANNEL.get_or_init(|| channel)
}

/// English version string with optional city codename, for logs and reports
/// (for example `1.0.0 "Constantinople"` or `0.4.0`).
pub fn version_identity() -> String {
    match eu4_harmony_folders::VERSION_CODENAME {
        Some(codename) => format!("{} \"{codename}\"", env!("CARGO_PKG_VERSION")),
        None => env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Short English label for the active install channel in logs and reports.
pub fn channel_tag(channel: eu4_harmony_folders::InstallChannel) -> Option<&'static str> {
    match channel {
        eu4_harmony_folders::InstallChannel::Beta => Some("beta"),
        eu4_harmony_folders::InstallChannel::Public => Some("public"),
        eu4_harmony_folders::InstallChannel::Local => Some("local"),
        eu4_harmony_folders::InstallChannel::Other => None,
    }
}
