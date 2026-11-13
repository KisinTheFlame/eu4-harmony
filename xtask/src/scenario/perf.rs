//! `perf`: what one run of the game costs, with bounded performance collection, for
//! comparing setups of the framework (docs/performance.md). It starts the
//! game afresh and records:
//! - the seconds from the start to the main menu;
//! - memory 20 s later (working set and private bytes);
//! - unless `menu_only`: a new game as the country, the seconds `menu.start`
//!   takes, CPU while paused for 30 s and while running at speed 5 for
//!   120 s (in cores: CPU seconds a second), memory, the dates before and
//!   after, and the game days a minute at speed 5.
//!
//! With `menu_only` it records CPU at the idle main menu for 30 s instead.
//!
//! With `signals`, a remote client reads signals of those kinds before the
//! game runs, so that they are worked out from then on, and the signals heard
//! while it ran are counted by kind. With `reads`, after the run it times
//! `country.get` and `country.budget` of the countries that own provinces and
//! `province.get` of every owned land province, one at a time, from the
//! client.
//! Either way it returns to the main menu at the end, where the framework's
//! log tells what working out the signals cost the session
//! (`signal costs:`).
use super::{text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    camera::{Camera, CameraRequest},
    countries::{Budget, Country, CountryRequest},
    menu::MenuRequest,
    performance::{PerformanceRequest, Report},
    provinces::{Province, ProvinceKind, ProvinceRequest, ProvinceSnapshot},
    session::Session,
    signals::{Signals, SignalsRequest},
    time::{Time, TimeRequest},
    CountryTag, Words,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub struct Perf;

/// The speed-ups `game_paths` leaves to the game, by name, group or `all`.
fn left_to_the_game(parameters: &Parameters) -> Vec<String> {
    parameters.text("game_paths").split(',').map(str::trim).filter(|n| !n.is_empty()).map(String::from).collect()
}

impl Scenario for Perf {
    fn name(&self) -> &'static str {
        "perf"
    }

    fn about(&self) -> &'static str {
        "bounded performance windows: completed days, frame submission, interface updates, startup, CPU and memory"
    }

    fn build(&self, parameters: &Parameters) -> Build {
        Build::Performance { switches: !left_to_the_game(parameters).is_empty() }
    }

    fn options(&self, parameters: &Parameters) -> Option<String> {
        let names = left_to_the_game(parameters);
        (!names.is_empty()).then(|| {
            let quoted: Vec<String> = names.iter().map(|n| format!("{n:?}")).collect();
            format!("[test-switches]\ngame-paths = [{}]\n", quoted.join(", "))
        })
    }

    fn minutes(&self) -> u32 {
        // Each window may be up to ten minutes, plus startup and warmup.
        25
    }

    fn alone(&self) -> bool {
        true
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter { name: "country", default: "MNG", about: "the country of the new game" },
            Parameter { name: "menu_only", default: "false", about: "CPU at the idle main menu instead of in-game performance windows" },
            Parameter {
                name: "collect",
                default: "true",
                about: "collect in-process metrics; false uses the same build with collection off for overhead comparisons",
            },
            Parameter { name: "paused_seconds", default: "30", about: "seconds in the paused window (1 to 600)" },
            Parameter { name: "running_seconds", default: "120", about: "seconds in the speed-5 window (1 to 600)" },
            Parameter { name: "signals", default: "", about: "the kinds of signals a remote client listens to while the game runs, by commas" },
            Parameter { name: "reads", default: "false", about: "after the run, time country.get, country.budget and province.get" },
            Parameter {
                name: "game_paths",
                default: "",
                about: "the speed-ups left to the game, by name, group or all, by commas: plays a build with the test switches",
            },
        ]
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let country = parameters.text("country");
        let tag = CountryTag::parse(&country).ok_or_else(|| Stop::Failed(format!("no country tag {country:?}")))?;
        let collect = parameters.flag("collect")?;
        let paused_seconds = window_seconds(parameters, "paused_seconds")?;
        let running_seconds = window_seconds(parameters, "running_seconds")?;
        record["measurement"] = json!({
            "build": self.build(parameters).name(), "collection": collect,
            "visibility": "hidden", "country": country, "game_paths": left_to_the_game(parameters),
            "paused_seconds": paused_seconds, "running_seconds": running_seconds,
            "unmeasured": ["physical display latency", "action response", "pure interface work duration", "GPU time"]
        });
        record["menu_seconds"] = rounded(game.restart()?, 1);
        let process = Process::find(game)?;
        pause(20);
        record["menu"] = process.sample()?.json();
        if parameters.flag("menu_only")? {
            let a = process.sample()?;
            let began = Instant::now();
            pause(paused_seconds as u64);
            let b = process.sample()?;
            record["menu_idle_cpu_share"] = rounded((b.cpu - a.cpu) / began.elapsed().as_secs_f64(), 3);
            return Ok(());
        }

        let starting = Instant::now();
        let _: Session = game.ask(&MenuRequest::Start { country: Some(tag) })?;
        record["start_seconds"] = rounded(starting.elapsed().as_secs_f64(), 1);
        let () = game.ask(&TimeRequest::Pause {})?;
        pause(10);
        record["camera"] = json!(game.ask::<Camera>(&CameraRequest::Get {})?);
        record["paused"] = measure(game, &process, paused_seconds, collect)?;
        let kinds: Vec<String> = parameters.text("signals").split(',').map(str::trim).filter(|k| !k.is_empty()).map(String::from).collect();
        let since = if kinds.is_empty() {
            None
        } else {
            let read: Signals = game.ask(&listen(&kinds, None))?;
            record["signals"] = json!(kinds);
            Some(read.last)
        };
        let before: Time = game.ask(&TimeRequest::Get {})?;
        let () = game.ask(&TimeRequest::SetSpeed { speed: 5 })?;
        let running = Instant::now();
        let () = game.ask(&TimeRequest::Resume {})?;
        record["running"] = measure(game, &process, running_seconds, collect)?;
        let () = game.ask(&TimeRequest::Pause {})?;
        let ran = running.elapsed().as_secs_f64();
        let after: Time = game.ask(&TimeRequest::Get {})?;
        record["date0"] = json!(before.date.to_string());
        record["date1"] = json!(after.date.to_string());
        // A date counts hours.
        let days = (after.date.0 - before.date.0) as f64 / 24.0;
        record["date_days_a_minute"] = rounded(days * 60.0 / ran, 1);
        record["date_interval_seconds"] = rounded(ran, 3);
        if collect {
            let metrics: Report = serde_json::from_value(record["running"]["metrics"].clone()).map_err(|e| Stop::Failed(e.to_string()))?;
            let completed: u64 = metrics.segments.iter().map(|s| s.days.completed).sum();
            if metrics.complete && metrics.elapsed_ms > 0.0 {
                record["days_a_minute"] = rounded(completed as f64 * 60_000.0 / metrics.elapsed_ms, 1);
            }
        }
        if let Some(since) = since {
            let read: Signals = game.ask(&listen(&kinds, Some(since)))?;
            let mut heard = BTreeMap::new();
            for numbered in &read.signals {
                *heard.entry(numbered.signal.kind()).or_insert(0u32) += 1;
            }
            record["heard"] = json!(heard);
            record["heard_missed"] = json!(read.missed);
        }
        if parameters.flag("reads")? {
            record["reads"] = reads(game)?;
        }
        game.return_to_menu()?;
        if collect {
            for name in ["paused", "running"] {
                let metrics: Report = serde_json::from_value(record[name]["metrics"].clone()).map_err(|e| Stop::Failed(e.to_string()))?;
                game.check(metrics.complete && metrics.reached_deadline, format!("{name} performance collection incomplete: {}", metrics.warnings.join("; ")))?;
            }
        }
        game.check(days > 0.0, format!("no day passed in {ran:.0} s at speed 5"))
    }

    fn report(&self, record: &Value) -> Vec<String> {
        let mut md = vec!["| Figure | Value |".to_string(), "| --- | --- |".into()];
        let mut row = |what: &str, value: String| {
            if !value.is_empty() {
                md.push(format!("| {what} | {value} |"));
            }
        };
        row("Build", text(&record["measurement"]["build"]));
        row("In-process collection", text(&record["measurement"]["collection"]));
        row("Visibility", text(&record["measurement"]["visibility"]));
        row("Seconds to the main menu", text(&record["menu_seconds"]));
        row("At the main menu: working set, private MB", pair(&record["menu"], "ws", "priv"));
        row("CPU at the idle main menu (cores)", text(&record["menu_idle_cpu_share"]));
        row("Seconds menu.start took", text(&record["start_seconds"]));
        row("Paused: CPU (cores)", text(&record["paused"]["cpu_share"]));
        row("Paused: working set, private MB", pair(&record["paused"], "ws", "priv"));
        row("At speed 5: CPU (cores)", text(&record["running"]["cpu_share"]));
        row("At speed 5: working set, private, peak MB", {
            let r = &record["running"];
            if r.is_null() {
                String::new()
            } else {
                format!("{}, {}, {}", text(&r["ws"]), text(&r["priv"]), text(&r["peak"]))
            }
        });
        let legacy = record["measurement"].is_null();
        if !record["date0"].is_null() {
            let seconds = if legacy { &record["running"]["seconds"] } else { &record["date_interval_seconds"] };
            row("Dates (external observation)", format!("{} to {} in {} s", text(&record["date0"]), text(&record["date1"]), text(seconds)));
        }
        if legacy {
            row("Game days a minute at speed 5 (legacy date-based measurement)", text(&record["days_a_minute"]));
        } else {
            row("Completed game days a minute in the speed-5 window", text(&record["days_a_minute"]));
            row("Date-based game days a minute (external observation)", text(&record["date_days_a_minute"]));
        }
        row("Signals listened to", record["signals"].as_array().map(|k| k.iter().map(text).collect::<Vec<_>>().join(", ")).unwrap_or_default());
        row(
            "Signals heard at speed 5",
            record["heard"].as_object().map(|h| h.iter().map(|(k, n)| format!("{k} {}", text(n))).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        );
        for (read, what) in [("country_get", "country.get"), ("country_budget", "country.budget"), ("province_get", "province.get")] {
            let r = &record["reads"][read];
            if !r.is_null() {
                row(&format!("{what}: reads, ms each from the client"), format!("{}, {}", text(&r["count"]), text(&r["ms_each"])));
            }
        }
        md.push(String::new());
        md.push("CPU and memory bracket the window from the client; the in-process metrics use their own exact deadline. Submission is the game's Present wrapper returning success, not monitor display. Pure interface work, action response and GPU time are unmeasured.".into());
        for name in ["paused", "running"] {
            if !record[name]["metrics"].is_null() {
                match serde_json::from_value::<Report>(record[name]["metrics"].clone()) {
                    Ok(metrics) => md.extend(report::metrics(name, &metrics)),
                    Err(e) => md.push(format!("Performance report could not be decoded: {e}")),
                }
            }
        }
        md
    }
}

