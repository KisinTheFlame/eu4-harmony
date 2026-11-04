//! `cargo xtask perf-run`: measures one run of the game, for comparing
//! setups of the framework (docs/performance.md).
//!
//! Within the claim, it starts the game hidden (`cargo xtask game start`)
//! and records, into `<out>\<name>.json`:
//! - the seconds from the start to the main menu;
//! - memory 20 s later (working set and private bytes);
//! - unless `--menu-only`: a new game as Ming, the seconds `menu.start`
//!   takes, CPU while paused for 30 s and while running at speed 5 for
//!   120 s (in cores: CPU seconds a second), memory, and the dates before
//!   and after the run.
//!
//! With `--menu-only` it records CPU at the idle main menu for 30 s instead.
//! The game is then closed. It goes through remote control only, so the
//! installed build must be the framework's.
use crate::{claim, game, Args, Result};
use serde_json::{json, Value};
use std::{path::Path, time::Duration, time::Instant};

pub fn run(args: &Args) -> Result<()> {
    claim::needs_claim(args)?;
    let out = Path::new(args.value("out").ok_or("--out names the folder for the results")?);
    std::fs::create_dir_all(out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;
    let name = args.value("name").ok_or("--name names the run")?;
    let remote = game::Client::installed(&args.game_dir()?)?;
    game::not_running()?;

    let mut result = json!({ "name": name, "started": now() });
    result["menu_seconds"] = rounded(claim::start_game(args, false)?, 1);
    let process = Process::find()?;
    pause(20);
    result["menu"] = process.sample()?.json();
    if args.flag("menu-only") {
        let a = process.sample()?;
        pause(30);
        let b = process.sample()?;
        result["menu_idle_cpu_share"] = rounded((b.cpu - a.cpu) / 30.0, 3);
        game::kill_process(process.id);
        return save(out, name, &result);
    }

    let starting = Instant::now();
    remote.ask(&["menu.start", "MNG"])?;
    result["start_seconds"] = rounded(starting.elapsed().as_secs_f64(), 1);
    pause(10);
    let a = process.sample()?;
    pause(30);
    let b = process.sample()?;
    result["paused"] = json!({ "cpu_share": rounded((b.cpu - a.cpu) / 30.0, 3), "ws": b.ws, "priv": b.private });
    result["date0"] = remote.ask(&["time.get"])?["date"].clone();
    remote.ask(&["time.set_speed", "5"])?;
    let a = process.sample()?;
    remote.ask(&["time.resume"])?;
    let running = Instant::now();
    pause(120);
    remote.ask(&["time.pause"])?;
    let ran = running.elapsed().as_secs_f64();
    let b = process.sample()?;
    result["running"] = json!({ "seconds": rounded(ran, 1), "cpu_share": rounded((b.cpu - a.cpu) / ran, 3), "ws": b.ws, "priv": b.private, "peak": b.peak });
    result["date1"] = remote.ask(&["time.get"])?["date"].clone();
    game::kill_process(process.id);
    save(out, name, &result)
}

fn pause(seconds: u64) {
    std::thread::sleep(Duration::from_secs(seconds));
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

/// The game's process.
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
        let id = *game::processes().first().ok_or("the game is not running")?;
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
