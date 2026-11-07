//! What a scenario is handed: the run's test game, asked through remote
//! control on its pipe with the request and answer types of `remote/`, over
//! one connection while the game process lasts. Every step has a time; the
//! game ending in a step, a step taking longer, the run interrupted and a
//! check that does not hold each end the scenario, as a [`Stop`].
use crate::claim;
use eu4_harmony_client::pipe::{self, Failure};
use eu4_harmony_remote::{
    menu::{Menu, MenuRequest, Place},
    time::{Advanced, TimeRequest},
    Error, GroupRequest,
};
use serde::de::DeserializeOwned;
use std::{
    path::PathBuf,
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
    owner: String,
    game_dir: PathBuf,
    /// The test game's pipe.
    pipe: String,
    client: Option<pipe::Client>,
    /// The saves the scenario loads, copied into the test game's user folder
    /// at each start.
    saves: Vec<String>,
    /// The game ran since the scenario began: its process missing then is an
    /// end, not a game to start.
    running: bool,
}

impl Game {
    pub fn new(owner: &str, game_dir: PathBuf) -> crate::Result<Self> {
        Ok(Self { owner: owner.into(), game_dir, pipe: claim::pipe_of(owner)?, client: None, saves: Vec::new(), running: false })
    }

    /// The saves the next scenario loads.
    pub fn loads(&mut self, saves: Vec<String>) {
        self.saves = saves;
    }

    /// Whether the test game runs.
    pub fn runs(&self) -> bool {
        claim::game_of(&self.owner).is_some()
    }

    /// Ends the test game, if it runs.
    pub fn end(&mut self) {
        self.client = None;
        let _ = claim::stop_game_as(&self.owner);
    }

    /// Asks `request` and reads its answer, within [`STEP`].
    pub fn ask<T: DeserializeOwned>(&mut self, request: &impl GroupRequest) -> Played<T> {
        self.ask_within(request, STEP)
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
            None => match pipe::connect(&self.pipe, pipe::GAME, deadline) {
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
        match self.exchange::<Menu>(&MenuRequest::Return {}, STEP)? {
            Ok(Ok(_)) => {}
            Ok(Err(refused)) => return Err(Stop::Failed(format!("{name}: {refused}"))),
            // A return that starts the game again ends this process, and the
            // pipe may break before its answer comes: with a game still
            // running, that is the return.
            Err(Failure::Broken) if !self.gone() => {}
            Err(failure) => return Err(self.why(&name, &failure, STEP)),
        }
        self.main_menu()
    }

    /// Waits up to [`MAIN_MENU`] for the main menu, in this process or the
    /// next one.
    pub fn main_menu(&mut self) -> Played<()> {
        self.started()?;
        self.client = None;
        let deadline = Instant::now() + MAIN_MENU;
        while Instant::now() < deadline {
            self.not_interrupted()?;
            if self.gone() {
                return Err(Stop::Crashed("the game ended on its way to the main menu".into()));
            }
            let look = Instant::now() + Duration::from_secs(10);
            if let Ok(mut client) = pipe::connect(&self.pipe, pipe::GAME, look) {
                if let Ok(Ok(menu)) = client.ask_for::<_, Menu>(&MenuRequest::State {}, Some(look)) {
                    if menu.place == Place::MainMenu {
                        self.client = Some(client);
                        return Ok(());
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        Err(Stop::Hung(format!("the main menu did not come up in {} s", MAIN_MENU.as_secs())))
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
        let starting = claim::Starting { saves: self.saves.clone(), ..Default::default() };
        match claim::start_game_as(&self.owner, &self.game_dir, &starting, MAIN_MENU) {
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
        let deadline = Instant::now() + BETWEEN_PROCESSES;
        while Instant::now() < deadline {
            if self.runs() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(500));
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
            Failure::TimedOut | Failure::NotOpen => Stop::Hung(format!("{name} had no answer in {} s", step.as_secs())),
            other => Stop::Failed(format!("{name}: {other}")),
        }
    }
}
