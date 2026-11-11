//! `cargo xtask scenario`: plays scenarios (GLOSSARY), courses of play in
//! the real game driven through remote control, and writes one report of
//! them (docs/plans/scenarios.md).
//!
//! The runner does everything but the playing: it finds Steam ready, builds
//! each build the scenarios named need, with the test games' pipes, claims
//! a test game for each scenario, named `<owner>-<scenario>-<six letters
//! and digits>` so that no other run's is it, starts it hidden, hands the
//! scenario the game at its main menu, tells how it ended, and releases the
//! claim, whatever ends the run: an error, a panic or Ctrl-C
//! (docs/plans/test-games.md). Only cargo, for the builds, and the game run as
//! programs of their own.
mod extensions;
mod game;
mod late;
mod leak;
mod logs;
mod perf;
mod sessions;
mod sweep;

pub use game::{Game, Played, Stop};

use crate::{
    claim::{self, GameEnd},
    files, repo, Result,
};
use eu4_harmony_folders::UserFolder;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

/// The builds a scenario may need.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// The player build, with the test games' pipes only, which name the
    /// pipe and nothing else.
    Player,
    /// A developer build with the memory account, which costs the game
    /// nothing between two asks.
    Memory,
    /// A developer build with the heap census, which slows the game and holds
    /// memory of its own.
    Census,
    /// A developer build with the test switches, which leave speed-ups to
    /// the game (core/src/speed.rs).
    Switches,
}

impl Build {
    pub fn name(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Memory => "memory account",
            Self::Census => "heap census",
            Self::Switches => "test switches",
        }
    }

    /// The core's features that make it: always the test games' pipes, for
    /// a test game is where it plays.
    fn features(self) -> &'static str {
        match self {
            Self::Player => "eu4-harmony-core/dev-test-games",
            Self::Memory => "eu4-harmony-core/dev-test-games,eu4-harmony-core/dev-memory",
            Self::Census => "eu4-harmony-core/dev-test-games,eu4-harmony-core/dev-heap-census",
            Self::Switches => "eu4-harmony-core/dev-test-games,eu4-harmony-core/dev-test-switches",
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
    /// The build it plays on with these parameters.
    fn build(&self, parameters: &Parameters) -> Build;
    /// About how long it plays with its parameters as the code has them.
    fn minutes(&self) -> u32;
    /// Whether it plays with no other test game running beside it, as one
    /// that measures what the game costs the computer does.
    fn alone(&self) -> bool {
        false
    }
    fn parameters(&self) -> &'static [Parameter];
    /// Plays the game, handed at its main menu, keeping in `record` what it
    /// found as it goes: a scenario that ends early keeps what it had.
    fn play(&self, game: &mut Game, parameters: &Parameters, record: &mut Value) -> Played<()>;
    /// The saves of the player's it loads, by their names as `menu.saves`
    /// gives them, copied into its test game before it starts.
    /// The developer tools' options its core starts with, in place of the
    /// checkout's `dev.toml`; None for the checkout's.
    fn options(&self, _parameters: &Parameters) -> Option<String> {
        None
    }
    fn saves(&self, _parameters: &Parameters) -> Vec<String> {
        Vec::new()
    }
    /// The test mods it puts in its test game's playset: folders of Harmony
    /// mods (docs/plans/whole-game.md).
    fn mods(&self, _parameters: &Parameters) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Its sections of the report, from its record; headings from `###`.
    fn report(&self, record: &Value) -> Vec<String>;
}

fn all() -> Vec<&'static dyn Scenario> {
    let mut all: Vec<&'static dyn Scenario> =
        vec![&perf::Perf, &leak::GROWTH, &leak::LEAK, &sweep::Sweep, &sessions::Sessions, &extensions::Extensions, &late::Late, &late::LONG, &late::CHECKED];
    all.extend(late::SEGMENTS.iter().map(|s| s as &dyn Scenario));
    all
}

fn find(name: &str) -> Option<&'static dyn Scenario> {
    all().into_iter().find(|s| s.name() == name)
}

/// A name for several scenarios played together, as if named one by one.
pub struct Set {
    pub name: &'static str,
    pub about: &'static str,
    pub scenarios: &'static [&'static str],
    /// How many of them play at once unless `--jobs` says.
    pub jobs: usize,
}

