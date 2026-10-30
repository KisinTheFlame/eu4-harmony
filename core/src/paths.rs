//! Game and user directories, resolved the way the game itself does.
use crate::Result;
use std::path::{Path, PathBuf};
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

/// The game's data directory, as the command line tool finds it too
/// ([`eu4_harmony_manifest::user_dir`]).
pub fn user_dir(game_dir: &Path) -> Result<PathBuf> {
    eu4_harmony_manifest::user_dir(Some(game_dir))
}
