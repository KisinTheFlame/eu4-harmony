//! The game's process, from outside it: which `eu4.exe` run, starting one,
//! following one to its exit code, and ending them.
//!
//! When the list of processes cannot be read, whether the game runs cannot
//! be told, and one rule holds for all of this ([`counts_as_running`]): a
//! game that cannot be told not to run is taken to run. So what guards the
//! player's files refuses, what waits for the game to end goes on waiting,
//! and what would end a game says it cannot instead of ending nothing.
use crate::Result;
use std::{path::Path, time::Duration};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, WAIT_OBJECT_0},
    System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, TerminateProcess, WaitForSingleObject, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    },
};

/// The game's executable.
pub const EXE: &str = "eu4.exe";
/// How long [`end`] waits for the games to be gone.
const ENDING: Duration = Duration::from_secs(60);
/// How often a wait looks again.
const LOOK: Duration = Duration::from_millis(500);
/// The exit code a game these commands end gets: the game itself exits with
/// 0, and the framework after a fatal error with 3.
pub const ENDED_HERE: u32 = 1;

/// The ids of every running `eu4.exe`; an error when the list of processes
/// cannot be read, which tells nothing of whether the game runs.
pub fn processes() -> Result<Vec<u32>> {
    use windows_sys::Win32::{
        Foundation::GetLastError,
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
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(EXE) {
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

/// Whether the game runs, as `found` (what [`processes`] gave) tells it:
/// when it cannot be told, it does.
pub fn counts_as_running(found: &Result<Vec<u32>>) -> bool {
    found.as_ref().map_or(true, |games| !games.is_empty())
}

/// Whether an `eu4.exe` runs, by [`counts_as_running`].
pub fn running() -> bool {
    counts_as_running(&processes())
}

/// Refuses while the game runs, which writes the player's files and holds
/// the framework's, and when it cannot be told whether it runs.
pub fn not_running() -> Result<()> {
    closed(processes())
}

/// Goes ahead only when the game was seen closed.
fn closed(found: Result<Vec<u32>>) -> Result<()> {
    match found {
        Ok(games) if games.is_empty() => Ok(()),
        Ok(_) => Err("eu4.exe is running; close it first".into()),
        Err(e) => Err(format!("cannot tell whether eu4.exe runs, so nothing is done: {e}")),
    }
}

/// Ends the `eu4.exe` `id` and waits for it to be gone, a minute at most;
/// an error when that cannot be told. The test games are ended one by one:
/// a game that is no test game's is the player's, which no command ends.
pub fn end_game(id: u32) -> Result<()> {
    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE, 0, id);
        if !process.is_null() {
            TerminateProcess(process, ENDED_HERE);
            CloseHandle(process);
        }
    }
    let mut waited = Duration::ZERO;
    loop {
        match processes() {
            Ok(games) if !games.contains(&id) => return Ok(()),
            Ok(_) if waited >= ENDING => return Err(format!("eu4.exe {id} did not end")),
            Ok(_) => {}
            Err(e) => return Err(format!("cannot tell whether eu4.exe {id} has ended: {e}")),
        }
        std::thread::sleep(LOOK);
        waited += LOOK;
    }
}

/// Waits for every `eu4.exe` to end, a minute at most.
pub fn wait_closed() -> Result<()> {
    let mut waited = Duration::ZERO;
    loop {
        match processes() {
            Ok(games) if games.is_empty() => return Ok(()),
            Ok(_) if waited >= ENDING => return Err("eu4.exe did not end".into()),
            Ok(_) => {}
            Err(e) => return Err(format!("cannot tell whether eu4.exe has ended: {e}")),
        }
        std::thread::sleep(LOOK);
        waited += LOOK;
    }
}

/// One `eu4.exe`, held open from when it is started or found until it is
/// dropped, so that its exit code stays to be read once it ends: Windows
/// keeps a process's exit code only while a handle to it is open.
#[derive(Debug)]
pub struct Game {
    /// The process's handle, kept as a number so that a game can go to
    /// another thread.
    handle: usize,
    id: u32,
    started: u64,
}

impl Game {
    /// Starts `exe` from `dir` with `switches`, with none of this program's
    /// handles: one that kept this program's output, a pipe, would keep
    /// whoever reads it waiting until the game ends, and every inheritable
    /// handle is passed on unless none is.
    pub fn spawn(exe: &Path, switches: &[String], dir: &Path) -> Result<Self> {
        use windows_sys::Win32::System::Threading::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};
        let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let mut line = wide(&std::iter::once(format!("\"{}\"", exe.display())).chain(switches.iter().cloned()).collect::<Vec<_>>().join(" "));
        let (exe_w, dir_w) = (wide(&exe.display().to_string()), wide(&dir.display().to_string()));
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let made = unsafe {
            CreateProcessW(exe_w.as_ptr(), line.as_mut_ptr(), std::ptr::null(), std::ptr::null(), 0, 0, std::ptr::null(), dir_w.as_ptr(), &startup, &mut info)
        };
        if made == 0 {
            return Err(format!("cannot start {}: {}", exe.display(), std::io::Error::last_os_error()));
        }
        unsafe { CloseHandle(info.hThread) };
        let started = start_time(info.hProcess as usize).unwrap_or(0);
        Ok(Self { handle: info.hProcess as usize, id: info.dwProcessId, started })
    }

    /// The process `id`, when it runs and can be opened: for its times and
    /// exit code, and to wait on, which [`Game::exit_code`] does.
    pub fn open(id: u32) -> Option<Self> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, id) } as usize;
        if handle == 0 {
            return None;
        }
        match start_time(handle) {
            Some(started) => Some(Self { handle, id, started }),
            None => {
                unsafe { CloseHandle(handle as _) };
                None
            }
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    /// When it started, as Windows counts it: with its id, which Windows
    /// gives again once it has ended, it names this process alone.
    pub fn started(&self) -> u64 {
        self.started
    }

    /// Waits for it to end; its exit code.
    pub fn wait(&self) -> Option<u32> {
        unsafe { WaitForSingleObject(self.handle as _, INFINITE) };
        self.exit_code()
    }

    /// Its exit code, once it has ended.
    pub fn exit_code(&self) -> Option<u32> {
        let mut code = 0;
        (unsafe { WaitForSingleObject(self.handle as _, 0) } == WAIT_OBJECT_0 && unsafe { GetExitCodeProcess(self.handle as _, &mut code) } != 0)
            .then_some(code)
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle as _) };
    }
}