/// The sets, in `cargo xtask scenario`'s list after the scenarios.
const SETS: &[Set] = &[
    Set {
        name: "whole",
        about: "the framework through the real game from end to end, the long game in stretches side by side (docs/plans/whole-game.md)",
        scenarios: &[
            "sweep",
            "sessions",
            "extensions",
            "late",
            "long-1444",
            "long-1600",
            "long-1700",
            "long-1760",
            "long-1780",
            "long-1800",
            "long-1820",
            "checked",
            "growth",
            "perf",
        ],
        jobs: claim::AT_ONCE,
    },
    Set {
        name: "release",
        about: "whole, and the long game played through in one, before a release (docs/plans/whole-game.md)",
        scenarios: &["whole", "long"],
        jobs: claim::AT_ONCE,
    },
];

/// The names asked for, each set's in its place by the scenarios it names,
/// a set it names by its own.
fn expanded(names: &[String]) -> Vec<String> {
    names
        .iter()
        .flat_map(|name| match SETS.iter().find(|set| set.name == name) {
            Some(set) => expanded(&set.scenarios.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            None => vec![name.clone()],
        })
        .collect()
}

/// How long a scenario that plays alone waits for other sessions' games to
/// end before it plays beside them.
const ALONE_WAIT: Duration = Duration::from_secs(20 * 60);

/// Whether a scenario that plays alone waits on, `others` other games
/// running and `waited` gone by.
fn alone_waits(others: usize, waited: Duration) -> bool {
    others > 0 && waited < ALONE_WAIT
}

/// How long a crash dump may take to read before the report goes without it.
const READING_A_DUMP: Duration = Duration::from_secs(180);
/// How long a test game that crashed may take to end on its own, its crash
/// handler writing its dump, before it is ended.
const CRASH_HANDLER: Duration = Duration::from_secs(60);

const USAGE: &str =
    "cargo xtask scenario [<name>... --owner <name> [--out <dir>] [--jobs <n>] [--game-dir <dir>] [--<name>.<parameter> <value>]...] | --from-report <dir>";

/// What the command line asked for.
#[derive(Debug, Default, PartialEq)]
struct Asked {
    names: Vec<String>,
    out: Option<PathBuf>,
    owner: Option<String>,
    /// How many scenarios play at once, each in a test game of its own.
    jobs: Option<usize>,
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
            Some("out") => asked.out = Some(value("out")?.into()),
            Some("owner") => asked.owner = Some(value("owner")?),
            Some("jobs") => {
                let n = value("jobs")?;
                asked.jobs = Some(n.parse().ok().filter(|&n| n > 0).ok_or_else(|| format!("--jobs {n}: a whole number above 0"))?);
            }
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
    let names = expanded(&asked.names);
    let mut playing = Vec::new();
    for name in &names {
        let scenario = find(name).ok_or_else(|| format!("no scenario {name}; cargo xtask scenario lists them"))?;
        playing.push((scenario, Parameters::of(scenario, &asked.parameters)?));
    }
    if let Some((name, parameter, _)) = asked.parameters.iter().find(|(name, _, _)| !names.contains(name)) {
        return Err(format!("--{name}.{parameter} is for a scenario this run does not play"));
    }
    let game_dir = match &asked.game_dir {
        Some(dir) => dir.clone(),
        None => crate::game::dir()?,
    };
    crate::game::steam_ready()?;

    let owner = claim::named_owner(asked.owner.as_deref())?;
    let jobs = asked.jobs.unwrap_or_else(|| SETS.iter().filter(|set| asked.names.contains(&set.name.to_string())).map(|set| set.jobs).max().unwrap_or(1));
    // More would only wait at their starts for one of the test games that
    // run at once on the computer.
    if jobs > claim::AT_ONCE {
        println!("--jobs {jobs}: {} at a time, as at most {} test games run at once", claim::AT_ONCE, claim::AT_ONCE);
    }
    let jobs = jobs.min(claim::AT_ONCE);
    let out = asked.out.clone().unwrap_or_else(|| repo().join("target").join("scenarios").join(stamp().1));
    std::fs::create_dir_all(&out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;
    let interrupts = Interrupts::catch();
    let ended = play_at_once(&playing, &owner, jobs, &game_dir, &out);
    drop(interrupts);
    ended
}

/// Set by Ctrl-C or Ctrl-Break while scenarios play: the scenario playing
/// ends with the game, the others are not played, and the runner gives the
/// game back.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// The run was interrupted.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Catches Ctrl-C and Ctrl-Break while it lives: the first sets
/// [`INTERRUPTED`]; another ends the runner at once, as without it, leaving
/// the claim to run out.
struct Interrupts;

impl Interrupts {
    fn catch() -> Self {
        unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(on_ctrl), 1) };
        Self
    }
}

