//! Game and user directories, resolved the way the game itself does, the
//! game's command line, and the folder this core was loaded from.
use crate::Result;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

/// Directory containing the running `eu4.exe`.
pub fn game_dir() -> Result<PathBuf> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 {
        return Err("GetModuleFileNameW failed".into());
    }
    let exe = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    exe.parent().map(Path::to_path_buf).ok_or_else(|| "exe has no parent directory".into())
}

/// The game's data directory: the folder `-userdir` on its command line
/// names, as the game takes it, else the one the command line tool finds too
/// ([`eu4_harmony_manifest::user_dir`]).
pub fn user_dir(game_dir: &Path) -> Result<PathBuf> {
    eu4_harmony_manifest::user_dir_of(command_line(), Some(game_dir))
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

/// The folder this core's DLL was loaded from: `eu4-harmony\` in the game
/// folder, or the test game's own, which `-harmony_core` names
/// (docs/test-games.md).
pub fn core_dir() -> Result<PathBuf> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};
    let mut module = std::ptr::null_mut();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    if unsafe { GetModuleHandleExW(flags, core_dir as *const () as *const u16, &mut module) } == 0 {
        return Err("GetModuleHandleExW of the core failed".into());
    }
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 {
        return Err("GetModuleFileNameW of the core failed".into());
    }
    let dll = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    dll.parent().map(Path::to_path_buf).ok_or_else(|| "the core has no folder".into())
}
