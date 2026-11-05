//! Reaching the running game from outside its process: remote control's
//! named pipe, asked in its JSON form or with the request and answer types of
//! `eu4-harmony-remote`; starting the game, hidden or shown, up to its main
//! menu; and the backup and restore of the player's files a run may change.
//! The command line tool and the repository's development commands
//! (`cargo xtask`) share it.
pub mod backup;
pub mod pipe;
pub mod start;

pub type Result<T> = std::result::Result<T, String>;

/// Whether an `eu4.exe` runs.
pub fn game_running() -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        let mut found = false;
        while more && !found {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            found = String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("eu4.exe");
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        found
    }
}

/// Refuses while the game runs, which writes the player's files and holds
/// the framework's.
pub fn game_not_running() -> Result<()> {
    if game_running() {
        return Err("eu4.exe is running; close it first".into());
    }
    Ok(())
}