impl Drop for Interrupts {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(on_ctrl), 0) };
    }
}

/// Windows calls it, on a thread of its own, for a console's Ctrl-C or
/// Ctrl-Break; it says whether it took the event.
unsafe extern "system" fn on_ctrl(kind: u32) -> windows_sys::core::BOOL {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
    if (kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT) && !INTERRUPTED.swap(true, Ordering::SeqCst) {
        println!("interrupted: ending the game and giving it back; again to end at once, leaving the claim");
        return 1;
    }
    0
}

/// Writes the report of the scenarios `played`; fails when one did not pass.
fn reported(owner: &str, played: &[Value], out: &Path) -> Result<()> {
    let report = json!({ "date": stamp().0, "owner": owner, "scenarios": played });
    write(&out.join("report.json"), &serde_json::to_string_pretty(&report).expect("JSON"))?;
    write(&out.join("report.md"), &markdown(&report))?;
    println!("report: {}", out.join("report.md").display());
    let failed = played.iter().filter(|p| p["outcome"] != "passed").count();
    if failed > 0 {
        return Err(format!("{failed} of {} scenarios did not pass", played.len()));
    }
    Ok(())
}

/// Plays the scenarios `jobs` at a time, each in a test game of its own
/// claimed as `<owner>-<scenario>-<suffix>`, the longest first, and those that play
/// alone after the rest; and writes the report, in the order they were
/// named. Each build they need is made first and copied aside, into the
/// report's folder, so that its scenarios start it however many builds follow.
fn play_at_once(playing: &[(&dyn Scenario, Parameters)], owner: &str, jobs: usize, game_dir: &Path, out: &Path) -> Result<()> {
    let mut made: Vec<(Build, std::result::Result<PathBuf, String>)> = Vec::new();
    for build in builds(playing) {
        let copy = match interrupted() {
            true => Err("the run was interrupted".to_string()),
            false => {
                println!("building the {} build", build.name());
                files::build(Some(build.features())).and_then(|()| set_aside(build, out))
            }
        };
        made.push((build, copy));
    }
    let build_of = |i: usize| {
        let (s, p) = &playing[i];
        made.iter().find(|(b, _)| *b == s.build(p)).map(|(_, copy)| copy.clone()).expect("every build was made")
    };
    let mut together: Vec<usize> = (0..playing.len()).filter(|&i| !playing[i].0.alone()).collect();
    together.sort_by_key(|&i| std::cmp::Reverse(playing[i].0.minutes()));
    let queue = Mutex::new(VecDeque::from(together));
    let played: Mutex<Vec<Option<Value>>> = Mutex::new(vec![None; playing.len()]);
    let play_one = |i: usize| {
        let (scenario, parameters) = &playing[i];
        let entry = play_claimed(*scenario, parameters, &format!("{owner}-{}-{}", scenario.name(), suffix()), &build_of(i), game_dir, out);
        played.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(entry);
    };
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                // Taken off the queue first, so that the lock is not held
                // while the scenario plays.
                let next = queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
                match next {
                    Some(i) => play_one(i),
                    None => break,
                }
            });
        }
    });
    for i in (0..playing.len()).filter(|&i| playing[i].0.alone()) {
        play_one(i);
    }
    let played: Vec<Value> = played.into_inner().unwrap_or_else(|e| e.into_inner()).into_iter().flatten().collect();
    reported(owner, &played, out)
}

/// Six lowercase letters and digits, new at each call: what makes a
/// scenario's test game's name its own.
fn suffix() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut n = std::collections::hash_map::RandomState::new().build_hasher().finish();
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    n ^= hasher.finish();
    (0..6)
        .map(|_| {
            let digit = (n % 36) as u8;
            n /= 36;
            char::from(if digit < 10 { b'0' + digit } else { b'a' + digit - 10 })
        })
        .collect()
}