mod report;

fn window_seconds(parameters: &Parameters, name: &str) -> Played<u32> {
    let seconds = parameters.number(name)?;
    if !(1..=600).contains(&seconds) {
        return Err(Stop::Failed(format!("{name} must be from 1 to 600 seconds")));
    }
    Ok(seconds)
}

/// No polling during collection: its in-process deadline is independent of
/// the time the pipe takes to answer the final request.
fn measure(game: &mut Game, process: &Process, seconds: u32, collect: bool) -> Played<Value> {
    let a = process.sample()?;
    let began = Instant::now();
    if collect {
        let () = game.ask(&PerformanceRequest::Begin { seconds })?;
    }
    pause(seconds as u64);
    let b = process.sample()?;
    let elapsed = began.elapsed().as_secs_f64();
    let metrics = if collect { Some(game.ask::<Report>(&PerformanceRequest::End {})?) } else { None };
    Ok(json!({
        "seconds": elapsed, "cpu_share": rounded((b.cpu - a.cpu) / elapsed, 3),
        "ws": b.ws, "priv": b.private, "peak": b.peak, "metrics": metrics
    }))
}

/// A read of the signals of `kinds` after `since`, which has them worked out
/// from then on.
fn listen(kinds: &[String], since: Option<u64>) -> SignalsRequest {
    SignalsRequest::Read { since, wait_ms: None, process: None, kinds: Some(Words(kinds.to_vec())), countries: None }
}

