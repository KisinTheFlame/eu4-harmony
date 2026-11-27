//! What a scenario is handed: the run's test game, asked through remote
//! control on its pipe with the request and answer types of `remote/`, over
//! one connection while the game process lasts. Every step has a time; the
//! game ending in a step, a step taking longer, the run interrupted and a
//! check that does not hold each end the scenario, as a [`Stop`].
//!
//! The game's process is reached only through the run's claim
//! ([`claim::RunGame`]), which follows each game to its exit code, and remote
//! control through [`Transport`] (a named pipe in production, or a scripted
//! responder in unit tests).
use crate::claim::{self, GameEnd, RunGame, Starting};
use eu4_harmony_client::pipe::{self, Failure, NoMainMenu};
use eu4_harmony_folders::UserFolder;
use eu4_harmony_remote::{
    console::ConsoleRequest,
    input::{InputRequest, Keys},
    menu::{Menu, MenuRequest, Place, Save},
    questions::{Question, QuestionRequest},
    session::{Session, SessionRequest},
    time::{Advanced, Stop as AdvanceStop, Time, TimeRequest},
    Code, Date, Error, GroupRequest,
};
use serde::de::DeserializeOwned;
use std::{
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Remote-control transport seam for [`Game`]: production connects to the
/// game's named pipe ([`PipeTransport`]), while tests inject an in-process
/// scripted responder without spawning OS pipe threads or sleeping.
pub trait Transport {
    /// Sends one JSON request line and returns the raw JSON response line.
    fn ask_line(&mut self, request: &str, deadline: Instant) -> Result<String, Failure>;

    /// Waits until the game answers that its main menu is up, invoking
    /// `going_on` between probes so crashes or interrupts abort the wait.
    fn wait_main_menu(&mut self, deadline: Instant, going_on: &mut dyn FnMut() -> Played<()>) -> Result<(), NoMainMenu<Stop>>;

    /// Drops any open connection (for example before ending or restarting).
    fn disconnect(&mut self);
}

/// Named-pipe implementation of [`Transport`] backed by `eu4_harmony_client::pipe`.
struct PipeTransport {
    name: String,
    exe: String,
    client: Option<pipe::Client>,
}

impl PipeTransport {
    fn new(name: impl Into<String>, exe: impl Into<String>) -> Self {
        Self { name: name.into(), exe: exe.into(), client: None }
    }
}

impl Transport for PipeTransport {
    fn ask_line(&mut self, request: &str, deadline: Instant) -> Result<String, Failure> {
        let mut client = match self.client.take() {
            Some(client) => client,
            None => pipe::connect(&self.name, &self.exe, deadline)?,
        };
        let answer = client.ask(request, Some(deadline));
        if answer.is_ok() {
            self.client = Some(client);
        }
        answer
    }

    fn wait_main_menu(&mut self, deadline: Instant, going_on: &mut dyn FnMut() -> Played<()>) -> Result<(), NoMainMenu<Stop>> {
        self.client = None;
        let client = pipe::main_menu(&self.name, &self.exe, deadline, going_on)?;
        self.client = Some(client);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.client = None;
    }
}

/// How long a step may take unless the scenario says otherwise: an answer,
/// which a load or the days run take the longest to give.
pub const STEP: Duration = Duration::from_secs(600);
/// How long the game may take to reach its main menu, from a start or a
/// return.
pub const MAIN_MENU: Duration = Duration::from_secs(300);
/// How long the autosave may take to be written once the console asked
/// for it.
const SAVE_WRITTEN: Duration = Duration::from_secs(180);
/// How many times time may stop on its way to a date, for questions and
/// pauses, before running it fails.
const RUN_STOPS: u32 = 500;
/// The autosave's name, as `menu.saves` gives it.
const AUTOSAVE: &str = "autosave";
/// How long a game process may be missing before it counts as ended: a
/// return to the main menu ends the process, and the framework starts the
/// next.
const BETWEEN_PROCESSES: Duration = Duration::from_secs(15);
/// How often the game's process is looked at while it may be missing.
const LOOK: Duration = Duration::from_millis(500);

/// Why a scenario ended before its end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// A check did not hold, or the game refused a request.
    Failed(String),
    /// The game ended.
    Crashed(String),
    /// A step took longer than its time.
    Hung(String),
    /// The run was interrupted, with Ctrl-C.
    Interrupted,
}

/// A scenario's step: what it gave, or why the scenario ends.
pub type Played<T> = Result<T, Stop>;