/// Copies the core just built, and its symbols, into the report's folder:
/// where its scenarios start it from, and where their crash dumps are read
/// with.
fn set_aside(build: Build, out: &Path) -> Result<PathBuf> {
    let to = out.join("builds").join(build.name().replace(' ', "-"));
    std::fs::create_dir_all(&to).map_err(|e| format!("cannot make {}: {e}", to.display()))?;
    for file in ["eu4_harmony_core.dll", "eu4_harmony_core.pdb"] {
        let from = crate::built().join(file);
        std::fs::copy(&from, to.join(file)).map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))?;
    }
    Ok(to)
}

/// Plays one scenario in a test game of its own, claimed as `owner` for it
/// and released after, with the core of `build`.
fn play_claimed(
    scenario: &dyn Scenario,
    parameters: &Parameters,
    owner: &str,
    build: &std::result::Result<PathBuf, String>,
    game_dir: &Path,
    out: &Path,
) -> Value {
    let build = match (build, interrupted()) {
        (Err(e), _) => return not_played(scenario, parameters, e),
        (Ok(_), true) => return not_played(scenario, parameters, "the run was interrupted"),
        (Ok(build), false) => build,
    };
    let run = match claim::claim_for_run(owner, &format!("scenario {}", scenario.name()), game_dir) {
        Ok(run) => run,
        Err(e) => return not_played(scenario, parameters, &format!("no test game for it: {e}")),
    };
    let played = claim::play_within(&run, &interrupted, || {
        let mut game = Game::new(owner, game_dir)?;
        game.build(build.clone());
        Ok(play(scenario, parameters, &mut game, owner, build, out))
    });
    played.unwrap_or_else(|e| not_played(scenario, parameters, &e))
}

/// Lists the scenarios.
fn list() {
    println!("{USAGE}\n\nscenarios:");
    for s in all() {
        let defaults = Parameters::of(s, &[]).unwrap_or_default();
        println!("  {} ({} build, about {} min): {}", s.name(), s.build(&defaults).name(), s.minutes(), s.about());
        for p in s.parameters() {
            println!("      --{}.{} {}: {}", s.name(), p.name, p.default, p.about);
        }
    }
    if !SETS.is_empty() {
        println!(
            "
sets:"
        );
    }
    for set in SETS {
        let scenarios = expanded(&[set.name.to_string()]);
        let minutes: u32 = scenarios.iter().filter_map(|n| find(n)).map(|s| s.minutes()).sum();
        println!("  {} (about {} min one after another, {} at a time): {}; plays {}", set.name, minutes, set.jobs, set.about, set.scenarios.join(", "));
    }
}

/// The builds the scenarios need, in the order they are first named.
fn builds(playing: &[(&dyn Scenario, Parameters)]) -> Vec<Build> {
    let mut builds = Vec::new();
    for (s, p) in playing {
        if !builds.contains(&s.build(p)) {
            builds.push(s.build(p));
        }
    }
    builds
}