/// `country.get` and `country.budget` of every country that owns a
/// province, and `province.get` of every owned land province, timed from the
/// client, one read at a time.
fn reads(game: &mut Game) -> Played<Value> {
    let provinces: Vec<Province> = game.ask(&ProvinceRequest::List { provinces: None })?;
    let mut owners: Vec<CountryTag> = provinces.iter().filter_map(|p| p.owner).collect();
    owners.sort();
    owners.dedup();
    // In the player's view a province the player has not discovered has no
    // owner, and province.get refuses it.
    let land: Vec<_> = provinces.iter().filter(|p| p.kind == ProvinceKind::Land && p.owner.is_some()).map(|p| p.id).collect();
    let started = Instant::now();
    for &country in &owners {
        let _: Box<Country> = game.ask(&CountryRequest::Get { country })?;
    }
    let country_get = each(owners.len(), started);
    let started = Instant::now();
    for &country in &owners {
        let _: Budget = game.ask(&CountryRequest::Budget { country })?;
    }
    let country_budget = each(owners.len(), started);
    let started = Instant::now();
    for &province in &land {
        let _: Box<ProvinceSnapshot> = game.ask(&ProvinceRequest::Get { province })?;
    }
    Ok(json!({ "country_get": country_get, "country_budget": country_budget, "province_get": each(land.len(), started) }))
}