pub struct Game {
    run: Box<dyn RunGame>,
    /// The test game's user folder; none in the tests of this module.
    user: Option<UserFolder>,
    /// Remote-control transport (named pipe in production, scripted in tests).
    transport: Box<dyn Transport>,
    /// The saves the scenario loads, copied into the test game's user folder
    /// at each start.
    saves: Vec<String>,
    /// The developer tools' options each start gives the core, in place of
    /// the checkout's `dev.toml`.
    options: Option<String>,
    /// The test mods each start puts in the test game's playset.
    mods: Vec<PathBuf>,
    /// The build each start copies the core of; None for this checkout's
    /// last build.
    build: Option<PathBuf>,
    /// The saves, options and test mods the running game was started with.
    started_with: Option<(Vec<String>, Option<String>, Vec<PathBuf>)>,
    /// The game ran since the scenario began: its process missing then is an
    /// end, not a game to start.
    running: bool,
    /// How many of the claim's games had ended when the scenario began.
    ends_before: usize,
    /// Peak count of other `eu4.exe` processes observed while this scenario's
    /// game ran.
    beside: usize,
    /// How long a step may take, and the wait for the main menu.
    step: Duration,
    menu_wait: Duration,
}

impl Game {
    pub fn new(owner: &str, game_dir: &Path) -> crate::Result<Self> {
        let mut game = Self::with(Box::new(claim::run_game(owner, game_dir, super::interrupted)), &claim::pipe_of(owner)?, pipe::GAME);
        game.user = Some(UserFolder::new(claim::user_dir(owner)?));
        Ok(game)
    }

    /// The test game `run` reaches, asked down the pipe `name` served by
    /// `exe`.
    fn with(run: Box<dyn RunGame>, name: &str, exe: &str) -> Self {
        Self::with_transport(run, Box::new(PipeTransport::new(name, exe)))
    }

    /// The test game `run` reaches, asked through `transport`.
    pub fn with_transport(run: Box<dyn RunGame>, transport: Box<dyn Transport>) -> Self {
        Self {
            run,
            user: None,
            transport,
            saves: Vec::new(),
            options: None,
            mods: Vec::new(),
            build: None,
            started_with: None,
            running: false,
            ends_before: 0,
            beside: 0,
            step: STEP,
            menu_wait: MAIN_MENU,
        }
    }

    /// The build each start copies the core of, in place of this checkout's
    /// last build.
    pub fn build(&mut self, build: PathBuf) {
        self.build = Some(build);
    }

    /// The folder holding the build's DLLs and PDBs, if set aside for this run.
    pub fn build_dir(&self) -> Option<&Path> {
        self.build.as_deref()
    }

    /// The saves the next scenario loads.
    pub fn loads(&mut self, saves: Vec<String>) {
        self.saves = saves;
    }

    /// The developer tools' options the next start gives the core, in place
    /// of the checkout's `dev.toml`.
    pub fn options(&mut self, options: Option<String>) {
        self.options = options;
    }

    /// The test mods the next start puts in the test game's playset.
    pub fn mods(&mut self, mods: Vec<PathBuf>) {
        self.mods = mods;
    }

    /// Whether the test game runs.
    pub fn runs(&self) -> bool {
        self.run.game().is_some()
    }

    /// How many other `eu4.exe` processes currently run beside this claim's own game.
    pub fn others_running(&self) -> usize {
        self.run.others_running()
    }

    /// How many other `eu4.exe` processes were observed running beside this
    /// scenario's game once it started.
    pub fn beside(&self) -> usize {
        self.beside.max(self.run.others_running())
    }

    /// The test game's process.
    pub fn process(&self) -> Played<u32> {
        self.run.game().ok_or_else(|| Stop::Crashed("the game is not running".into()))
    }

    /// Ends the test game, if it runs.
    pub fn end(&mut self) {
        self.transport.disconnect();
        let _ = self.run.stop();
    }

    /// How the last of the claim's games that ended since the scenario
    /// began ended, the last that crashed first: its exit code is the one
    /// sign of a crash that bypasses the game's crash handler.
    pub fn ended(&self) -> Option<GameEnd> {
        let ends = self.run.ends();
        let since = ends.get(self.ends_before..).unwrap_or_default();
        since.iter().rev().find(|end| end.crashed()).or(since.last()).cloned()
    }

    /// Waits up to `limit` for the game to end on its own.
    pub fn wait_gone(&mut self, limit: Duration) {
        self.transport.disconnect();
        let mut waited = Duration::ZERO;
        while waited < limit && self.runs() {
            self.run.wait(LOOK);
            waited += LOOK;
        }
    }

    /// Asks `request` and reads its answer, within [`STEP`].
    pub fn ask<T: DeserializeOwned>(&mut self, request: &impl GroupRequest) -> Played<T> {
        self.ask_within(request, self.step)
    }

    /// Asks `request` and reads its answer, within `step`; the game is
    /// started first if the scenario has not had it yet.
    pub fn ask_within<T: DeserializeOwned>(&mut self, request: &impl GroupRequest, step: Duration) -> Played<T> {
        let name = request.full_name();
        match self.exchange(request, step)? {
            Ok(answer) => answer.map_err(|refused| Stop::Failed(format!("{name}: {refused}"))),
            Err(failure) => Err(self.why(&name, &failure, step)),
        }
    }

