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

/// The interactive window station and its default desktop.
const INTERACTIVE_STATION: &str = "WinSta0";
const DEFAULT_DESKTOP: &str = "Default";
const INTERACTIVE_DEFAULT_DESKTOP: &str = r"WinSta0\Default";
/// `WSF_VISIBLE` in `USEROBJECTFLAGS.dwFlags` for a visible window station.
const WSF_VISIBLE: u32 = 0x0001;
/// Desktop access rights needed for a GUI process on `WinSta0\Default`:
/// `DESKTOP_READOBJECTS | DESKTOP_CREATEWINDOW | DESKTOP_WRITEOBJECTS`.
const DESKTOP_LAUNCH_ACCESS: u32 = 0x0001 | 0x0002 | 0x0080;

impl Game {
    /// Starts `exe` from `dir` with `switches`, with none of this program's
    /// handles: one that kept this program's output, a pipe, would keep
    /// whoever reads it waiting until the game ends, and every inheritable
    /// handle is passed on unless none is.
    ///
    /// When the caller runs on a non-default desktop inside the visible
    /// interactive window station (`WinSta0`, such as an agent runner's hidden
    /// desktop), targets `WinSta0\Default` if that desktop can be opened so
    /// visible starts appear on the screen and hidden starts (`-harmony_hidden`)
    /// avoid inactive-desktop DWM/Direct3D 9 frame throttling. Falls back to
    /// inheriting the caller's desktop otherwise.
    pub fn spawn(exe: &Path, switches: &[String], dir: &Path) -> Result<Self> {
        Self::spawn_with_desktop(exe, switches, dir, interactive_default_desktop())
    }

