//! `cargo xtask scenario`: plays scenarios (GLOSSARY), courses of play in
//! the real game driven through remote control, within one claim, and writes
//! one report of them (docs/scenarios.md).
//!
//! The runner does everything but the playing: it finds Steam ready, claims
//! the game unless this checkout holds it, builds and installs each build the
//! scenarios named need, starts the game hidden, hands each scenario the game
//! at its main menu, tells how it ended, keeps the lease going and notices
//! the player taking the game, and releases the claim it made. Only cargo,
//! for the builds, and the game run as programs of their own.
mod game;
mod leak;
mod perf;

pub use game::{Game, Played, Stop};

use crate::{claim, files, repo, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

/// The builds a scenario may need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// The player build: no development tools.
    Player,
    /// A developer build with the memory account, which costs the game
    /// nothing between two asks.
    Memory,
    /// A developer build with the heap census, which slows the game and holds
    /// memory of its own.
    Census,
}

impl Build {
    pub fn name(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Memory => "memory account",
            Self::Census => "heap census",
        }
    }

    /// The core's feature that makes it, if any.
    fn feature(self) -> Option<&'static str> {
        match self {
            Self::Player => None,
            Self::Memory => Some("eu4-harmony-core/dev-memory"),
            Self::Census => Some("eu4-harmony-core/dev-heap-census"),
        }
    }
}

/// One of a scenario's parameters: its name, its value in the code, and what
/// it is.
#[derive(Clone, Copy)]
pub struct Parameter {
    pub name: &'static str,
    pub default: &'static str,
    pub about: &'static str,
}

/// A scenario's parameters for a run: each as the code has it, or as the
/// command line gave it.
#[derive(Debug, Default)]
pub struct Parameters(BTreeMap<String, String>);

impl Parameters {
    /// The parameters of `scenario`, with the values `given` for its name;
    /// one it does not have, or a number or flag of another kind, is refused.
    fn of(scenario: &dyn Scenario, given: &[(String, String, String)]) -> Result<Self> {
        let mut values: BTreeMap<String, String> = scenario.parameters().iter().map(|p| (p.name.to_string(), p.default.to_string())).collect();
        for (name, parameter, value) in given.iter().filter(|(name, _, _)| name == scenario.name()) {
            let Some(declared) = scenario.parameters().iter().find(|p| p.name == parameter) else {
                return Err(format!("{name} has no parameter {parameter}; cargo xtask scenario lists them"));
            };
            let kind_holds = if declared.default.parse::<u32>().is_ok() {
                value.parse::<u32>().is_ok()
            } else if matches!(declared.default, "true" | "false") {
                matches!(value.as_str(), "true" | "false")
            } else {
                true
            };
            if !kind_holds {
                return Err(format!("--{name}.{parameter} {value}: of the kind of {}", declared.default));
            }
            values.insert(parameter.clone(), value.clone());
        }
        Ok(Self(values))
    }

    pub fn text(&self, name: &str) -> String {
        self.0.get(name).cloned().unwrap_or_default()
    }

    pub fn number(&self, name: &str) -> Played<u32> {
        self.text(name).parse().map_err(|_| Stop::Failed(format!("the parameter {name} is not a whole number")))
    }

    pub fn flag(&self, name: &str) -> Played<bool> {
        match self.text(name).as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(Stop::Failed(format!("the parameter {name} is {other:?}, not true or false"))),
        }
    }

    fn json(&self) -> Value {
        json!(self.0)
    }
}

/// A course of play in the real game.
pub trait Scenario: Sync {
    fn name(&self) -> &'static str;
    /// What it plays and measures, in a line.
    fn about(&self) -> &'static str;
    fn build(&self) -> Build;
    /// About how long it plays with its parameters as the code has them.
    fn minutes(&self) -> u32;
    fn parameters(&self) -> &'static [Parameter];
    /// Plays the game, handed at its main menu, keeping in `record` what it
    /// found as it goes: a scenario that ends early keeps what it had.
    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()>;
    /// Its sections of the report, from its record; headings from `###`.
    fn report(&self, record: &Value) -> Vec<String>;
}

