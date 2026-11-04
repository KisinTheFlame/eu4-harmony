//! `cargo xtask perf-run`: measures one run of the game, for comparing
//! setups (docs/performance.md).
//!
//! It starts eu4.exe directly and records, into `<out>\<name>.json`:
//! - the seconds from launch to the main menu, which it recognizes on screen;
//! - memory 20 s later (working set and private bytes);
//! - unless `--menu-only`: a new game as Ming from the 1444 East Asia
//!   bookmark, the seconds it takes to load, CPU while paused for 30 s and
//!   while running at speed 5 for 120 s (in cores: CPU seconds a second),
//!   memory, and crops of the date before and after the run
//!   (`<name>-date0.png`, `<name>-date1.png`).
//!
//! With `--menu-only` it records CPU at the idle main menu for 30 s instead.
//! The game is then closed. Back up the player's files first: the run
//! autosaves.
//!
//! Clicks are physical pixels of a 3840x2160 screen at the game's default
//! interface scale. The run recognizes two screens by comparing a region with
//! a reference taken on this machine, once: the main menu, with
//! `--reference` while it shows, and the start-of-game introduction, with
//! `--intro-reference` while it is open. It measures vanilla too, where no
//! framework runs and remote control cannot click or press keys, so it sends
//! the game its clicks and keys itself, as a mouse and keyboard would: keep
//! the game window in front, and the machine otherwise idle, while it runs.
use crate::{
    game,
    screen::{self, Area},
    Args, Result,
};
use serde_json::{json, Value};
use std::{path::Path, time::Instant};

/// The "IV" of the logo, top left of the main menu: a texture, the same in
/// every setup.
const MENU_AREA: Area = (690, 60, 240, 200);
/// The bottom left ornament of the start-of-game introduction window.
const INTRO_AREA: Area = (1195, 1500, 100, 80);
/// The date at the top right.
const DATE_AREA: Area = (3380, 0, 460, 90);