    /// Asks `request` and reads its answer within `step`, the game's refusal
    /// as it came: a refusal is an answer here, not the scenario's end.
    pub fn try_ask<T: DeserializeOwned>(&mut self, request: &impl GroupRequest, step: Duration) -> Played<Result<T, Error>> {
        let name = request.full_name();
        match self.exchange(request, step)? {
            Ok(answer) => Ok(answer),
            Err(failure) => Err(self.why(&name, &failure, step)),
        }
    }

    /// Sends `request` and reads its answer, or why none came.
    fn exchange<T: DeserializeOwned>(&mut self, request: &impl GroupRequest, step: Duration) -> Played<Result<Result<T, Error>, Failure>> {
        self.not_interrupted()?;
        self.started()?;
        let deadline = Instant::now() + step;
        let line = serde_json::to_string(&eu4_harmony_remote::request_json(request)).expect("a request is JSON");
        let answer_line = match self.transport.ask_line(&line, deadline) {
            Ok(line) => line,
            Err(failure) => return Ok(Err(failure)),
        };
        let unreadable = |why: String| Failure::Io(io::Error::new(io::ErrorKind::InvalidData, format!("the answer to {}: {why}", request.full_name())));
        let decoded = match pipe::answer_of(&answer_line).map_err(unreadable) {
            Ok(Ok(result)) => serde_json::from_value(result).map(Ok).map_err(|e| unreadable(e.to_string())),
            Ok(Err(error)) => serde_json::from_value(error).map(Err).map_err(|e| unreadable(e.to_string())),
            Err(failure) => Err(failure),
        };
        if decoded.is_ok() {
            self.beside = self.beside.max(self.run.others_running());
        }
        Ok(decoded)
    }

    /// Lets `days` days run at speed 5.
    pub fn advance(&mut self, days: u32) -> Played<Advanced> {
        self.ask(&TimeRequest::Advance { days, speed: Some(5) })
    }

    /// Lets `days` days run at speed 5, as the player would: each question
    /// that stops the time answered with its first option, a pause let go
    /// on, and a window that keeps pausing it, such as the game menu a load
    /// may leave open, closed with Escape; the date then. Another session or
    /// the main menu coming instead fails it.
    pub fn run(&mut self, days: u32) -> Played<Date> {
        let time: Time = self.ask(&TimeRequest::Get {})?;
        let goal = time.date.0 + days as i32 * 24;
        let mut stuck = 0;
        for _ in 0..RUN_STOPS {
            let before = time_now(self)?;
            let left = (goal - before.0).div_euclid(24);
            if left <= 0 {
                return Ok(before);
            }
            let advanced = self.advance(left as u32)?;
            match advanced.stopped {
                AdvanceStop::Date => {}
                AdvanceStop::Paused if advanced.date != before => stuck = 0,
                AdvanceStop::Paused => {
                    stuck += 1;
                    if stuck % 2 == 0 {
                        let keys = Keys::parse("escape").map_err(Stop::Failed)?;
                        let () = self.ask(&InputRequest::Key { keys, repeat: None })?;
                    }
                }
                AdvanceStop::Question => {
                    let questions: Vec<Question> = self.ask(&QuestionRequest::List {})?;
                    for q in questions {
                        if let Some(first) = q.options.first() {
                            let () = self.ask(&QuestionRequest::Answer { question: q.id, option: first.option })?;
                        }
                    }
                }
                AdvanceStop::Session | AdvanceStop::MainMenu => {
                    return Err(Stop::Failed(format!("time stopped on {} for another place: {}", advanced.date, advanced.reason)))
                }
            }
        }
        Err(Stop::Failed(format!("time stopped {RUN_STOPS} times before {days} days had run")))
    }

    /// The session, read again when the one the request was sent in ended
    /// before it ran (`session_changed`), as after the console's `observe`.
    pub fn session(&mut self) -> Played<Session> {
        for _ in 0..5 {
            match self.try_ask::<Session>(&SessionRequest::Get {}, self.step)? {
                Ok(session) => return Ok(session),
                Err(e) if e.code == Code::SessionChanged => continue,
                Err(e) => return Err(Stop::Failed(format!("session.get: {e}"))),
            }
        }
        Err(Stop::Failed("session.get: the session changed five times over".into()))
    }

    /// Puts the game in observe mode with no country in view. The game puts
    /// a country in view as it loads a save of observe mode, and a save made
    /// with one in view names that country its player, which a load of it
    /// then plays.
    pub fn observe_with_no_country(&mut self) -> Played<Session> {
        let mut session = self.session()?;
        if !session.observing {
            self.console("observe")?;
            session = self.session()?;
        }
        if session.observing && session.player.is_some() {
            session = self.ask(&SessionRequest::LeaveView {})?;
        }
        Ok(session)
    }

    /// Runs `command` in the game console; what it printed.
    pub fn console(&mut self, command: &str) -> Played<String> {
        self.ask(&ConsoleRequest::Run { command: command.into() })
    }