/// When the process `handle` started, as Windows counts it.
fn start_time(handle: usize) -> Option<u64> {
    let mut times: [FILETIME; 4] = unsafe { std::mem::zeroed() };
    let [created, exited, kernel, user] = &mut times;
    (unsafe { GetProcessTimes(handle as _, created, exited, kernel, user) } != 0)
        .then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What guards the player's files goes ahead only when the game was seen
    /// closed: a process list that cannot be read is refused, as a game that
    /// runs is.
    #[test]
    fn only_a_game_seen_closed_lets_the_work_go_ahead() {
        assert_eq!(closed(Ok(vec![])), Ok(()));
        assert!(closed(Ok(vec![7])).unwrap_err().contains("close it first"));
        assert!(closed(Err("no snapshot".into())).unwrap_err().contains("cannot tell whether eu4.exe runs"));
    }

    #[test]
    fn a_game_that_cannot_be_told_not_to_run_runs() {
        assert!(!counts_as_running(&Ok(vec![])));
        assert!(counts_as_running(&Ok(vec![7])));
        assert!(counts_as_running(&Err("no snapshot".into())));
    }

    /// A process found by its id, as `watch` finds the games that run, is
    /// held so that its exit code is read once it ends.
    #[test]
    fn a_process_found_by_its_id_is_followed_to_its_exit_code() {
        let cmd = Path::new(r"C:\Windows\System32\cmd.exe");
        let line = ["/c", "ping", "-n", "2", "127.0.0.1", ">nul", "&", "exit", "9"].map(String::from);
        let started = Game::spawn(cmd, &line, Path::new(r"C:\Windows")).unwrap();
        let found = Game::open(started.id()).expect("it runs");
        drop(started);
        assert_ne!(found.started(), 0);
        let mut code = None;
        for _ in 0..200 {
            code = found.exit_code();
            if code.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(code, Some(9));
    }

    /// A process this program starts is held from its start, so its exit
    /// code is there however soon it ends.
    #[test]
    fn a_started_process_keeps_its_exit_code_however_soon_it_ends() {
        let cmd = Path::new(r"C:\Windows\System32\cmd.exe");
        let game = Game::spawn(cmd, &["/c".into(), "exit".into(), "7".into()], Path::new(r"C:\Windows")).unwrap();
        assert_ne!(game.started(), 0);
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(game.exit_code(), Some(7));
        assert_eq!(game.wait(), Some(7));
    }
}