    fn spawn_with_desktop(exe: &Path, switches: &[String], dir: &Path, desktop: Option<&str>) -> Result<Self> {
        use windows_sys::Win32::System::Threading::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};
        let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let mut line = wide(&std::iter::once(format!("\"{}\"", exe.display())).chain(switches.iter().cloned()).collect::<Vec<_>>().join(" "));
        let (exe_w, dir_w) = (wide(&exe.display().to_string()), wide(&dir.display().to_string()));
        let mut desktop_w = desktop.map(wide);
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        if let Some(name) = desktop_w.as_mut() {
            startup.lpDesktop = name.as_mut_ptr();
        }
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

/// Whether Steam runs with a user logged in, as it must for the game to
/// start: the process and the user Steam records in the registry, its process
/// still running. The game is never started without it, and Steam never
/// started by these commands: it is the player's, and may want them to log in
/// or wait for an update.
pub fn steam_ready() -> Result<()> {
    use windows_sys::Win32::System::{
        Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
        Threading::PROCESS_QUERY_LIMITED_INFORMATION,
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

/// When the process `handle` started, as Windows counts it.
fn start_time(handle: usize) -> Option<u64> {
    let mut times: [FILETIME; 4] = unsafe { std::mem::zeroed() };
    let [created, exited, kernel, user] = &mut times;
    (unsafe { GetProcessTimes(handle as _, created, exited, kernel, user) } != 0)
        .then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Chooses the `STARTUPINFOW.lpDesktop` value for a spawned game process.
///
/// Returns `Some(r"WinSta0\Default")` only when the caller belongs to the
/// visible interactive window station (`WinSta0`), its thread is attached to a
/// non-default desktop, and `Default` can be opened for window creation. In
/// all other cases (already on `WinSta0\Default`, or in a non-interactive
/// session/service whose own window station also contains a `Default` desktop),
/// returns `None` so `CreateProcessW` inherits the caller's desktop.
fn default_desktop_for(station: Option<(&str, bool)>, desktop: Option<&str>, can_open_default: impl FnOnce() -> bool) -> Option<&'static str> {
    let (station_name, visible) = station?;
    if !visible || !station_name.eq_ignore_ascii_case(INTERACTIVE_STATION) {
        return None;
    }
    let desktop_name = desktop?;
    if desktop_name.eq_ignore_ascii_case(DEFAULT_DESKTOP) {
        return None;
    }
    can_open_default().then_some(INTERACTIVE_DEFAULT_DESKTOP)
}

/// Returns `Some(r"WinSta0\Default")` when the current process runs on another
/// desktop inside the visible `WinSta0` window station and can open `Default`.
fn interactive_default_desktop() -> Option<&'static str> {
    let station = current_window_station();
    let desktop = current_desktop();
    default_desktop_for(station.as_ref().map(|(name, visible)| (name.as_str(), *visible)), desktop.as_deref(), can_open_default_desktop)
}

/// The calling process's window station name and whether `WSF_VISIBLE` is set.
fn current_window_station() -> Option<(String, bool)> {
    use windows_sys::Win32::System::StationsAndDesktops::{GetProcessWindowStation, GetUserObjectInformationW, UOI_FLAGS, USEROBJECTFLAGS};
    let station = unsafe { GetProcessWindowStation() };
    if station.is_null() {
        return None;
    }
    let name = user_object_name(station)?;
    let mut flags: USEROBJECTFLAGS = unsafe { std::mem::zeroed() };
    let mut needed = 0u32;
    let ok = unsafe {
        GetUserObjectInformationW(station, UOI_FLAGS, (&mut flags as *mut USEROBJECTFLAGS).cast(), std::mem::size_of::<USEROBJECTFLAGS>() as u32, &mut needed)
    };
    if ok == 0 {
        return None;
    }
    Some((name, (flags.dwFlags & WSF_VISIBLE) != 0))
}

/// The calling thread's current desktop name.
fn current_desktop() -> Option<String> {
    use windows_sys::Win32::System::{StationsAndDesktops::GetThreadDesktop, Threading::GetCurrentThreadId};
    let desktop = unsafe { GetThreadDesktop(GetCurrentThreadId()) };
    if desktop.is_null() {
        return None;
    }
    user_object_name(desktop)
}

/// Reads the UTF-16 `UOI_NAME` of a window station or desktop handle.
fn user_object_name(handle: *mut std::ffi::c_void) -> Option<String> {
    use windows_sys::Win32::System::StationsAndDesktops::{GetUserObjectInformationW, UOI_NAME};
    let mut needed = 0u32;
    unsafe { GetUserObjectInformationW(handle, UOI_NAME, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 || !needed.is_multiple_of(2) {
        return None;
    }
    let mut buf = vec![0u16; (needed as usize) / 2];
    let ok = unsafe { GetUserObjectInformationW(handle, UOI_NAME, buf.as_mut_ptr().cast(), needed, &mut needed) };
    if ok == 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Whether `"Default"` in the calling process's current window station can be
/// opened with window-creation access.
fn can_open_default_desktop() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenDesktopW};
    let name: Vec<u16> = DEFAULT_DESKTOP.encode_utf16().chain([0]).collect();
    let desktop = unsafe { OpenDesktopW(name.as_ptr(), 0, 0, DESKTOP_LAUNCH_ACCESS) };
    if desktop.is_null() {
        return false;
    }
    unsafe { CloseDesktop(desktop) };
    true
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

    #[test]
    fn non_default_desktop_in_visible_winsta0_targets_default_when_accessible() {
        assert_eq!(default_desktop_for(Some(("WinSta0", true)), Some("exebox-123"), || true), Some(INTERACTIVE_DEFAULT_DESKTOP));
        assert_eq!(default_desktop_for(Some(("winsta0", true)), Some("CustomDesktop"), || true), Some(INTERACTIVE_DEFAULT_DESKTOP));
        assert_eq!(default_desktop_for(Some(("WinSta0", true)), Some("exebox-123"), || false), None);
    }

    #[test]
    fn already_default_or_non_interactive_station_inherits_without_probing() {
        let must_not_probe = || -> bool { panic!("OpenDesktopW probe must not run") };
        assert_eq!(default_desktop_for(Some(("WinSta0", true)), Some("Default"), must_not_probe), None);
        assert_eq!(default_desktop_for(Some(("WinSta0", true)), Some("default"), must_not_probe), None);
        assert_eq!(default_desktop_for(Some(("WinSta0", false)), Some("exebox-123"), must_not_probe), None);
        assert_eq!(default_desktop_for(Some(("Service-0x0-3e7$", false)), Some("Hidden"), must_not_probe), None);
        assert_eq!(default_desktop_for(Some(("Service-0x0-3e7$", true)), Some("Hidden"), must_not_probe), None);
        assert_eq!(default_desktop_for(None, Some("exebox-123"), must_not_probe), None);
        assert_eq!(default_desktop_for(Some(("WinSta0", true)), None, must_not_probe), None);
    }

    #[test]
    fn desktop_probe_queries_the_current_session_and_supports_explicit_lpdesktop() {
        let station = current_window_station();
        let desktop = current_desktop();
        assert!(station.as_ref().is_some_and(|(name, _)| !name.is_empty()));
        assert!(desktop.as_ref().is_some_and(|name| !name.is_empty()));
        let chosen = interactive_default_desktop();
        assert!(chosen.is_none() || chosen == Some(INTERACTIVE_DEFAULT_DESKTOP));

        // Exercise the non-null `STARTUPINFOW.lpDesktop` path against the
        // caller's own desktop so the test works in both interactive and CI
        // sessions without flashing a console window on another desktop.
        let (station_name, _) = station.unwrap();
        let desktop_name = desktop.unwrap();
        let current = format!(r"{station_name}\{desktop_name}");
        let root = system_root();
        let cmd = root.join(r"System32\cmd.exe");
        let proc = Game::spawn_with_desktop(&cmd, &["/c".into(), "exit".into(), "5".into()], &root, Some(&current)).unwrap();
        assert_eq!(proc.wait(), Some(5));
    }

    fn system_root() -> std::path::PathBuf {
        std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
    }

    /// A process found by its id, as `watch` finds the games that run, is
    /// held so that its exit code is read once it ends.
    #[test]
    fn a_process_found_by_its_id_is_followed_to_its_exit_code() {
        let root = system_root();
        let cmd = root.join(r"System32\cmd.exe");
        let line = ["/c", "ping", "-n", "2", "127.0.0.1", ">nul", "&", "exit", "9"].map(String::from);
        let started = Game::spawn_with_desktop(&cmd, &line, &root, None).unwrap();
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
        let root = system_root();
        let cmd = root.join(r"System32\cmd.exe");
        let game = Game::spawn_with_desktop(&cmd, &["/c".into(), "exit".into(), "7".into()], &root, None).unwrap();
        assert_ne!(game.started(), 0);
        // However long a busy computer takes to run it, its code is kept.
        let mut code = None;
        for _ in 0..200 {
            code = game.exit_code();
            if code.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(code, Some(7));
        assert_eq!(game.wait(), Some(7));
    }
}