    /// Writes the game's autosave with the console's `autosave`, waits until
    /// its file is written, which a save written in the background finishes
    /// after the command, and loads it in the game; the session it began.
    pub fn autosave_and_load(&mut self) -> Played<Session> {
        let asked = seconds_now().saturating_sub(2);
        self.console("autosave")?;
        let deadline = Instant::now() + SAVE_WRITTEN;
        let mut last = None;
        loop {
            let saves: Vec<Save> = self.ask(&MenuRequest::Saves {})?;
            let now = saves.iter().find(|s| s.name == AUTOSAVE && s.modified >= asked).map(|s| s.bytes);
            if now.is_some() && now == last {
                break;
            }
            if Instant::now() > deadline {
                return Err(Stop::Hung(format!("the autosave was not written in {} s", SAVE_WRITTEN.as_secs())));
            }
            last = now;
            self.run.wait(Duration::from_secs(1));
        }
        self.ask(&MenuRequest::Load { save: AUTOSAVE.into() })
    }

    /// The test game's user folder.
    pub fn user(&self) -> Played<UserFolder> {
        self.user.clone().ok_or_else(|| Stop::Failed("the test game has no user folder".into()))
    }

    /// Returns to the main menu, and waits for it.
    pub fn return_to_menu(&mut self) -> Played<()> {
        let name = MenuRequest::Return {}.full_name();
        match self.exchange::<Menu>(&MenuRequest::Return {}, self.step)? {
            Ok(Ok(_)) => {}
            Ok(Err(refused)) => return Err(Stop::Failed(format!("{name}: {refused}"))),
            // A return that starts the game again ends this process, and the
            // pipe may break before its answer comes: with a game still
            // running, that is the return.
            Err(Failure::Broken) if !self.gone() => {}
            Err(failure) => return Err(self.why(&name, &failure, self.step)),
        }
        self.main_menu()
    }

    /// Waits for the main menu, in this process or the next one.
    pub fn main_menu(&mut self) -> Played<()> {
        self.started()?;
        self.transport.disconnect();
        let run = &self.run;
        let mut going_on = || {
            if super::interrupted() {
                return Err(Stop::Interrupted);
            }
            let mut waited = Duration::ZERO;
            while waited < BETWEEN_PROCESSES {
                if run.game().is_some() {
                    return Ok(());
                }
                run.wait(LOOK);
                waited += LOOK;
            }
            Err(Stop::Crashed("the game ended on its way to the main menu".into()))
        };
        match self.transport.wait_main_menu(Instant::now() + self.menu_wait, &mut going_on) {
            Ok(()) => Ok(()),
            Err(NoMainMenu::Stopped(stop)) => Err(stop),
            Err(NoMainMenu::TimedOut) => Err(Stop::Hung(format!("the main menu did not come up in {} s", self.menu_wait.as_secs()))),
        }
    }

    /// The main menu, from wherever the game is: started if it is not
    /// running, returned to from a game.
    pub fn at_main_menu(&mut self) -> Played<()> {
        let menu: Menu = self.ask(&MenuRequest::State {})?;
        match menu.place {
            Place::MainMenu => Ok(()),
            Place::Game => self.return_to_menu(),
            _ => self.main_menu(),
        }
    }

    /// Starts the game afresh, ending it first if it runs; the seconds to its
    /// main menu.
    pub fn restart(&mut self) -> Played<f64> {
        self.not_interrupted()?;
        if self.runs() {
            self.end();
        }
        self.start()
    }

    /// A check: the scenario fails with `what` unless `holds`.
    pub fn check(&self, holds: bool, what: impl Into<String>) -> Played<()> {
        if holds {
            Ok(())
        } else {
            Err(Stop::Failed(what.into()))
        }
    }

    /// A new scenario begins, its saves, options and test mods set: the
    /// game, if it runs and was started with what it needs, is the one it
    /// has; one started with other options or test mods, or without its
    /// saves, is ended, for its first step to start it as it needs.
    pub fn next_scenario(&mut self) {
        let fits = self
            .started_with
            .as_ref()
            .is_some_and(|(saves, options, mods)| self.saves.iter().all(|s| saves.contains(s)) && *options == self.options && *mods == self.mods);
        if self.runs() && !fits {
            self.end();
        }
        self.running = self.runs();
        self.ends_before = self.run.ends().len();
        self.beside = if self.running { self.run.others_running() } else { 0 };
    }

    /// The game for the scenario: started unless it ran, an end when it ran
    /// and is gone.
    fn started(&mut self) -> Played<()> {
        if !self.running {
            if !self.runs() {
                self.start()?;
            }
            self.running = true;
            self.beside = self.beside.max(self.run.others_running());
        } else if self.gone() {
            return Err(Stop::Crashed("the game ended".into()));
        }
        Ok(())
    }

