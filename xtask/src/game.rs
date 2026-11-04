//! The installed game: its folder, and its process.
use crate::Result;
use std::path::{Path, PathBuf};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
        Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE},
    },
};

/// The game folder: `EU4_GAME_DIR` when it is set, or else the Steam library
/// that holds the game.
pub fn dir() -> Result<PathBuf> {
    match std::env::var_os("EU4_GAME_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => eu4_harmony_manifest::steam_game_dir().ok_or_else(|| "cannot find the game: pass --game-dir or set EU4_GAME_DIR".into()),
    }
}

/// The ids of every running `eu4.exe`.
pub fn processes() -> Vec<u32> {
    let mut found = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return found;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("eu4.exe") {
                found.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    found
}

pub fn running() -> bool {
    !processes().is_empty()
}

/// Refuses while the game runs.
pub fn not_running() -> Result<()> {
    if running() {
        return Err("eu4.exe is running; close it first".into());
    }
    Ok(())
}

/// Starts `eu4.exe` from the game folder, as the player's shortcut does.
pub fn start(game_dir: &Path) -> Result<()> {
    let exe = game_dir.join("eu4.exe");
    std::process::Command::new(&exe).current_dir(game_dir).spawn().map(drop).map_err(|e| format!("cannot start {}: {e}", exe.display()))
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