fn all() -> Vec<&'static dyn Scenario> {
    vec![&perf::Perf, &leak::GROWTH, &leak::LEAK]
}

fn find(name: &str) -> Option<&'static dyn Scenario> {
    all().into_iter().find(|s| s.name() == name)
}

/// How often the runner looks at its claim.
const LOOK_EVERY: Duration = Duration::from_secs(5);
/// The lease is renewed when less than this is left of it.
const RENEW_BELOW: u64 = 10 * 60;
/// How long a crash dump may take to read before the report goes without it.
const READING_A_DUMP: Duration = Duration::from_secs(180);

const USAGE: &str =
    "cargo xtask scenario [<name>... [--out <dir>] [--wait] [--owner <name>] [--game-dir <dir>] [--<name>.<parameter> <value>]...] | --from-report <dir>";

/// What the command line asked for.
#[derive(Debug, Default, PartialEq)]
struct Asked {
    names: Vec<String>,
    out: Option<PathBuf>,
    wait: bool,
    owner: Option<String>,
    game_dir: Option<PathBuf>,
    from_report: Option<PathBuf>,
    /// Scenario, parameter and value.
    parameters: Vec<(String, String, String)>,
}

fn parse(raw: &[String]) -> Result<Asked> {
    let mut asked = Asked::default();
    let mut words = raw.iter();
    while let Some(word) = words.next() {
        let mut value = |key: &str| words.next().cloned().ok_or_else(|| format!("--{key} needs a value"));
        match word.strip_prefix("--") {
            Some("wait") => asked.wait = true,
            Some("out") => asked.out = Some(value("out")?.into()),
            Some("owner") => asked.owner = Some(value("owner")?),
            Some("game-dir") => asked.game_dir = Some(value("game-dir")?.into()),
            Some("from-report") => asked.from_report = Some(value("from-report")?.into()),
            Some(key) => match key.split_once('.') {
                Some((name, parameter)) => asked.parameters.push((name.into(), parameter.into(), value(key)?)),
                None => return Err(format!("unknown option --{key}; {USAGE}")),
            },
            None => asked.names.push(word.clone()),
        }
    }
    Ok(asked)
}

pub fn run(raw: &[String]) -> Result<()> {
    let asked = parse(raw)?;
    if let Some(dir) = &asked.from_report {
        let path = dir.join("report.json");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let report: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", path.display()))?;
        write(&dir.join("report.md"), &markdown(&report))?;
        println!("report: {}", dir.join("report.md").display());
        return Ok(());
    }
    if asked.names.is_empty() {
        list();
        return Ok(());
    }
    let mut playing = Vec::new();
    for name in &asked.names {
        let scenario = find(name).ok_or_else(|| format!("no scenario {name}; cargo xtask scenario lists them"))?;
        playing.push((scenario, Parameters::of(scenario, &asked.parameters)?));
    }
    if let Some((name, parameter, _)) = asked.parameters.iter().find(|(name, _, _)| !asked.names.contains(name)) {
        return Err(format!("--{name}.{parameter} is for a scenario this run does not play"));
    }
    let game_dir = match &asked.game_dir {
        Some(dir) => dir.clone(),
        None => crate::game::dir()?,
    };
    crate::game::steam_ready()?;

    let owner = asked.owner.clone().unwrap_or_else(|| claim::owner_of(None));
    let held_before = claim::held_by(&owner).is_ok();
    if !held_before {
        let purpose = format!("scenarios {}", asked.names.join(", "));
        if claim::take(&owner, &purpose, asked.wait, &game_dir)? == claim::Claimed::Busy {
            return Err("the game is busy; --wait waits for it in the queue".into());
        }
    }
    let out = asked.out.clone().unwrap_or_else(|| repo().join("target").join("scenarios").join(stamp().1));
    if let Err(e) = std::fs::create_dir_all(&out) {
        if !held_before {
            claim::give_back(&owner, &game_dir, false)?;
        }
        return Err(format!("cannot make {}: {e}", out.display()));
    }
    let taken = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let looking = look_at_the_claim(owner.clone(), taken.clone(), done.clone());

    let mut played = Vec::new();
    for build in builds(&playing) {
        let these: Vec<&(&dyn Scenario, Parameters)> = playing.iter().filter(|(s, _)| s.build() == build).collect();
        let ready = if taken.load(Ordering::SeqCst) { Err("the player took the game".to_string()) } else { prepare(build, &game_dir) };
        let mut game = Game::new(&owner, game_dir.clone(), taken.clone());
        for (scenario, parameters) in these {
            let entry = match &ready {
                Err(e) => not_played(*scenario, parameters, e),
                Ok(()) if taken.load(Ordering::SeqCst) => not_played(*scenario, parameters, "the player took the game"),
                Ok(()) => play(*scenario, parameters, &mut game, &game_dir, &out),
            };
            played.push(entry);
        }
    }
    let was_taken = taken.load(Ordering::SeqCst);
    if !was_taken {
        crate::game::kill();
        let _ = crate::game::wait_closed();
    }
    done.store(true, Ordering::SeqCst);
    let _ = looking.join();

    let report = json!({ "date": stamp().0, "owner": owner, "scenarios": played });
    write(&out.join("report.json"), &serde_json::to_string_pretty(&report).expect("JSON"))?;
    write(&out.join("report.md"), &markdown(&report))?;
    println!("report: {}", out.join("report.md").display());
    if was_taken {
        println!("the player took the game: their claim ends it and puts back this run's files");
    } else if !held_before {
        claim::give_back(&owner, &game_dir, false)?;
    }
    let failed = played.iter().filter(|p| p["outcome"] != "passed").count();
    if failed > 0 {
        return Err(format!("{failed} of {} scenarios did not pass", played.len()));
    }
    Ok(())
}