/// `count` reads since `started`, and the milliseconds each took.
fn each(count: usize, started: Instant) -> Value {
    json!({ "count": count, "ms_each": rounded(started.elapsed().as_secs_f64() * 1000.0 / count.max(1) as f64, 2) })
}

fn pair(v: &Value, a: &str, b: &str) -> String {
    if v.is_null() {
        String::new()
    } else {
        format!("{}, {}", text(&v[a]), text(&v[b]))
    }
}

fn pause(seconds: u64) {
    std::thread::sleep(Duration::from_secs(seconds));
}

fn rounded(value: f64, decimals: i32) -> Value {
    let scale = 10f64.powi(decimals);
    json!((value * scale).round() / scale)
}

/// The game's process.
pub(super) struct Process {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

/// What the process has used: CPU seconds, and its working set, private
/// bytes and peak working set in MB.
pub(super) struct Sample {
    pub cpu: f64,
    pub ws: u64,
    pub private: u64,
    pub peak: u64,
}

impl Sample {
    fn json(&self) -> Value {
        json!({ "cpu": self.cpu, "ws": self.ws, "priv": self.private, "peak": self.peak })
    }
}

impl Process {
    pub fn find(game: &Game) -> Played<Self> {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
        let id = game.process()?;
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, id) };
        if handle.is_null() {
            return Err(Stop::Failed(format!("cannot open the game's process {id}")));
        }
        Ok(Self { handle })
    }

    pub fn sample(&self) -> Played<Sample> {
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
                return Err(Stop::Crashed("the game's times cannot be read: it is gone".into()));
            }
            let ticks = |t: &FILETIME| ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 1e7;
            let mut memory: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            memory.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            if GetProcessMemoryInfo(self.handle, (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(), memory.cb) == 0 {
                return Err(Stop::Crashed("the game's memory cannot be read: it is gone".into()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_off_keeps_the_same_build_and_switches_compose() {
        let on = Parameters::of(&Perf, &[]).unwrap();
        let off = Parameters::of(&Perf, &[("perf".into(), "collect".into(), "false".into())]).unwrap();
        assert_eq!(Perf.build(&on), Perf.build(&off));
        let switched = Parameters::of(&Perf, &[("perf".into(), "game_paths".into(), "drawing-thread".into())]).unwrap();
        assert_eq!(Perf.build(&switched), Build::Performance { switches: true });
        assert!(Perf.options(&switched).unwrap().contains("drawing-thread"));
        let invalid = Parameters::of(&Perf, &[("perf".into(), "running_seconds".into(), "0".into())]).unwrap();
        assert!(window_seconds(&invalid, "running_seconds").is_err());
    }

    #[test]
    fn historical_date_rates_are_not_relabelled_as_completed_days() {
        let record = json!({ "days_a_minute": 900, "date0": "1444.11.11", "date1": "1449.11.11", "running": { "seconds": 120 } });
        let md = Perf.report(&record).join("\n");
        assert!(md.contains("legacy date-based measurement") && md.contains("in 120 s"), "{md}");
        assert!(!md.contains("Completed game days a minute"), "{md}");
    }

    #[test]
    fn the_report_leaves_out_what_was_not_measured() {
        let record = json!({ "menu_seconds": 51.2, "menu": { "ws": 900, "priv": 1200 }, "menu_idle_cpu_share": 0.04 });
        let md = Perf.report(&record).join("\n");
        assert!(md.contains("| Seconds to the main menu | 51.2 |") && md.contains("| At the main menu: working set, private MB | 900, 1200 |"), "{md}");
        assert!(md.contains("| CPU at the idle main menu (cores) | 0.04 |") && !md.contains("speed 5"), "{md}");
    }
}