    fn start(&mut self) -> Played<f64> {
        self.transport.disconnect();
        let starting =
            Starting { saves: self.saves.clone(), options: self.options.clone(), mods: self.mods.clone(), build: self.build.clone(), ..Default::default() };
        self.started_with = Some((self.saves.clone(), self.options.clone(), self.mods.clone()));
        match self.run.start(&starting, self.menu_wait) {
            Ok(seconds) => {
                self.running = true;
                Ok(seconds)
            }
            Err(_) if super::interrupted() => Err(Stop::Interrupted),
            Err(e) if e.contains("the game ended") => Err(Stop::Crashed(format!("the start: {e}"))),
            Err(e) => Err(Stop::Hung(format!("the start: {e}"))),
        }
    }

    fn not_interrupted(&self) -> Played<()> {
        if super::interrupted() {
            return Err(Stop::Interrupted);
        }
        Ok(())
    }

    /// Whether no game process runs, and none comes in a few seconds.
    fn gone(&self) -> bool {
        let mut waited = Duration::ZERO;
        while waited < BETWEEN_PROCESSES {
            if self.runs() {
                return false;
            }
            self.run.wait(LOOK);
            waited += LOOK;
        }
        true
    }

    /// Why asking `name` failed.
    fn why(&self, name: &str, failure: &Failure, step: Duration) -> Stop {
        if let Err(stop) = self.not_interrupted() {
            return stop;
        }
        let ended = format!("the game ended while answering {name}");
        match failure {
            // Only a return to the main menu ends the process on purpose,
            // and `return_to_menu` takes that itself. An ended process may
            // stay in the process list for a while, so its list is no
            // sign here.
            Failure::Broken => Stop::Crashed(ended),
            _ if self.gone() => Stop::Crashed(ended),
            Failure::TimedOut | Failure::NotOpen => Stop::Hung(format!("{name} had no answer in {} s", step.as_secs_f64())),
            other => Stop::Failed(format!("{name}: {other}")),
        }
    }
}

/// Changes one top-level setting while keeping the rest of the claim's file,
/// creating the file when missing.
pub fn set_setting(path: &Path, key: &str, value: &str) -> Played<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Stop::Failed(format!("cannot read {}: {e}", path.display()))),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Stop::Failed(format!("cannot make {}: {e}", parent.display())))?;
    }
    std::fs::write(path, with_setting(&text, key, value)).map_err(|e| Stop::Failed(format!("cannot write {}: {e}", path.display())))
}

/// Replaces or appends `key=value` on a top-level `key=...` line.
pub fn with_setting(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut changed = String::new();
    for line in text.lines() {
        if line.split_once('=').is_some_and(|(name, _)| name.trim() == key) {
            changed.push_str(&format!("{key}={value}\n"));
            found = true;
        } else {
            changed.push_str(line);
            changed.push('\n');
        }
    }
    if !found {
        changed.push_str(&format!("{key}={value}\n"));
    }
    changed
}

/// Reads a `key=value` setting (at any indentation) from `path`, trimmed.
pub fn read_setting(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches('"').to_string())
    })
}

/// The whole days from `from` to `to`; a date counts hours.
pub fn days_between(from: Date, to: Date) -> i32 {
    (to.0 - from.0).div_euclid(24)
}

/// The game's date.
fn time_now(game: &mut Game) -> Played<Date> {
    game.ask::<Time>(&TimeRequest::Get {}).map(|t| t.date)
}