/// Lists the scenarios.
fn list() {
    println!("{USAGE}\n\nscenarios:");
    for s in all() {
        println!("  {} ({} build, about {} min): {}", s.name(), s.build().name(), s.minutes(), s.about());
        for p in s.parameters() {
            println!("      --{}.{} {}: {}", s.name(), p.name, p.default, p.about);
        }
    }
}

/// The builds the scenarios need, in the order they are first named.
fn builds(playing: &[(&dyn Scenario, Parameters)]) -> Vec<Build> {
    let mut builds = Vec::new();
    for (s, _) in playing {
        if !builds.contains(&s.build()) {
            builds.push(s.build());
        }
    }
    builds
}

/// Builds `build` and installs it, with the game closed.
fn prepare(build: Build, game_dir: &Path) -> Result<()> {
    crate::game::kill();
    crate::game::wait_closed()?;
    println!("building the {} build", build.name());
    files::build(build.feature())?;
    files::install_built(game_dir)
}

/// Looks at the claim every few seconds until `done`: renews the lease, and
/// sets `taken` once the player takes the game or the claim is gone.
fn look_at_the_claim(owner: String, taken: Arc<AtomicBool>, done: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !done.load(Ordering::SeqCst) {
            match claim::look(&owner) {
                Ok(claim::Look::Held(left)) if left < RENEW_BELOW => {
                    let _ = claim::renew_as(&owner);
                }
                Ok(claim::Look::Taken | claim::Look::Gone) => taken.store(true, Ordering::SeqCst),
                _ => {}
            }
            std::thread::sleep(LOOK_EVERY);
        }
    })
}

/// Plays one scenario; its entry in the report.
fn play(scenario: &dyn Scenario, parameters: &Parameters, game: &mut Game, game_dir: &Path, out: &Path) -> Value {
    println!("playing {}", scenario.name());
    let began = Instant::now();
    let since = SystemTime::now();
    let mut record = json!({});
    game.next_scenario();
    // A game that runs is brought to its main menu; one that does not is
    // started by the scenario's first step, which a scenario that times the
    // start takes itself.
    let at_main_menu = if crate::game::running() { game.at_main_menu() } else { Ok(()) };
    let ended = at_main_menu.and_then(|()| scenario.play(game, parameters, &mut record));
    let (outcome, why) = match &ended {
        Ok(()) => ("passed", None),
        Err(Stop::Failed(why)) => ("failed", Some(why.clone())),
        Err(Stop::Crashed(why)) => ("crashed", Some(why.clone())),
        Err(Stop::Hung(why)) => ("hung", Some(why.clone())),
        Err(Stop::Taken) => ("unfinished", Some("the player took the game".into())),
    };
    let mut entry = entry(scenario, parameters, outcome, why.as_deref());
    entry["minutes"] = json!((began.elapsed().as_secs_f64() / 6.0).round() / 10.0);
    match ended {
        Err(Stop::Crashed(_)) => {
            entry["crash"] = crash(game_dir, since, &out.join(format!("{}-crash.log", scenario.name())));
            let _ = crate::game::wait_closed();
        }
        Err(Stop::Hung(_)) => {
            crate::game::kill();
            let _ = crate::game::wait_closed();
        }
        _ => {}
    }
    entry["record"] = record;
    println!("{}: {outcome}{}", scenario.name(), why.map_or(String::new(), |w| format!(": {w}")));
    entry
}

