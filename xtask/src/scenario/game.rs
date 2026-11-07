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
use eu4_harmony_remote::{
    menu::{Menu, MenuRequest, Place},
    time::{Advanced, TimeRequest},
    Error, GroupRequest,
};
use serde::de::DeserializeOwned;
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// How long a step may take unless the scenario says otherwise: an answer,
/// which a load or the days run take the longest to give.
pub const STEP: Duration = Duration::from_secs(600);
/// How long the game may take to reach its main menu, from a start or a
/// return.
pub const MAIN_MENU: Duration = Duration::from_secs(300);
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
    /// The test game's pipe, and the executable trusted to serve it.
    pipe: (String, String),
    client: Option<pipe::Client>,
    /// The saves the scenario loads, copied into the test game's user folder
    /// at each start.
    saves: Vec<String>,
    /// The game ran since the scenario began: its process missing then is an
    /// end, not a game to start.
    running: bool,
    /// How many of the claim's games had ended when the scenario began.
    ends_before: usize,
    /// How long a step may take, and the wait for the main menu.
    step: Duration,
    menu_wait: Duration,
}

impl Game {
    pub fn new(owner: &str, game_dir: &Path) -> crate::Result<Self> {
        Ok(Self::with(Box::new(claim::run_game(owner, game_dir)), &claim::pipe_of(owner)?, pipe::GAME))
    }

    /// The test game `run` reaches, asked down the pipe `name` served by
    /// `exe`.
    fn with(run: Box<dyn RunGame>, name: &str, exe: &str) -> Self {
        Self { run, pipe: (name.into(), exe.into()), client: None, saves: Vec::new(), running: false, ends_before: 0, step: STEP, menu_wait: MAIN_MENU }
    }

    /// The saves the next scenario loads.
    pub fn loads(&mut self, saves: Vec<String>) {
        self.saves = saves;
    }

    /// Whether the test game runs.
    pub fn runs(&self) -> bool {
        self.run.game().is_some()
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
        }
        Ok(answer)
    }

    /// Lets `days` days run at speed 5.
    pub fn advance(&mut self, days: u32) -> Played<Advanced> {
        self.ask(&TimeRequest::Advance { days, speed: Some(5) })
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

    /// A new scenario begins: the game, if it runs, is the one it has.
    pub fn next_scenario(&mut self) {
        self.running = self.runs();
        self.ends_before = self.run.ends().len();
    }

    /// The game for the scenario: started unless it ran, an end when it ran
    /// and is gone.
    fn started(&mut self) -> Played<()> {
        if !self.running {
            if !self.runs() {
                self.start()?;
            }
            self.running = true;
        } else if self.gone() {
            return Err(Stop::Crashed("the game ended".into()));
        }
        Ok(())
    }

    fn start(&mut self) -> Played<f64> {
        self.client = None;
        let starting = Starting { saves: self.saves.clone(), ..Default::default() };
        match self.run.start(&starting, self.menu_wait) {
            Ok(seconds) => {
                self.running = true;
                Ok(seconds)
            }
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
        let mut claim = held("a", 1000);
        claim.game = running.then_some(GAME);
        let m: &'static Recorded = Box::leak(Box::new(Recorded::new(claim::recorded::state(vec![claim]))));
        if running {
            m.serving.lock().unwrap().push((claim::pipe_of("a").unwrap(), GAME.id));
        }
        let name = pipe_name(test);
        let mut game = Game::with(Box::new(claim::recorded_run_game(m, "a")), &name, &this_program());
        (game.step, game.menu_wait) = (SHORT, Duration::from_secs(5));
        game.next_scenario();
        (m, game, name)
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
