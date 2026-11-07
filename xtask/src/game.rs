//! The installed game: its folder, and whether Steam is ready to start it.
//! The game's process is the client's (`eu4_harmony_client::game`).
use crate::Result;
use std::path::PathBuf;
use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::OpenProcess};

/// The game folder, as every program of EU4 Harmony finds it
/// ([`eu4_harmony_folders::game_dir`]).
pub fn dir() -> Result<PathBuf> {
    eu4_harmony_folders::game_dir(None)
}

/// Whether Steam runs with a user logged in, as it must for the game to
/// start: the process and the user Steam records in the registry, its
/// process still running. The game is never started without it, and Steam
/// never started by these commands: it is the player's, and may want them
/// to log in or wait for an update.
pub fn steam_ready() -> Result<()> {
    use windows_sys::Win32::System::{
        Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
        Threading::{GetExitCodeProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    let read = |name: &str| -> u32 {
        let key: Vec<u16> = "Software\\Valve\\Steam\\ActiveProcess\0".encode_utf16().collect();
        let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let (mut value, mut size) = (0u32, 4u32);
        let read = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut value as *mut u32).cast(), &mut size)
        };
        if read == 0 {
            value
        } else {
            0
        }
    };
    let process = read("pid");
    let alive = process != 0
        && unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process);
            let mut code = 0;
            let alive = !handle.is_null() && GetExitCodeProcess(handle, &mut code) != 0 && code == 259;
            if !handle.is_null() {
                CloseHandle(handle);
            }
            alive
        };
    if !alive {
        return Err("Steam is not running: start it, logged in, and run this again".into());
    }
    if read("ActiveUser") == 0 {
        return Err("Steam runs with no user logged in: log in, and run this again".into());
    }
    Ok(())
}
