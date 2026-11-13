//! What a scenario is handed: the run's test game, asked through remote
//! control on its pipe with the request and answer types of `remote/`, over
//! one connection while the game process lasts. Every step has a time; the
//! game ending in a step, a step taking longer, the run interrupted and a
//! check that does not hold each end the scenario, as a [`Stop`].
//!
//! The game's process is reached only through the run's claim
//! ([`claim::RunGame`]), which follows each game to its exit code, and the
//! pipe by its name; so how a scenario ends is tested on the claim's
//! recorded computer, with the client's own test server for the pipe.
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
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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
    /// The test game's pipe, and the executable trusted to serve it.
    pipe: (String, String),
    client: Option<pipe::Client>,
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
        Self {
            run,
            user: None,
            pipe: (name.into(), exe.into()),
            client: None,
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
        self.client = None;
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
        self.client = None;
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
        let mut client = match self.client.take() {
            Some(client) => client,
            None => match pipe::connect(&self.pipe.0, &self.pipe.1, deadline) {
                Ok(client) => client,
                Err(failure) => return Ok(Err(failure)),
            },
        };
        let answer = client.ask_for(request, Some(deadline));
        if answer.is_ok() {
            self.client = Some(client);
            self.beside = self.beside.max(self.run.others_running());
        }
        Ok(answer)
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
            std::thread::sleep(Duration::from_secs(1));
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
        self.client = None;
        let going_on = || {
            self.not_interrupted()?;
            match self.gone() {
                true => Err(Stop::Crashed("the game ended on its way to the main menu".into())),
                false => Ok(()),
            }
        };
        match pipe::main_menu(&self.pipe.0, &self.pipe.1, Instant::now() + self.menu_wait, going_on) {
            Ok(client) => {
                self.client = Some(client);
                Ok(())
            }
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
        self.client = None;
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
    use std::thread;

    const SHORT: Duration = Duration::from_millis(300);
    const MAIN_MENU_ANSWER: &str = "{\"ok\": true, \"result\": {\"process\": 1, \"place\": \"main_menu\"}}\n";

    /// `owner` a's claim on a recorded computer where its test game runs or
    /// not, and the game of a's run, asked down the test's own pipe.
    fn game_on(test: &str, running: bool) -> (&'static Recorded, Game, String) {
        let mut claim = held("a");
        claim.game = running.then_some(GAME);
        let m: &'static Recorded = Box::leak(Box::new(Recorded::new(claim::recorded::state(vec![claim]))));
        if running {
            m.serving.lock().unwrap().push((claim::pipe_of("a").unwrap(), GAME.id));
        }
        let name = pipe_name(test);
        let mut game = Game::with(Box::new(claim::recorded_run_game(m, "a")), &name, &this_program());
        (game.step, game.menu_wait) = (SHORT, Duration::from_secs(5));
        game.started_with = Some(Default::default());
        game.next_scenario();
        (m, game, name)
    }

    /// A game started without what the next scenario needs is ended, for
    /// that scenario's first step to start it with it.
    #[test]
    fn a_game_started_for_other_options_or_test_mods_is_ended_for_the_next_scenario() {
        let (m, mut game, _) = game_on("other-start", true);
        game.next_scenario();
        assert!(!m.did().iter().any(|d| matches!(d, Did::End(_))), "the same start's game is kept");
        game.mods(vec![PathBuf::from("qa-steady")]);
        game.next_scenario();
        assert!(m.did().contains(&Did::End(GAME.id)));
    }

    #[test]
    fn a_game_that_ends_while_answering_has_crashed_and_its_exit_code_is_told() {
        let (m, mut game, name) = game_on("crashed", true);
        let server = serve_one(&name, |_, _| {});
        m.ended.lock().unwrap().push(GameEnd { game: GAME, seen: "03:12:41".into(), code: 0xc000_0374 });
        let stop = game.ask::<Menu>(&MenuRequest::State {}).unwrap_err();
        server.join().unwrap();
        assert_eq!(stop, Stop::Crashed("the game ended while answering menu".into()));
        assert_eq!(game.ended().map(|end| end.code), Some(0xc000_0374), "the claim followed it to its end");
    }

    #[test]
    fn a_step_with_no_answer_in_its_time_while_the_game_runs_has_hung() {
        let (_, mut game, name) = game_on("hung", true);
        let server = serve_one(&name, |_, _| thread::sleep(SHORT * 4));
        let stop = game.ask::<Menu>(&MenuRequest::State {}).unwrap_err();
        server.join().unwrap();
        assert!(matches!(&stop, Stop::Hung(why) if why.contains("had no answer")), "{stop:?}");
        assert_eq!(game.ended(), None);
    }

    #[test]
    fn a_refusal_fails_the_scenario() {
        let (_, mut game, name) = game_on("failed", true);
        let server = serve_one(&name, |pipe, _| answer(pipe, "{\"ok\": false, \"error\": {\"code\": \"rejected\", \"message\": \"not now\"}}\n"));
        let stop = game.ask::<Menu>(&MenuRequest::State {}).unwrap_err();
        server.join().unwrap();
        assert!(matches!(&stop, Stop::Failed(why) if why.contains("not now")), "{stop:?}");
    }

    #[test]
    fn a_game_that_does_not_run_is_started_within_the_claim() {
        let (m, mut game, name) = game_on("started", false);
        let server = serve_one(&name, |pipe, _| answer(pipe, MAIN_MENU_ANSWER));
        let menu: Menu = game.ask(&MenuRequest::State {}).unwrap();
        server.join().unwrap();
        assert_eq!(menu.place, Place::MainMenu);
        assert!(m.did().contains(&Did::Start));
        assert_eq!(game.process(), Ok(GAME.id), "the claim names the game it started");
    }

    #[test]
    fn a_game_gone_since_the_scenario_began_has_crashed() {
        let (m, mut game, _) = game_on("gone", true);
        m.serving.lock().unwrap().clear();
        assert_eq!(game.ask::<Menu>(&MenuRequest::State {}).unwrap_err(), Stop::Crashed("the game ended".into()));
        assert!(!m.did().contains(&Did::Start), "a game that ran is not started again");
    }

    /// A return that starts the game again breaks the pipe before its
    /// answer; the next process's pipe answers once its main menu is up.
    #[test]
    fn a_return_waits_for_the_main_menu_across_the_restart() {
        let (_, mut game, name) = game_on("restart", true);
        let first = serve_one(&name, |_, request| assert!(request.contains("menu.return"), "{request}"));
        let next = {
            let name = name.clone();
            thread::spawn(move || {
                first.join().unwrap();
                thread::sleep(SHORT);
                serve_one(&name, |pipe, _| answer(pipe, MAIN_MENU_ANSWER)).join().unwrap();
            })
        };
        assert_eq!(game.return_to_menu(), Ok(()));
        next.join().unwrap();
    }

    #[test]
    fn a_restart_ends_the_test_game_and_starts_it_within_the_claim() {
        let (m, mut game, _) = game_on("again", true);
        assert_eq!(game.restart(), Ok(30.0));
        let ends_and_starts: Vec<Did> = m.did().into_iter().filter(|d| matches!(d, Did::End(_) | Did::Start)).collect();
        assert_eq!(ends_and_starts, [Did::End(GAME.id), Did::Start]);
    }
}
