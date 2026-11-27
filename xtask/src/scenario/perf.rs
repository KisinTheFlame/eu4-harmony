//! Standard performance scenarios (`perf-early` and `perf-late`, run together as
//! the `perf` set; see `docs/performance.md` and `docs/dev-tools.md`):
//! - `perf-early`: a fresh 1444.11.11 game in observe mode with Ming (`MNG`)
//!   put in view and the camera positioned on Ming's capital (Beijing, 1816).
//! - `perf-late`: a late-game save (default `observer-1780.7.1`) in observe
//!   mode with the save's rank-1 great power at start (e.g. `TUR` in
//!   `observer-1780.7.1`) put in view and the camera positioned on its capital.
//!
//! Each starts the game afresh on the `dev` build with
//! `[performance] enabled = true` and records:
//! - startup time to the main menu and memory 20 s later;
//! - session start / save load time;
//! - bounded paused (30 s) and speed-5 (120 s) windows: completed days/min,
//!   daily/month-start/year-start durations, submission intervals, update-ready
//!   intervals, interface age, CPU (in cores) and memory.
use super::{text, Build, Game, Parameter, Parameters, Played, Scenario, Stop};
use eu4_harmony_remote::{
    camera::{Camera, CameraRequest},
    countries::{Budget, Country, CountryRequest},
    menu::MenuRequest,
    performance::{PerformanceRequest, Report},
    provinces::{Province, ProvinceKind, ProvinceRequest, ProvinceSnapshot},
    session::{Session, SessionRequest},
    signals::{Signals, SignalsRequest},
    time::{Time, TimeRequest},
    CountryTag, ProvinceId, Words,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Fixed early-game country in view (1444.11.11 Ming).
const EARLY_COUNTRY: CountryTag = CountryTag(*b"MNG");
/// Beijing (`1816`), Ming's capital in 1444.11.11.
const EARLY_CAPITAL: ProvinceId = ProvinceId(1816);
/// Default late-game save for `perf-late` (40.5 years of runway to 1821.1.2).
const DEFAULT_LATE_SAVE: &str = "observer-1780.7.1";

pub struct PerfRun {
    /// False for `perf-early` (1444, Ming); true for `perf-late` (1780 save,
    /// rank-1 great power at start).
    pub late: bool,
}

pub const PERF_EARLY: PerfRun = PerfRun { late: false };
pub const PERF_LATE: PerfRun = PerfRun { late: true };

/// The speed-ups `game_paths` leaves to the game, by name, group or `all`.
fn left_to_the_game(parameters: &Parameters) -> Vec<String> {
    parameters.text("game_paths").split(',').map(str::trim).filter(|n| !n.is_empty()).map(String::from).collect()
}

impl Scenario for PerfRun {
    fn name(&self) -> &'static str {
        if self.late {
            "perf-late"
        } else {
            "perf-early"
        }
    }

    fn about(&self) -> &'static str {
        if self.late {
            "late-game (1780) bounded performance windows in observe mode from the rank-1 great power's view"
        } else {
            "early-game (1444) bounded performance windows in observe mode from Ming's view"
        }
    }

    fn build(&self, _parameters: &Parameters) -> Build {
        Build::Dev
    }

    fn options(&self, parameters: &Parameters) -> Option<String> {
        let mut toml = String::from("[performance]\nenabled = true\n");
        let names = left_to_the_game(parameters);
        if !names.is_empty() {
            let quoted: Vec<String> = names.iter().map(|n| format!("{n:?}")).collect();
            toml.push_str(&format!("\n[test-switches]\ngame-paths = [{}]\n", quoted.join(", ")));
        }
        Some(toml)
    }

    fn saves(&self, parameters: &Parameters) -> Vec<String> {
        if self.late {
            vec![parameters.text("save")]
        } else {
            Vec::new()
        }
    }

    fn minutes(&self) -> u32 {
        // Default 30 s paused + 120 s speed-5 windows, plus startup and warmup.
        4
    }

    fn alone(&self) -> bool {
        true
    }

    fn parameters(&self) -> &'static [Parameter] {
        const EARLY_PARAMS: [Parameter; 7] = [
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
        ];
        const LATE_PARAMS: [Parameter; 8] = [
            Parameter { name: "save", default: DEFAULT_LATE_SAVE, about: "the late-game save to measure (bare save name or explicit .eu4 path)" },
            Parameter { name: "country", default: "auto", about: "country put in view in observe mode; auto selects the save's rank-1 great power at start" },
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
        ];
        if self.late {
            &LATE_PARAMS
        } else {
            &EARLY_PARAMS
        }
    }

    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()> {
        let collect = parameters.flag("collect")?;
        let paused_seconds = window_seconds(parameters, "paused_seconds")?;
        let running_seconds = window_seconds(parameters, "running_seconds")?;
        let menu_only = !self.late && parameters.flag("menu_only")?;

        let (save_spec, tag, country_source, mut capital) = if self.late {
            let save_spec = parameters.text("save");
            let saves_dir = crate::saves::saves_dir().map_err(Stop::Failed)?;
            let save_path = crate::saves::resolve_save(&save_spec, &saves_dir).map_err(Stop::Failed)?;
            let country_param = parameters.text("country");
            let explicit = if country_param.eq_ignore_ascii_case("auto") || country_param.is_empty() {
                None
            } else {
                Some(CountryTag::parse(&country_param).ok_or_else(|| Stop::Failed(format!("no country tag {country_param:?}")))?)
            };
            let info = crate::saves::read_start_info(&save_path, explicit).map_err(Stop::Failed)?;
            let (tag, source) = match explicit {
                Some(tag) => (tag, "parameter"),
                None => {
                    let top = info
                        .top_great_power
                        .ok_or_else(|| Stop::Failed(format!("save {save_spec:?} has no rank-1 great power in great_powers; pass --perf-late.country <TAG>")))?;
                    (top, "top_great_power")
                }
            };
            (Some(save_spec), tag, source, info.capital.map(ProvinceId))
        } else {
            (None, EARLY_COUNTRY, "fixed", Some(EARLY_CAPITAL))
        };

        record["measurement"] = json!({
            "scenario": self.name(),
            "build": self.build(parameters).name(),
            "collection": collect,
            "visibility": "hidden",
            "mode": "observe",
            "save": save_spec,
            "country": tag.to_string(),
            "country_source": country_source,
            "capital": capital.map(|p| p.0),
            "game_paths": left_to_the_game(parameters),
            "paused_seconds": paused_seconds,
            "running_seconds": running_seconds,
            "unmeasured": ["physical display latency", "action response", "pure interface work duration", "GPU time"]
        });

        record["menu_seconds"] = rounded(game.restart()?, 1);
        let process = Process::find(game)?;
        pause(20);
        record["menu"] = process.sample()?.json();
        if menu_only {
            let a = process.sample()?;
            let began = Instant::now();
            pause(paused_seconds as u64);
            let b = process.sample()?;
            record["menu_idle_cpu_share"] = rounded((b.cpu - a.cpu) / began.elapsed().as_secs_f64(), 3);
            return Ok(());
        }

        if let Some(save_spec) = &save_spec {
            let save_name = crate::saves::save_name(save_spec);
            let loading = Instant::now();
            let _: Session = game.ask(&MenuRequest::Load { save: save_name })?;
            record["load_seconds"] = rounded(loading.elapsed().as_secs_f64(), 1);
            let session = game.observe_with_no_country()?;
            game.check(
                session.observing && session.player.is_none(),
                format!("late perf began in observe mode with no country in view, answered {session:?}"),
            )?;
        } else {
            let starting = Instant::now();
            let session: Session = game.ask(&MenuRequest::Start { country: None })?;
            record["start_seconds"] = rounded(starting.elapsed().as_secs_f64(), 1);
            game.check(
                session.observing && session.player.is_none(),
                format!("early perf began in observe mode with no country in view, answered {session:?}"),
            )?;
        }

        let viewed: Session = game.ask(&SessionRequest::View { country: tag })?;
        game.check(viewed.observing && viewed.player == Some(tag), format!("session.view {tag} answered {viewed:?}"))?;
        let () = game.ask(&TimeRequest::Pause {})?;

        if capital.is_none() {
            let provinces: Vec<Province> = game.ask(&ProvinceRequest::List { provinces: None })?;
            capital = provinces.iter().find(|p| p.kind == ProvinceKind::Land && p.owner == Some(tag)).map(|p| p.id);
            record["measurement"]["capital"] = json!(capital.map(|p| p.0));
        }
        if let Some(province) = capital {
            let _: Camera = game.ask(&CameraRequest::LookAt { province: Some(province), x: None, y: None, glide: Some(false) })?;
        }

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
        row("Mode", text(&record["measurement"]["mode"]));
        row("Save", text(&record["measurement"]["save"]));
        let country = text(&record["measurement"]["country"]);
        let source = text(&record["measurement"]["country_source"]);
        if !country.is_empty() {
            let label = if source.is_empty() { country } else { format!("{country} ({source})") };
            row("Country in view", label);
        }
        row("Capital province", text(&record["measurement"]["capital"]));
        if record["camera"].is_object() {
            let cam = &record["camera"];
            let prov = text(&cam["province"]);
            let suffix = if prov.is_empty() { String::new() } else { format!(" (province {prov})") };
            row("Camera", format!("{}, {}, height {}{suffix}", text(&cam["x"]), text(&cam["y"]), text(&cam["height"])));
        }
        row("Seconds to the main menu", text(&record["menu_seconds"]));
        row("At the main menu: working set, private MB", pair(&record["menu"], "ws", "priv"));
        row("CPU at the idle main menu (cores)", text(&record["menu_idle_cpu_share"]));
        row("Seconds menu.start took", text(&record["start_seconds"]));
        row("Seconds menu.load took", text(&record["load_seconds"]));
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
    fn perf_early_and_perf_late_defaults_match_the_standard_protocol() {
        let early = Parameters::of(&PERF_EARLY, &[]).unwrap();
        let late = Parameters::of(&PERF_LATE, &[]).unwrap();
        assert_eq!(PERF_EARLY.build(&early), Build::Dev);
        assert_eq!(PERF_LATE.build(&late), Build::Dev);
        assert!(PERF_EARLY.saves(&early).is_empty());
        assert_eq!(PERF_LATE.saves(&late), ["observer-1780.7.1"]);
        assert_eq!(late.text("country"), "auto");

        let off = Parameters::of(&PERF_EARLY, &[("perf-early".into(), "collect".into(), "false".into())]).unwrap();
        assert_eq!(PERF_EARLY.build(&early), PERF_EARLY.build(&off));
        assert!(PERF_EARLY.options(&early).unwrap().contains("[performance]\nenabled = true"));

        let switched = Parameters::of(&PERF_LATE, &[("perf-late".into(), "game_paths".into(), "drawing-thread".into())]).unwrap();
        let options = PERF_LATE.options(&switched).unwrap();
        assert!(options.contains("[performance]\nenabled = true") && options.contains("drawing-thread"));

        let invalid = Parameters::of(&PERF_EARLY, &[("perf-early".into(), "running_seconds".into(), "0".into())]).unwrap();
        assert!(window_seconds(&invalid, "running_seconds").is_err());
    }

    #[test]
    fn historical_date_rates_are_not_relabelled_as_completed_days() {
        let record = json!({ "days_a_minute": 900, "date0": "1444.11.11", "date1": "1449.11.11", "running": { "seconds": 120 } });
        let md = PERF_EARLY.report(&record).join("\n");
        assert!(md.contains("legacy date-based measurement") && md.contains("in 120 s"), "{md}");
        assert!(!md.contains("Completed game days a minute"), "{md}");
    }

    #[test]
    fn the_report_shows_observe_view_metadata_and_leaves_out_unmeasured_rows() {
        let menu = json!({ "menu_seconds": 51.2, "menu": { "ws": 900, "priv": 1200 }, "menu_idle_cpu_share": 0.04 });
        let md = PERF_EARLY.report(&menu).join("\n");
        assert!(md.contains("| Seconds to the main menu | 51.2 |") && md.contains("| At the main menu: working set, private MB | 900, 1200 |"), "{md}");
        assert!(md.contains("| CPU at the idle main menu (cores) | 0.04 |") && !md.contains("speed 5"), "{md}");

        let late = json!({
            "measurement": {
                "scenario": "perf-late", "build": "developer", "collection": true, "visibility": "hidden",
                "mode": "observe", "save": "observer-1780.7.1", "country": "TUR", "country_source": "top_great_power", "capital": 151
            },
            "camera": { "x": 3100, "y": 1300, "height": 300, "province": 151 },
            "load_seconds": 5.4
        });
        let md = PERF_LATE.report(&late).join("\n");
        assert!(md.contains("| Mode | observe |"), "{md}");
        assert!(md.contains("| Save | observer-1780.7.1 |"), "{md}");
        assert!(md.contains("| Country in view | TUR (top_great_power) |"), "{md}");
        assert!(md.contains("| Capital province | 151 |"), "{md}");
        assert!(md.contains("| Seconds menu.load took | 5.4 |"), "{md}");
    }
}
