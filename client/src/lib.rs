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

/// The ids of every running `eu4.exe`; an error when the list of processes
/// cannot be read, which tells nothing of whether the game runs.
pub fn game_processes() -> Result<Vec<u32>> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError},
        System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
    };
    let mut found = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return Err(format!("the list of processes cannot be read (error {})", GetLastError()));
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        // The list always has the system's own processes.
        if Process32FirstW(snapshot, &mut entry) == 0 {
            let error = GetLastError();
            CloseHandle(snapshot);
            return Err(format!("the list of processes cannot be read (error {error})"));
        }
        loop {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("eu4.exe") {
                found.push(entry.th32ProcessID);
            }
            if Process32NextW(snapshot, &mut entry) == 0 {
                break;
            }
        }
        CloseHandle(snapshot);
    }
    Ok(found)
}

/// Whether an `eu4.exe` runs; an error when that cannot be told.
pub fn game_running() -> Result<bool> {
    game_processes().map(|found| !found.is_empty())
}

/// Refuses while the game runs, which writes the player's files and holds
/// the framework's, and when it cannot be told whether it runs.
pub fn game_not_running() -> Result<()> {
    closed(game_running())
}

/// Goes ahead only when the game was seen closed.
fn closed(running: Result<bool>) -> Result<()> {
    match running {
        Ok(false) => Ok(()),
        Ok(true) => Err("eu4.exe is running; close it first".into()),
        Err(e) => Err(format!("cannot tell whether eu4.exe runs, so nothing is done: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What guards the player's files goes ahead only when the game was seen
    /// closed: a process list that cannot be read is refused, as a game that
    /// runs is.
    #[test]
    fn only_a_game_seen_closed_lets_the_work_go_ahead() {
        assert_eq!(closed(Ok(false)), Ok(()));
        assert!(closed(Ok(true)).unwrap_err().contains("close it first"));
        assert!(closed(Err("no snapshot".into())).unwrap_err().contains("cannot tell whether eu4.exe runs"));
    }
}