/// Plays one scenario; its entry in the report.
fn play(scenario: &dyn Scenario, parameters: &Parameters, game: &mut Game, owner: &str, build: &Path, out: &Path) -> Value {
    println!("playing {}", scenario.name());
    let began = Instant::now();
    let user = claim::user_dir(owner).map(UserFolder::new);
    let crashes = user.as_ref().ok().and_then(|user| user.crash_folders().ok()).unwrap_or_default();
    let logs_before = user.as_ref().ok().map(logs::Mark::of);
    let mut record = json!({});
    game.loads(scenario.saves(parameters));
    game.options(scenario.options(parameters));
    game.mods(scenario.mods(parameters));
    game.next_scenario();
    // The games that run beside it, which take some of the computer it
    // measures: one that plays alone waits for other sessions' to end, a
    // while.
    let others = || eu4_harmony_client::game::processes().unwrap_or_default().len().saturating_sub(usize::from(game.runs()));
    if scenario.alone() {
        let waiting = Instant::now();
        let mut said = false;
        while alone_waits(others(), waiting.elapsed()) {
            if !said {
                println!("{} waits up to {} min for the {} other game(s) running to end", scenario.name(), ALONE_WAIT.as_secs() / 60, others());
                said = true;
            }
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    let beside = others();
    // A game that runs is brought to its main menu; one that does not is
    // started by the scenario's first step, which a scenario that times the
    // start takes itself.
    let at_main_menu = if game.runs() { game.at_main_menu() } else { Ok(()) };
    let ended = at_main_menu.and_then(|()| scenario.play(game, parameters, &mut record)).and_then(|()| match (&user, &logs_before) {
        // The framework's logs since the scenario began hold no panic and
        // no fatal error.
        (Ok(user), Some(mark)) => match mark.troubles(user).first() {
            Some(line) => Err(Stop::Failed(format!("the framework logged: {line}"))),
            None => Ok(()),
        },
        _ => Ok(()),
    });
    let (outcome, why) = match &ended {
        Ok(()) => ("passed", None),
        Err(Stop::Failed(why)) => ("failed", Some(why.clone())),
        Err(Stop::Crashed(why)) => ("crashed", Some(why.clone())),
        Err(Stop::Hung(why)) => ("hung", Some(why.clone())),
        Err(Stop::Interrupted) => ("unfinished", Some("the run was interrupted".into())),
    };
    let mut entry = entry(scenario, parameters, outcome, why.as_deref());
    entry["minutes"] = json!((began.elapsed().as_secs_f64() / 6.0).round() / 10.0);
    entry["games_beside"] = json!(beside);
    entry["alone"] = json!(scenario.alone());
    match ended {
        Err(Stop::Crashed(_)) => {
            // A game in its crash handler ends on its own once its dump is
            // written, with the exit code that says how it crashed.
            game.wait_gone(CRASH_HANDLER);
            entry["crash"] = crash(owner, &crashes, game.ended(), build, &out.join(format!("{}-crash.log", scenario.name())));
            game.end();
        }
        Err(Stop::Hung(_)) => game.end(),
        _ => {}
    }
    entry["record"] = record;
    println!("{}: {outcome}{}", scenario.name(), why.map_or(String::new(), |w| format!(": {w}")));
    entry
}

fn entry(scenario: &dyn Scenario, parameters: &Parameters, outcome: &str, why: Option<&str>) -> Value {
    json!({ "name": scenario.name(), "about": scenario.about(), "build": scenario.build(parameters).name(), "parameters": parameters.json(), "outcome": outcome, "ended": why })
}

fn not_played(scenario: &dyn Scenario, parameters: &Parameters, why: &str) -> Value {
    entry(scenario, parameters, "not played", Some(why))
}

/// How `owner`'s test game ended, as the claim followed it, and what its
/// newest crash folder, of those not `known` when the scenario began, says:
/// its exception, and the stack its dump gives, written to `log` too.
fn crash(owner: &str, known: &[String], ended: Option<GameEnd>, build: &Path, log: &Path) -> Value {
    let ended = ended.map_or(Value::Null, |end| json!({ "exit_code": format!("0x{:x}", end.code), "said": end.describe() }));
    let Ok(user) = claim::user_dir(owner) else { return json!({ "folder": null, "ended": ended }) };
    let Some(folder) = UserFolder::new(user).new_crash_folders(known).pop() else { return json!({ "folder": null, "ended": ended }) };
    let exception: Vec<String> = std::fs::read_to_string(folder.join("exception.txt")).unwrap_or_default().lines().take(12).map(String::from).collect();
    let dump = folder.join("minidump.dmp");
    let mut stack = Vec::new();
    if dump.is_file() {
        let (dump_text, log_text) = (dump.display().to_string(), log.display().to_string());
        let build = build.to_path_buf();
        let reading = std::thread::spawn(move || crate::crash_dump::read(&dump_text, &log_text, &[build], "C:\\symbols", &[".ecxr".into(), "kn 20".into()]));
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
    json!({ "folder": folder.display().to_string(), "ended": ended, "exception": exception, "stack": stack, "log": log.display().to_string() })
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
        if s["alone"] == true && s["games_beside"].as_u64().is_some_and(|n| n > 0) {
            md.push(format!(
                "It measures what the game costs the computer, and {} other game(s) ran beside it after waiting {} min for them: its numbers are not comparable.",
                text(&s["games_beside"]),
                ALONE_WAIT.as_secs() / 60
            ));
        }
        if let Some(beside) = s["games_beside"].as_u64() {
            md.push(format!("Other games running beside it as it began: {beside}."));
        }
        let crash = &s["crash"];
        if crash.is_object() {
            md.extend([String::new(), "### The crash".into(), String::new()]);
            match crash["ended"]["said"].as_str() {
                Some(said) => md.push(format!("The claim followed the game to its end: {said}.")),
                None => md.push("The claim saw no end of the game's.".into()),
            }
            md.push(String::new());
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
        let asked = parse(&words("perf leak --leak.rounds 1 --out D:\\r")).unwrap();
        assert_eq!(asked.names, ["perf", "leak"]);
        assert!(asked.out == Some(PathBuf::from("D:\\r")));
        assert_eq!(asked.parameters, [("leak".to_string(), "rounds".to_string(), "1".to_string())]);
        assert!(parse(&words("--rounds 1")).unwrap_err().contains("unknown option --rounds"));
        assert!(parse(&words("perf --out")).unwrap_err().contains("--out needs a value"));
        assert_eq!(parse(&words("whole --jobs 3")).unwrap().jobs, Some(3));
        assert!(parse(&words("whole --jobs 0")).unwrap_err().contains("a whole number above 0"));
    }

    /// A run names its owner, and each scenario's test game has a name of
    /// its own after it.
    #[test]
    fn a_run_names_its_owner_and_each_test_game_its_own() {
        assert!(claim::named_owner(None).unwrap_err().contains("--owner <name> is needed"));
        assert!(claim::named_owner(Some("a b")).is_err());
        assert!(claim::named_owner(Some("")).is_err());
        assert_eq!(claim::named_owner(Some("lucid-1.a_b")).unwrap(), "lucid-1.a_b");
        let (a, b) = (suffix(), suffix());
        assert!(a.len() == 6 && a.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()), "{a}");
        assert_ne!(a, b);
    }

    #[test]
    fn whole_plays_the_long_game_in_stretches_and_release_plays_it_through() {
        assert_eq!(SETS.iter().find(|set| set.name == "whole").map(|set| set.jobs), Some(claim::AT_ONCE));
        let alone: Vec<&str> = all().iter().filter(|s| s.alone()).map(|s| s.name()).collect();
        assert_eq!(alone, ["perf"]);
        let whole = expanded(&words("whole"));
        assert!(whole.iter().any(|s| s == "long-1444") && !whole.iter().any(|s| s == "long"), "{whole:?}");
        assert!(whole.iter().all(|s| find(s).is_some()), "{whole:?}");
        let release = expanded(&words("release"));
        assert_eq!(release.len(), whole.len() + 1);
        assert_eq!(release.last().map(String::as_str), Some("long"));
    }

    #[test]
    fn a_scenario_that_plays_alone_waits_a_while_for_other_games() {
        assert!(!alone_waits(0, Duration::ZERO));
        assert!(alone_waits(2, Duration::from_secs(60)));
        assert!(!alone_waits(2, ALONE_WAIT));
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
              "parameters": {}, "record": {}, "crash": { "folder": "C:\\crashes\\eu4_1", "ended": { "exit_code": "0xc0000005", "said": "eu4.exe 7 had ended by 03:00:00: an access violation (exit code 0xc0000005)" }, "exception": ["EXCEPTION_ACCESS_VIOLATION"], "stack": [] } },
        ] });
        let md = markdown(&report);
        assert!(md.starts_with("# Scenarios, 2026-11-05 12:00\n\n| Scenario | Build | Outcome | Ended at | Minutes |\n"), "{md}");
        assert!(md.contains("| perf | player | passed |  | 4.5 |\n| perf | player | crashed | the game ended | 1.0 |"), "{md}");
        assert!(md.contains("## perf\n\nWhat one run costs.\nParameters: country MNG.\n\n| Figure | Value |"), "{md}");
        assert!(
            md.contains("### The crash\n\nThe claim followed the game to its end: eu4.exe 7 had ended by 03:00:00: an access violation (exit code 0xc0000005).\n\nThe game's crash folder: `C:\\crashes\\eu4_1`.\n\nIts exception:\n\n```text\nEXCEPTION_ACCESS_VIOLATION\n```"),
            "{md}"
        );
        assert!(md.contains("Its dump was not read in time."), "{md}");
    }
}