pub fn run(args: &Args) -> Result<()> {
    screen::physical_pixels();
    let out = Path::new(args.value("out").ok_or("--out names the folder for results and references")?);
    std::fs::create_dir_all(out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;
    let menu_reference = out.join("menu-reference.bin");
    let intro_reference = out.join("intro-corner.png");
    if args.flag("reference") {
        std::fs::write(&menu_reference, screen::sample_screen(MENU_AREA)?).map_err(|e| format!("cannot write {}: {e}", menu_reference.display()))?;
        println!("saved {}", menu_reference.display());
        return Ok(());
    }
    if args.flag("intro-reference") {
        screen::save(INTRO_AREA, &intro_reference)?;
        println!("saved {}", intro_reference.display());
        return Ok(());
    }
    let name = args.value("name").ok_or("--name names the run")?;
    let menu = std::fs::read(&menu_reference).map_err(|e| format!("cannot read {} (take it with --reference): {e}", menu_reference.display()))?;
    let game_dir = args.game_dir()?;

    // A game closed a moment ago may take a few seconds to exit.
    for _ in 0..30 {
        if !game::running() {
            break;
        }
        screen::pause(500);
    }
    if game::running() {
        return Err("eu4 is running".into());
    }
    let mut result = json!({ "name": name, "started": now() });
    let launched = Instant::now();
    game::start(&game_dir)?;
    result["menu_seconds"] = rounded(wait_for(&menu, MENU_AREA, 300.0, launched)?, 1);
    let process = Process::find()?;
    screen::pause(20_000);
    result["menu"] = process.sample()?.json();
    if args.flag("menu-only") {
        let a = process.sample()?;
        screen::pause(30_000);
        let b = process.sample()?;
        result["menu_idle_cpu_share"] = rounded((b.cpu - a.cpu) / 30.0, 3);
        game::kill_process(process.id);
        return save(out, name, &result);
    }

    let intro = screen::sample_file(&intro_reference).map_err(|e| format!("{e} (take it with --intro-reference)"))?;
    // Single player, the 1444 East Asia bookmark, Ming, Start, normal mode.
    screen::click(1815, 2025);
    screen::pause(15_000);
    screen::click(120, 300);
    screen::pause(5_000);
    screen::click(2550, 1200);
    screen::pause(3_000);
    screen::click(3645, 2109);
    screen::pause(3_000);
    screen::click(2064, 1347);
    result["start_seconds"] = rounded(wait_for(&intro, INTRO_AREA, 120.0, Instant::now())?, 1);
    screen::pause(2_000);
    screen::click(1920, 1587);
    screen::pause(10_000);
    let a = process.sample()?;
    screen::pause(30_000);
    let b = process.sample()?;
    result["paused"] = json!({ "cpu_share": rounded((b.cpu - a.cpu) / 30.0, 3), "ws": b.ws, "priv": b.private });
    screen::save(DATE_AREA, &out.join(format!("{name}-date0.png")))?;
    // The date panel's + button, four times: speed 5.
    for _ in 0..4 {
        screen::click(3725, 40);
    }
    let a = process.sample()?;
    screen::space();
    let running = Instant::now();
    screen::pause(120_000);
    screen::space();
    let ran = running.elapsed().as_secs_f64();
    let b = process.sample()?;
    result["running"] = json!({ "seconds": rounded(ran, 1), "cpu_share": rounded((b.cpu - a.cpu) / ran, 3), "ws": b.ws, "priv": b.private, "peak": b.peak });
    screen::pause(1_000);
    screen::save(DATE_AREA, &out.join(format!("{name}-date1.png")))?;
    game::kill_process(process.id);
    save(out, name, &result)
}

/// The seconds since `since` until the region looks like `reference`,
/// looked at every quarter of a second, at most `limit` seconds.
fn wait_for(reference: &[u8], area: Area, limit: f64, since: Instant) -> Result<f64> {
    let clock = Instant::now();
    while clock.elapsed().as_secs_f64() < limit {
        screen::pause(250);
        if screen::difference(reference, &screen::sample_screen(area)?) < 12.0 {
            return Ok(since.elapsed().as_secs_f64());
        }
    }
    Err(format!("not seen within {limit} s"))
}

fn save(out: &Path, name: &str, result: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(result).expect("JSON");
    let path = out.join(format!("{name}.json"));
    std::fs::write(&path, &text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    println!("{text}");
    Ok(())
}

fn rounded(value: f64, decimals: i32) -> Value {
    let scale = 10f64.powi(decimals);
    json!((value * scale).round() / scale)
}

/// The game's process: the `eu4.exe` with a visible window.
struct Process {
    id: u32,
    handle: windows_sys::Win32::Foundation::HANDLE,
}

/// What the process has used: CPU seconds, and its working set, private
/// bytes and peak working set in MB.
struct Sample {
    cpu: f64,
    ws: u64,
    private: u64,
    peak: u64,
}

impl Sample {
    fn json(&self) -> Value {
        json!({ "cpu": self.cpu, "ws": self.ws, "priv": self.private, "peak": self.peak })
    }
}

impl Process {
    fn find() -> Result<Self> {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
        let id = game::processes().into_iter().find(|&id| has_window(id)).ok_or("the game has no window")?;
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, id) };
        if handle.is_null() {
            return Err(format!("cannot open the game's process {id}"));
        }
        Ok(Self { id, handle })
    }

    fn sample(&self) -> Result<Sample> {
        use windows_sys::Win32::{
            Foundation::FILETIME,
            System::{
                ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
                Threading::GetProcessTimes,
            },
        };
        let mb = |bytes: usize| ((bytes as f64) / 1048576.0).round() as u64;
        unsafe {
            let mut times: [FILETIME; 4] = std::mem::zeroed();
            let [created, exited, kernel, user] = &mut times;
            if GetProcessTimes(self.handle, created, exited, kernel, user) == 0 {
                return Err("cannot read the game's times; it may be gone".into());
            }
            let ticks = |t: &FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 1e7;
            let mut memory: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            memory.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            if GetProcessMemoryInfo(self.handle, (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(), memory.cb) == 0 {
                return Err("cannot read the game's memory; it may be gone".into());
            }
            Ok(Sample {
                cpu: ticks(&times[2]) + ticks(&times[3]),
                ws: mb(memory.WorkingSetSize),
                private: mb(memory.PrivateUsage),
                peak: mb(memory.PeakWorkingSetSize),
            })
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

/// Whether the process shows a top-level window.
fn has_window(id: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible},
    };
    struct Search {
        id: u32,
        found: bool,
    }
    unsafe extern "system" fn each(window: HWND, search: LPARAM) -> i32 {
        let search = &mut *(search as *mut Search);
        let mut owner = 0;
        GetWindowThreadProcessId(window, &mut owner);
        if owner == search.id && IsWindowVisible(window) != 0 {
            search.found = true;
            return 0;
        }
        1
    }
    let mut search = Search { id, found: false };
    unsafe { EnumWindows(Some(each), &mut search as *mut Search as LPARAM) };
    search.found
}

/// The local date and time, as `yyyy-MM-dd HH:mm:ss`.
fn now() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}
