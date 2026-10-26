//! Game and user directories, resolved the way the game itself does.
use crate::Result;
use std::path::{Path, PathBuf};
use windows_sys::Win32::{
    System::{Com::CoTaskMemFree, LibraryLoader::GetModuleFileNameW},
    UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT},
};

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

fn documents() -> Result<PathBuf> {
    let mut out: *mut u16 = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut out) };
    if hr < 0 || out.is_null() {
        return Err(format!("SHGetKnownFolderPath(Documents) failed: {hr:#x}"));
    }
    let len = (0..).take_while(|&i| unsafe { *out.add(i) } != 0).count();
    let path = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, len) });
    unsafe { CoTaskMemFree(out as _) };
    Ok(PathBuf::from(path))
}

/// The game's data directory: `userdir.txt` next to the exe when it names one,
/// otherwise `Documents\Paradox Interactive\Europa Universalis IV`.
pub fn user_dir(game_dir: &Path) -> Result<PathBuf> {
    let userdir_txt = game_dir.join("userdir.txt");
    if let Ok(text) = std::fs::read_to_string(&userdir_txt) {
        let text = text.trim_start_matches('\u{feff}').trim();
        if !text.is_empty() {
            let path = PathBuf::from(text);
            return Ok(if path.is_absolute() { path } else { game_dir.join(path) });
        }
    }
    Ok(documents()?.join("Paradox Interactive").join("Europa Universalis IV"))
}