/// Seconds since 1970, as `menu.saves` gives a save's time.
fn seconds_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::recorded::{held, Did, Recorded, GAME};
    use eu4_harmony_client::pipe::tests::{answer, pipe_name, serve_one, this_program};
    use serde_json::{json, Value};
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    const SHORT: Duration = Duration::from_millis(300);
    const MAIN_MENU_ANSWER: &str = "{\"ok\": true, \"result\": {\"process\": 1, \"place\": \"main_menu\"}}\n";

    /// Owned `RunGame` wrapper around `Arc<Recorded>` so unit tests never leak
    /// allocations via `Box::leak`.
    struct OwnedRecorded {
        recorded: Arc<Recorded>,
        owner: String,
    }

    impl OwnedRecorded {
        fn new(recorded: Arc<Recorded>, owner: &str) -> Self {
            Self { recorded, owner: owner.into() }
        }
    }

    impl RunGame for OwnedRecorded {
        fn game(&self) -> Option<u32> {
            claim::recorded_run_game(&self.recorded, &self.owner).game()
        }

        fn start(&self, starting: &Starting, wait: Duration) -> crate::Result<f64> {
            claim::recorded_run_game(&self.recorded, &self.owner).start(starting, wait)
        }

        fn stop(&self) -> crate::Result<()> {
            claim::recorded_run_game(&self.recorded, &self.owner).stop()
        }

        fn ends(&self) -> Vec<GameEnd> {
            claim::recorded_run_game(&self.recorded, &self.owner).ends()
        }

        fn wait(&self, time: Duration) {
            claim::recorded_run_game(&self.recorded, &self.owner).wait(time)
        }

        fn others_running(&self) -> usize {
            claim::recorded_run_game(&self.recorded, &self.owner).others_running()
        }
    }

    type Responder = Box<dyn FnMut(&Value) -> Result<String, Failure>>;

    /// Scripted in-process [`Transport`] for deterministic, zero-sleep tests.
    struct ScriptedTransport {
        requests: Arc<Mutex<Vec<Value>>>,
        handler: Responder,
        menu_outcomes: VecDeque<Result<(), NoMainMenu<Stop>>>,
    }

    impl ScriptedTransport {
        fn new(handler: impl FnMut(&Value) -> Result<String, Failure> + 'static) -> (Self, Arc<Mutex<Vec<Value>>>) {
            let requests = Arc::new(Mutex::new(Vec::new()));
            (Self { requests: requests.clone(), handler: Box::new(handler), menu_outcomes: VecDeque::new() }, requests)
        }

        fn with_menu_outcomes(mut self, outcomes: impl IntoIterator<Item = Result<(), NoMainMenu<Stop>>>) -> Self {
            self.menu_outcomes.extend(outcomes);
            self
        }
    }

    impl Transport for ScriptedTransport {
        fn ask_line(&mut self, request: &str, _deadline: Instant) -> Result<String, Failure> {
            let parsed: Value = serde_json::from_str(request).expect("valid JSON request");
            self.requests.lock().unwrap().push(parsed.clone());
            (self.handler)(&parsed)
        }

        fn wait_main_menu(&mut self, _deadline: Instant, going_on: &mut dyn FnMut() -> Played<()>) -> Result<(), NoMainMenu<Stop>> {
            if let Err(stop) = going_on() {
                return Err(NoMainMenu::Stopped(stop));
            }
            self.menu_outcomes.pop_front().unwrap_or(Ok(()))
        }

        fn disconnect(&mut self) {}
    }

    fn ok_json(result: Value) -> Result<String, Failure> {
        Ok(json!({"ok": true, "result": result}).to_string())
    }

    fn err_json(code: &str, message: &str) -> Result<String, Failure> {
        Ok(json!({"ok": false, "error": {"code": code, "message": message}}).to_string())
    }

    fn recorded_state(running: bool) -> Arc<Recorded> {
        let mut claim = held("a");
        claim.game = running.then_some(GAME);
        let m = Arc::new(Recorded::new(claim::recorded::state(vec![claim])));
        if running {
            m.serving.lock().unwrap().push((claim::pipe_of("a").unwrap(), GAME.id));
        }
        m
    }

    fn scripted_game(running: bool, transport: ScriptedTransport) -> (Arc<Recorded>, Game) {
        let m = recorded_state(running);
        let mut game = Game::with_transport(Box::new(OwnedRecorded::new(m.clone(), "a")), Box::new(transport));
        (game.step, game.menu_wait) = (SHORT, Duration::from_secs(5));
        game.started_with = Some(Default::default());
        game.next_scenario();
        (m, game)
    }

    /// A game started without what the next scenario needs is ended, for
    /// that scenario's first step to start it with it.
    #[test]
    fn a_game_started_for_other_options_or_test_mods_is_ended_for_the_next_scenario() {
        let (transport, _) = ScriptedTransport::new(|_| ok_json(json!({})));
        let (m, mut game) = scripted_game(true, transport);
        game.next_scenario();
        assert!(!m.did().iter().any(|d| matches!(d, Did::End(_))), "the same start's game is kept");
        game.mods(vec![PathBuf::from("qa-steady")]);
        game.next_scenario();
        assert!(m.did().contains(&Did::End(GAME.id)));
    }

    #[test]
    fn a_game_that_ends_while_answering_has_crashed_and_its_exit_code_is_told() {
        let (transport, _) = ScriptedTransport::new(|_| Err(Failure::Broken));
        let (m, mut game) = scripted_game(true, transport);
        m.ended.lock().unwrap().push(GameEnd { game: GAME, seen: "03:12:41".into(), code: 0xc000_0374 });
        let stop = game.ask::<Menu>(&MenuRequest::State {}).unwrap_err();
        assert_eq!(stop, Stop::Crashed("the game ended while answering menu".into()));
        assert_eq!(game.ended().map(|end| end.code), Some(0xc000_0374), "the claim followed it to its end");
    }

    #[test]
    fn a_step_with_no_answer_in_its_time_while_the_game_runs_has_hung() {
        let (transport, _) = ScriptedTransport::new(|_| Err(Failure::TimedOut));
        let (_, mut game) = scripted_game(true, transport);
        let stop = game.ask::<Menu>(&MenuRequest::State {}).unwrap_err();
        assert!(matches!(&stop, Stop::Hung(why) if why.contains("had no answer")), "{stop:?}");
        assert_eq!(game.ended(), None);
    }

    #[test]
    fn a_refusal_fails_the_scenario() {
        let (transport, _) = ScriptedTransport::new(|_| err_json("rejected", "not now"));
        let (_, mut game) = scripted_game(true, transport);
        let stop = game.ask::<Menu>(&MenuRequest::State {}).unwrap_err();
        assert!(matches!(&stop, Stop::Failed(why) if why.contains("not now")), "{stop:?}");
    }

    #[test]
    fn a_game_that_does_not_run_is_started_within_the_claim() {
        let (transport, _) = ScriptedTransport::new(|_| Ok(MAIN_MENU_ANSWER.into()));
        let (m, mut game) = scripted_game(false, transport);
        let menu: Menu = game.ask(&MenuRequest::State {}).unwrap();
        assert_eq!(menu.place, Place::MainMenu);
        assert!(m.did().contains(&Did::Start));
        assert_eq!(game.process(), Ok(GAME.id), "the claim names the game it started");
    }

    #[test]
    fn a_game_gone_since_the_scenario_began_has_crashed() {
        let (transport, _) = ScriptedTransport::new(|_| Ok(MAIN_MENU_ANSWER.into()));
        let (m, mut game) = scripted_game(true, transport);
        m.serving.lock().unwrap().clear();
        assert_eq!(game.ask::<Menu>(&MenuRequest::State {}).unwrap_err(), Stop::Crashed("the game ended".into()));
        assert!(!m.did().contains(&Did::Start), "a game that ran is not started again");
    }

    /// A return that starts the game again breaks the pipe before its
    /// answer; the next process's pipe answers once its main menu is up.
    #[test]
    fn a_return_waits_for_the_main_menu_across_the_restart() {
        let (transport, requests) = ScriptedTransport::new(|req| {
            assert_eq!(req["capability"], "menu.return");
            Err(Failure::Broken)
        });
        let (_, mut game) = scripted_game(true, transport.with_menu_outcomes([Ok(())]));
        assert_eq!(game.return_to_menu(), Ok(()));
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_restart_ends_the_test_game_and_starts_it_within_the_claim() {
        let (transport, _) = ScriptedTransport::new(|_| ok_json(json!({})));
        let (m, mut game) = scripted_game(true, transport);
        assert_eq!(game.restart(), Ok(30.0));
        let ends_and_starts: Vec<Did> = m.did().into_iter().filter(|d| matches!(d, Did::End(_) | Did::Start)).collect();
        assert_eq!(ends_and_starts, [Did::End(GAME.id), Did::Start]);
    }

    /// Real named-pipe smoke test proving [`PipeTransport`] connects and
    /// exchanges JSON with an OS pipe server.
    #[test]
    fn pipe_transport_exchanges_json_over_named_pipe() {
        let m = recorded_state(true);
        let name = pipe_name("transport-smoke");
        let mut game = Game::with(Box::new(OwnedRecorded::new(m, "a")), &name, &this_program());
        (game.step, game.menu_wait) = (Duration::from_secs(2), Duration::from_secs(5));
        game.started_with = Some(Default::default());
        game.next_scenario();
        let server = serve_one(&name, |pipe, _| answer(pipe, MAIN_MENU_ANSWER));
        let menu: Menu = game.ask(&MenuRequest::State {}).unwrap();
        server.join().unwrap();
        assert_eq!(menu.place, Place::MainMenu);
    }

    #[test]
    fn run_answers_questions_closes_stuck_pauses_with_escape_and_reaches_goal_date() {
        let start = Date::from_ymd(1444, 11, 11).unwrap();
        let mut current = start;
        let mut step = 0usize;
        let (transport, requests) = ScriptedTransport::new(move |req| {
            match req["capability"].as_str().unwrap() {
                "time.get" => ok_json(json!({"date": current.to_string(), "paused": true, "speed": 5})),
                "time.advance" => {
                    step += 1;
                    match step {
                        // First advance stops on a question after 2 days.
                        1 => {
                            current = Date(current.0 + 2 * 24);
                            ok_json(
                                json!({"date": current.to_string(), "stopped": "question", "reason": "event", "process": 1, "signals": [], "last": 0, "missed": false}),
                            )
                        }
                        // Next two advances stay paused at the same date -> triggers Escape on the second.
                        2 | 3 => ok_json(
                            json!({"date": current.to_string(), "stopped": "paused", "reason": "window", "process": 1, "signals": [], "last": 0, "missed": false}),
                        ),
                        // Final advance reaches the 5-day goal.
                        _ => {
                            current = Date(start.0 + 5 * 24);
                            ok_json(
                                json!({"date": current.to_string(), "stopped": "date", "reason": "reached", "process": 1, "signals": [], "last": 0, "missed": false}),
                            )
                        }
                    }
                }
                "question.list" => ok_json(json!([{
                    "id": 7,
                    "event": "flavor_fra.1",
                    "title": "Test Event",
                    "description": "Body",
                    "deadline": "1445.1.1",
                    "options": [{"option": 0, "text": "OK"}]
                }])),
                "question.answer" => {
                    assert_eq!(req["question"], 7);
                    assert_eq!(req["option"], 0);
                    ok_json(Value::Null)
                }
                "input.key" => {
                    assert_eq!(req["keys"], "escape");
                    ok_json(Value::Null)
                }
                other => panic!("unexpected request: {other}"),
            }
        });
        let (_, mut game) = scripted_game(true, transport);
        let reached = game.run(5).unwrap();
        assert_eq!(reached, Date(start.0 + 5 * 24));
        let caps: Vec<String> = requests.lock().unwrap().iter().map(|r| r["capability"].as_str().unwrap().to_string()).collect();
        assert!(caps.contains(&"question.answer".to_string()));
        assert!(caps.contains(&"input.key".to_string()));
    }

    #[test]
    fn observe_with_no_country_retries_session_changed_and_leaves_view() {
        let mut session_calls = 0usize;
        let (transport, requests) = ScriptedTransport::new(move |req| match req["capability"].as_str().unwrap() {
            "session.get" => {
                session_calls += 1;
                match session_calls {
                    1 => ok_json(json!({"number": 1, "player": "FRA", "observing": false, "date": "1444.11.11"})),
                    2 => err_json("session_changed", "switching to observe"),
                    _ => ok_json(json!({"number": 2, "player": "FRA", "observing": true, "date": "1444.11.11"})),
                }
            }
            "console.run" => {
                assert_eq!(req["command"], "observe");
                ok_json(json!(""))
            }
            "session.leave_view" => ok_json(json!({"number": 2, "player": null, "observing": true, "date": "1444.11.11"})),
            other => panic!("unexpected request: {other}"),
        });
        let (_, mut game) = scripted_game(true, transport);
        let session = game.observe_with_no_country().unwrap();
        assert!(session.observing);
        assert_eq!(session.player, None);
        let caps: Vec<String> = requests.lock().unwrap().iter().map(|r| r["capability"].as_str().unwrap().to_string()).collect();
        assert_eq!(caps, ["session.get", "console.run", "session.get", "session.get", "session.leave_view"]);
    }

    #[test]
    fn autosave_and_load_waits_for_stable_size_and_loads_autosave() {
        let now = seconds_now();
        let mut saves_calls = 0usize;
        let (transport, requests) = ScriptedTransport::new(move |req| match req["capability"].as_str().unwrap() {
            "console.run" => {
                assert_eq!(req["command"], "autosave");
                ok_json(json!(""))
            }
            "menu.saves" => {
                saves_calls += 1;
                let bytes = if saves_calls == 1 { 1024 } else { 2048 };
                ok_json(json!([{
                    "name": "autosave",
                    "file": "autosave.eu4",
                    "bytes": bytes,
                    "modified": now,
                    "player": "FRA",
                    "date": "1444.11.11",
                    "missing_mods": []
                }]))
            }
            "menu.load" => {
                assert_eq!(req["save"], "autosave");
                ok_json(json!({"number": 2, "player": "FRA", "observing": false, "date": "1444.11.11"}))
            }
            other => panic!("unexpected request: {other}"),
        });
        let (_, mut game) = scripted_game(true, transport);
        let session = game.autosave_and_load().unwrap();
        assert_eq!(session.number, 2);
        let saves_queries = requests.lock().unwrap().iter().filter(|r| r["capability"] == "menu.saves").count();
        assert_eq!(saves_queries, 3, "grows on poll 2, stabilizes on poll 3");
    }

    #[test]
    fn sweep_scenario_plays_in_process_over_scripted_transport() {
        use crate::scenario::{Parameters, Scenario};
        let mut in_game = false;
        let (transport, _) = ScriptedTransport::new(move |req| match req["capability"].as_str().unwrap() {
            "menu" => ok_json(json!({"process": 1, "place": if in_game { "game" } else { "main_menu" }})),
            "menu.start" => {
                in_game = true;
                ok_json(json!({"number": 1, "player": "FRA", "observing": false, "date": "1444.11.11"}))
            }
            "menu.return" => {
                in_game = false;
                ok_json(json!({"process": 1, "place": "main_menu"}))
            }
            "time.get" => {
                if !in_game {
                    err_json("not_in_game", "not in a game")
                } else {
                    ok_json(json!({"date": "1444.11.11", "paused": true, "speed": 2}))
                }
            }
            "time.pause" => ok_json(Value::Null),
            "session.get" => ok_json(json!({"number": 1, "player": "FRA", "observing": false, "date": "1444.11.11"})),
            "question.list" | "province.list" | "province.neighbors" | "army.list" => ok_json(json!([])),
            other => panic!("unexpected request in minimal sweep: {other}"),
        });
        let (_, mut game) = scripted_game(true, transport);
        let sweep = super::super::sweep::Sweep;
        let params =
            Parameters::of(&sweep, &[("sweep".into(), "places".into(), "main menu,paused".into()), ("sweep".into(), "only".into(), "time.get".into())])
                .unwrap();
        let mut record = json!({});
        sweep.play(&mut game, &params, &mut record).unwrap();
        assert_eq!(record["findings"], json!([]), "minimal sweep produced unexpected findings: {record}");
    }
}