fn entry(scenario: &dyn Scenario, parameters: &Parameters, outcome: &str, why: Option<&str>) -> Value {
    json!({ "name": scenario.name(), "about": scenario.about(), "build": scenario.build().name(), "parameters": parameters.json(), "outcome": outcome, "ended": why })
}

fn not_played(scenario: &dyn Scenario, parameters: &Parameters, why: &str) -> Value {
    entry(scenario, parameters, "not played", Some(why))
}

/// What the game's newest crash folder since `since` says: its exception,
/// and the stack its dump gives, written to `log` too.
fn crash(game_dir: &Path, since: SystemTime, log: &Path) -> Value {
    let Ok(user) = eu4_harmony_manifest::user_dir(Some(game_dir)) else { return Value::Null };
    let newest = std::fs::read_dir(user.join("crashes"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(modified, _)| *modified >= since)
        .max();
    let Some((_, folder)) = newest else { return json!({ "folder": null }) };
    let exception: Vec<String> = std::fs::read_to_string(folder.join("exception.txt")).unwrap_or_default().lines().take(12).map(String::from).collect();
    let dump = folder.join("minidump.dmp");
    let mut stack = Vec::new();
    if dump.is_file() {
        let (dump_text, log_text) = (dump.display().to_string(), log.display().to_string());
        let reading = std::thread::spawn(move || crate::crash_dump::read(&dump_text, &log_text, "C:\\symbols", &[".ecxr".into(), "kn 20".into()]));
        let deadline = Instant::now() + READING_A_DUMP;
        while !reading.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
        if reading.is_finished() {
            let text = std::fs::read_to_string(log).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            stack = lines[lines.len().saturating_sub(24)..].iter().map(|l| l.to_string()).collect();
        }
    }
    json!({ "folder": folder.display().to_string(), "exception": exception, "stack": stack, "log": log.display().to_string() })
}

/// The report: a table of the scenarios, then each one's sections.
fn markdown(report: &Value) -> String {
    let scenarios = report["scenarios"].as_array().cloned().unwrap_or_default();
    let mut md = vec![
        format!("# Scenarios, {}", text(&report["date"])),
        String::new(),
        "| Scenario | Build | Outcome | Ended at | Minutes |".to_string(),
        "| --- | --- | --- | --- | --- |".into(),
    ];
    for s in &scenarios {
        md.push(format!("| {} | {} | {} | {} | {} |", text(&s["name"]), text(&s["build"]), text(&s["outcome"]), text(&s["ended"]), text(&s["minutes"])));
    }
    for s in &scenarios {
        md.extend([String::new(), format!("## {}", text(&s["name"])), String::new(), format!("{}.", capitalized(&text(&s["about"])))]);
        if let Some(parameters) = s["parameters"].as_object().filter(|p| !p.is_empty()) {
            let written: Vec<String> = parameters.iter().map(|(k, v)| format!("{k} {}", text(v))).collect();
            md.push(format!("Parameters: {}.", written.join(", ")));
        }
        let crash = &s["crash"];
        if crash.is_object() {
            md.extend([String::new(), "### The crash".into(), String::new()]);
            if crash["folder"].is_null() {
                md.push("No crash folder of the game's came of it.".into());
            } else {
                md.push(format!("The game's crash folder: `{}`.", text(&crash["folder"])));
                for (title, key) in [("Its exception", "exception"), ("The stack its dump gives", "stack")] {
                    let lines: Vec<String> = crash[key].as_array().into_iter().flatten().map(text).collect();
                    if !lines.is_empty() {
                        md.extend([String::new(), format!("{title}:"), String::new(), "```text".into()]);
                        md.extend(lines);
                        md.push("```".into());
                    }
                }
                if crash["stack"].as_array().is_none_or(|s| s.is_empty()) {
                    md.extend([String::new(), "Its dump was not read in time.".into()]);
                }
            }
        }
        if let Some(scenario) = find(&text(&s["name"])) {
            let record = &s["record"];
            if record.as_object().is_some_and(|r| !r.is_empty()) {
                md.push(String::new());
                md.extend(scenario.report(record));
            }
        }
    }
    md.iter().map(|l| format!("{l}\n")).collect()
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// A string as itself, null as nothing, anything else as JSON.
pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The local date and time: `yyyy-MM-dd HH:mm`, and `yyyyMMdd-HHmmss` for a
/// folder's name.
fn stamp() -> (String, String) {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    (
        format!("{:04}-{:02}-{:02} {:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute),
        format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond),
    )
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn the_command_line_names_scenarios_and_their_parameters() {
        let asked = parse(&words("perf leak --wait --leak.rounds 1 --out D:\\r")).unwrap();
        assert_eq!(asked.names, ["perf", "leak"]);
        assert!(asked.wait && asked.out == Some(PathBuf::from("D:\\r")));
        assert_eq!(asked.parameters, [("leak".to_string(), "rounds".to_string(), "1".to_string())]);
        assert!(parse(&words("--rounds 1")).unwrap_err().contains("unknown option --rounds"));
        assert!(parse(&words("perf --out")).unwrap_err().contains("--out needs a value"));
    }

    #[test]
    fn a_parameter_keeps_its_value_unless_given_one_of_its_kind() {
        let given = |v: &str| vec![("perf".to_string(), "menu_only".to_string(), v.to_string())];
        let p = Parameters::of(&perf::Perf, &[]).unwrap();
        assert_eq!((p.text("country"), p.flag("menu_only")), ("MNG".to_string(), Ok(false)));
        assert_eq!(Parameters::of(&perf::Perf, &given("true")).unwrap().flag("menu_only"), Ok(true));
        assert!(Parameters::of(&perf::Perf, &given("yes")).unwrap_err().contains("of the kind of false"));
        let unknown = vec![("perf".to_string(), "rounds".to_string(), "1".to_string())];
        assert!(Parameters::of(&perf::Perf, &unknown).unwrap_err().contains("perf has no parameter rounds"));
    }

    #[test]
    fn the_report_says_how_each_scenario_ended_and_what_it_found() {
        let report = json!({ "date": "2026-11-05 12:00", "scenarios": [
            { "name": "perf", "about": "what one run costs", "build": "player", "outcome": "passed", "ended": null, "minutes": 4.5,
              "parameters": { "country": "MNG" }, "record": { "menu_seconds": 50.0 } },
            { "name": "perf", "about": "what one run costs", "build": "player", "outcome": "crashed", "ended": "the game ended", "minutes": 1.0,
              "parameters": {}, "record": {}, "crash": { "folder": "C:\\crashes\\eu4_1", "exception": ["EXCEPTION_ACCESS_VIOLATION"], "stack": [] } },
        ] });
        let md = markdown(&report);
        assert!(md.starts_with("# Scenarios, 2026-11-05 12:00\n\n| Scenario | Build | Outcome | Ended at | Minutes |\n"), "{md}");
        assert!(md.contains("| perf | player | passed |  | 4.5 |\n| perf | player | crashed | the game ended | 1.0 |"), "{md}");
        assert!(md.contains("## perf\n\nWhat one run costs.\nParameters: country MNG.\n\n| Figure | Value |"), "{md}");
        assert!(
            md.contains("### The crash\n\nThe game's crash folder: `C:\\crashes\\eu4_1`.\n\nIts exception:\n\n```text\nEXCEPTION_ACCESS_VIOLATION\n```"),
            "{md}"
        );
        assert!(md.contains("Its dump was not read in time."), "{md}");
    }
}
