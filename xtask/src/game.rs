//! The installed game: its folder, and its process.
use crate::Result;
use std::path::PathBuf;
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE},
};

/// The game folder: `EU4_GAME_DIR` when it is set, or else the Steam library
/// that holds the game.
pub fn dir() -> Result<PathBuf> {
    match std::env::var_os("EU4_GAME_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => eu4_harmony_manifest::steam_game_dir().ok_or_else(|| "cannot find the game: pass --game-dir or set EU4_GAME_DIR".into()),
    }
}

/// The ids of every running `eu4.exe`; none when the list of processes
/// cannot be read, which [`running`] counts as a game that runs.
pub fn processes() -> Vec<u32> {
    eu4_harmony_client::game_processes().unwrap_or_default()
}

/// Whether an `eu4.exe` runs, as it counts for what this guards: when it
/// cannot be told, it does.
pub fn running() -> bool {
    eu4_harmony_client::game_running().unwrap_or(true)
}

/// Refuses while the game runs, and when it cannot be told whether it runs.
pub fn not_running() -> Result<()> {
    eu4_harmony_client::game_not_running()
}

/// Waits for `eu4.exe` to end, a minute at most.
pub fn wait_closed() -> Result<()> {
    for _ in 0..120 {
        match eu4_harmony_client::game_running() {
            Ok(false) => return Ok(()),
            Ok(true) => {}
            Err(e) => return Err(format!("cannot tell whether eu4.exe has ended: {e}")),
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Err("eu4.exe did not end".into())
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

/// Ends every running `eu4.exe` at once.
pub fn kill() {
    processes().into_iter().for_each(kill_process);
}

pub fn kill_process(id: u32) {
    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE, 0, id);
        if !process.is_null() {
            TerminateProcess(process, 1);
            CloseHandle(process);
        }
    }
}
