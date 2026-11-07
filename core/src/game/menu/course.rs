//! What remote control asks of the main menu and of a session, and every way
//! of leaving a session, as a whole (docs/sessions.md): a save loaded or a
//! new game started from the main menu, a save loaded in a session's place,
//! a return to the main menu, a new game from a session; and the player's own
//! returns and in-game loads, which take the same steps.
//!
//! [`Course`] numbers each request, takes it from asking to settling across
//! the hooks the game passes, and answers what came of it; [`plan`] lays out
//! the steps of leaving a session, which it takes:
//!
//! * a load or start from the main menu runs at the main menu's next frame
//!   ([`Course::main_menu_frame`]), then is followed as the game takes it up,
//!   until it fails or the game takes over;
//! * an in-game load runs at the next game frame ([`Course::game_frame`]),
//!   and every in-game load, the player's too, takes its steps around the
//!   game's own ([`Course::in_game_load`]);
//! * a return takes its steps in the map manager's frame up to the restart it
//!   asks for ([`Course::returning`]), and the rest at the restart
//!   ([`Course::restart`]);
//! * a new game from a session is a return, then a start: the start, its
//!   last step, runs at the new main menu's first frame.
//!
//! Everything it does in the game goes through [`Game`]: the hooks and
//! capabilities only hand over what they hold (`super`'s `InGame`); the
//! tests record the game instead.
use super::{Launch, Place, Returned};
use crate::{
    game::{fail, GResult, Status, Tag},
    sessions::{At, End},
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long the main menu's frames may go on, after a load or start ran in
/// one, with none of the game's steps towards a game ([`Steps`]) changing,
/// before the load or start is given up. The game took its steps at most
/// 1.8 s apart in the runs (the setup's commands run at a later frame), and
/// a save being read, which no frame shows, is a step of its own; 30 s
/// leaves a slow machine more than 15 times that, and a client still hears
/// within half a minute of a start the game will not carry out.
pub(super) const QUIET: Duration = Duration::from_secs(30);

/// A load or a start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    /// The save's PhysFS path, and its player's country.
    Load(String, Tag),
    Start([u8; 3]),
    /// From a session: the save's name, without `.eu4`.
    LoadInGame(String),
}

impl Action {
    fn describe(&self) -> String {
        match self {
            Action::Load(path, _) => format!("loading {path}"),
            Action::Start(letters) => format!("a new game as {}", String::from_utf8_lossy(letters)),
            Action::LoadInGame(name) => format!("loading {name} in the session's place"),
        }
    }
}

/// What the main menu's frame sees of a game being started from it; each
/// change is a step towards the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Steps {
    /// The saves the game began to read: a save is read with no frame
    /// presented, so only the count shows it.
    pub loads: usize,
    pub page: u32,
    /// The single player setup shows, and its start was made: by remote
    /// control or by the player.
    pub started: bool,
    /// The main menu accepted the start (its start command ran).
    pub accepted: bool,
    /// The game starts at the main menu's next frame.
    pub launch: bool,
    /// The application's idler switch is pending.
    pub switching: bool,
    /// The main menu is the world's idler (in the world's map manager slot,
    /// `campaign::world::MAP_MANAGER`): a game's map manager takes the slot
    /// when it is made, at the start, before the main menu goes.
    pub idler: bool,
}

impl Steps {
    fn starting(&self) -> bool {
        self.started || self.accepted || self.launch || self.switching || !self.idler
    }
}

/// What the game's save loader has done (`crate::sessions`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Reads {
    /// The saves the game began to read since it started.
    pub loads: usize,
    /// It is reading one.
    pub reading: bool,
    /// It read the last one it began to read.
    pub last_read: bool,
}

/// Where a session is left for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    MainMenu,
    Save,
    NewGame,
}

/// One step of leaving a session, in the order [`plan`] gives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The ironman exit save; leaving goes no further when it was not
    /// written, and the session goes on.
    ExitSave,
    /// The session ends, and everything that keeps something of it hears
    /// why (`crate::sessions::ending`): after the exit save, once leaving is
    /// certain.
    Ended(End),
    /// The game's in-game load: it resets the world and reads the save.
    Load,
    /// The game's restart.
    Restart,
    /// The in-process return (GLOSSARY).
    Return,
    /// A new game started from the main menu.
    Start,
}

/// The framework settings leaving a session follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub return_without_restart: bool,
    pub save_ironman_before_load: bool,
}

impl Rules {
    pub fn now() -> Self {
        let settings = crate::settings::current();
        Self { return_without_restart: settings.return_without_restart, save_ironman_before_load: settings.save_ironman_before_load }
    }
}

/// The steps that take a session, `ironman` or not, to `to`; or why it
/// cannot go there.
pub fn plan(to: Destination, ironman: bool, rules: Rules) -> Result<Vec<Step>, &'static str> {
    let reason = match to {
        Destination::MainMenu => End::Return,
        Destination::Save => End::Load,
        Destination::NewGame => End::NewGame,
    };
    let last: &[Step] = match to {
        Destination::Save => &[Step::Load],
        Destination::MainMenu if rules.return_without_restart => &[Step::Return],
        Destination::MainMenu => &[Step::Restart],
        Destination::NewGame if rules.return_without_restart => &[Step::Return, Step::Start],
        Destination::NewGame => {
            return Err(
                "a new game from a session needs \"return without restarting\" on in the framework settings; return to the main menu and start it there",
            )
        }
    };
    // Only an in-game load may leave an ironman session without its save: an undo.
    let exit_save = ironman && (to != Destination::Save || rules.save_ironman_before_load);
    // The exit save first: when it fails the session goes on, so it is told
    // it ended only once leaving is certain. Extensions' data reaches that
    // save through `save_data`, which `on_session_ended` has no part in.
    let mut steps: Vec<Step> = exit_save.then_some(Step::ExitSave).into_iter().collect();
    steps.push(Step::Ended(reason));
    steps.extend_from_slice(last);
    Ok(steps)
}

/// What the game's in-game load answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loaded {
    /// It loaded the save (whether the game could read it, `saves` says).
    Read,
    /// It read no save: the framework stopped it, which changed nothing, or
    /// it had no name, after it took the session's interface down.
    Nothing,
    /// No such save; it changed nothing.
    NoFile,
    /// It refused the save's header, after it had reset the world.
    Refused,
    Other(u32),
}

impl Loaded {
    pub fn of(code: u32) -> Self {
        match code {
            0 => Self::Read,
            1 => Self::Nothing,
            2 => Self::NoFile,
            3 => Self::Refused,
            n => Self::Other(n),
        }
    }
}

/// What the course does in the game: the game's own (`super`'s `InGame`),
/// recorded in the tests.
pub(super) trait Game {
    /// What the main menu shows of a game being started from it, or why it
    /// is not the main menu the framework knows.
    fn steps(&mut self) -> Result<Steps, String>;
    /// Why a load or start cannot run in the main menu now, which leaves it
    /// untouched: it is not the main menu the framework knows, or it left
    /// its first page.
    fn first_page(&mut self) -> Result<(), String>;
    /// Carries a load or start out in the main menu, as the game's own clicks
    /// do, or says why it did not go through.
    fn carry_out(&mut self, action: &Action) -> Result<(), String>;
    /// The main menu as the player could use it, after a load or start that
    /// did not go through.
    fn back_to_first_page(&mut self);
    /// Nothing is continued any more: the player left the single player
    /// setup, whose page stays.
    fn not_continuing(&mut self);
    fn reads(&self) -> Reads;
    fn now(&self) -> Instant;
    /// The framework settings now.
    fn rules(&self) -> Rules;
    /// Whether the session is ironman.
    fn ironman(&self) -> bool;
    /// Sets the flag the game's own question sets when the player confirms
    /// a return: the map manager acts on it at its next frame.
    fn ask_return(&mut self) -> GResult<()>;
    /// Asks the game for its in-game load of the save `name`, in the
    /// session's place, as the load question does: what it answered, which
    /// passes [`Course::in_game_load`] on the way; or why it was not asked.
    fn load_in_game(&mut self, name: &str) -> Result<u32, String>;
    /// Writes the ironman exit save, or says why it was not written.
    fn exit_save(&mut self) -> Result<(), String>;
    /// The session ends, for `end` (`crate::sessions::ending`).
    fn end(&mut self, end: End);
    /// The game's own in-game load, inside [`Course::in_game_load`], and
    /// what it answered.
    fn load(&mut self) -> u32;
    /// The in-process return, or why it could not be made.
    fn return_in_process(&mut self) -> Result<(), String>;
    /// The game's restart, and what it answered.
    fn restart(&mut self) -> usize;
    /// The session stays: the map manager no longer returns.
    fn stay(&mut self);
    /// The player hears that the ironman exit save failed.
    fn save_failed(&mut self);
}

enum Stage {
    /// Runs at the main menu's next frame, or an in-game load at the next
    /// game frame; a new game from a session at the first frame of the main
    /// menu its return comes to.
    Asked,
    /// Ran there, and the game takes it up: what the main menu's frame saw
    /// last, and since when; and how many saves the game had begun to read
    /// when it ran.
    Taken {
        seen: Steps,
        since: Instant,
        loads: usize,
    },
    /// The game took a load or start that ran in the main menu over: a
    /// game's frame showed. A main menu's frame, after a return, no longer
    /// judges it.
    InGame {
        loads: usize,
    },
    /// An in-game load ran, with this many saves begun to be read before.
    LoadedInGame {
        loads: usize,
    },
    Failed(String),
}

struct Attempt {
    number: u64,
    action: Action,
    stage: Stage,
}

/// What a return still has to take at the restart the map manager's frame
/// asks for next: its steps from there, or why the session stays.
type Rest = Result<Vec<Step>, String>;

struct State {
    /// How many numbers were given: every request takes the next, so that
    /// one waiting is not told what came of another.
    numbered: u64,
    /// The last load or start asked for.
    launch: Option<Attempt>,
    /// The return remote control asked for that the map manager's frame has
    /// not acted on yet.
    return_asked: Option<u64>,
    /// What came of the last return remote control asked for, once the map
    /// manager's frame acted on it.
    returned: Option<(u64, Result<(), String>)>,
    /// The rest of the return the map manager's frame acted on, for the
    /// restart it asks for next.
    rest: Option<Rest>,
    /// Why the course stopped the last in-game load, if it did.
    stopped: Option<String>,
}

impl State {
    fn number(&mut self) -> u64 {
        self.numbered += 1;
        self.numbered
    }

    fn asked(&self, of: impl Fn(&Action) -> bool) -> bool {
        matches!(&self.launch, Some(Attempt { action, stage: Stage::Asked, .. }) if of(action))
    }
}

/// Each request remote control made of the main menu and of a session, and
/// the return the map manager's frame acts on. Each step takes the state,
/// decides, and lets it go while the game is called, which may call back
/// into the course.
pub(super) struct Course(Mutex<State>);

#[derive(Debug, PartialEq, Eq)]
enum Judged {
    /// The game took a step: wait again from now.
    Step,
    /// Nothing changed yet: wait.
    Wait,
    /// Nothing is being started any more: the main menu is open.
    Open,
    /// Nothing changed for [`QUIET`]: the game will not carry it out.
    Quiet,
}

/// What a main menu's frame makes of a load or start that ran, from what it
/// saw last (`seen`, `quiet` ago) and what it sees `now`.
fn judge(seen: &Steps, now: &Steps, loading: bool, quiet: Duration) -> Judged {
    if !now.starting() && !loading {
        Judged::Open
    } else if now != seen {
        Judged::Step
    } else if quiet < QUIET {
        Judged::Wait
    } else {
        Judged::Quiet
    }
}

pub(super) const UNREAD: &str = "the game could not read the save: it starts a game anyway, from what it read of it (menu and session say where it is)";

impl Course {
    pub const fn new() -> Self {
        Self(Mutex::new(State { numbered: 0, launch: None, return_asked: None, returned: None, rest: None, stopped: None }))
    }

    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Asks for `action`, to run at the main menu's next frame, or at the
    /// next game frame for an in-game load; answers its number. It replaces
    /// any load or start asked for before.
    pub fn ask(&self, action: Action) -> u64 {
        self.with(|s| {
            let number = s.number();
            s.launch = Some(Attempt { number, action, stage: Stage::Asked });
            number
        })
    }

    /// From a session: asks for a new game as `letters`, which the session
    /// is left for as a return leaves it; the start runs at the first frame
    /// of the main menu the return comes to. `known` checks the country, once
    /// leaving for a new game is possible.
    pub fn new_game_from_session(&self, game: &mut impl Game, letters: [u8; 3], known: impl FnOnce() -> GResult<()>) -> GResult<u64> {
        if let Err(why) = plan(Destination::NewGame, false, game.rules()) {
            return fail(Status::Unavailable, why);
        }
        known()?;
        game.ask_return()?;
        log!("menu: a new game as {} from the session", String::from_utf8_lossy(&letters));
        Ok(self.ask(Action::Start(letters)))
    }

    /// Asks for a return to the main menu, as the game's own question does
    /// when the player confirms it. With "return without restarting" on it
    /// is an in-process return, whose number [`Course::returned`] takes, the
    /// same for every request made before the map manager's frame acts on
    /// it; otherwise the game's next frame starts `eu4.exe` again (None).
    pub fn return_to_main_menu(&self, game: &mut impl Game) -> GResult<Option<u64>> {
        game.ask_return()?;
        log!("menu: returning to the main menu");
        if !game.rules().return_without_restart {
            return Ok(None);
        }
        Ok(Some(self.with(|s| match s.return_asked {
            Some(number) => number,
            None => {
                let number = s.number();
                s.return_asked = Some(number);
                number
            }
        })))
    }

    /// What came of the return `number` so far: an in-process return was set
    /// off and the main menu is up (`at_main_menu`), or the session stays,
    /// and why.
    pub fn returned(&self, number: u64, at_main_menu: impl FnOnce() -> bool) -> Returned {
        let outcome = self.with(|s| s.returned.as_ref().filter(|(n, _)| *n == number).map(|(_, outcome)| outcome.clone()));
        match outcome {
            Some(Err(why)) => Returned::Failed(why),
            Some(Ok(())) if at_main_menu() => Returned::Done,
            _ => Returned::Waiting,
        }
    }

    /// The last load or start asked for.
    pub fn last(&self) -> u64 {
        self.with(|s| s.launch.as_ref().map_or(0, |a| a.number))
    }

    /// Whether a load remote control asked for from a session has not run yet.
    pub fn loading_in_game(&self) -> bool {
        self.with(|s| s.asked(|a| matches!(a, Action::LoadInGame(_))))
    }

    /// Whether a game is being started from the main menu: remote control
    /// asked for one that has not run yet, a save is being read, or the
    /// game's own state says so (see [`Steps`]), whoever started it. A main
    /// menu that no longer reads as one, as when a game takes over within the
    /// frame, is not open either.
    pub fn starting(&self, game: &mut impl Game) -> bool {
        self.with(|s| s.asked(|_| true)) || game.reads().reading || game.steps().map_or(true, |s| s.starting())
    }

    /// Where the game is, at the safe point `at` (`game` being at the main
    /// menu there, if it is).
    pub fn place(&self, at: At, game: &mut impl Game) -> Place {
        match at {
            At::Game if self.loading_in_game() => Place::Starting,
            At::Game => Place::Game,
            At::MainMenu(_) if !self.starting(game) => Place::MainMenu,
            At::MainMenu(_) | At::Elsewhere => Place::Starting,
        }
    }

    /// What came of the load or start `number` ([`Course::last`]), at a
    /// safe point outside a game's frame. A load fails when the game read a
    /// save since it ran and could not read it: its loader said so. The game
    /// goes on and starts a game from what it read (seen in the game with a
    /// save whose game state was damaged).
    pub fn outcome(&self, number: u64, reads: Reads) -> Launch {
        let unread = |loads: usize| reads.loads > loads && !reads.reading && !reads.last_read;
        self.with(|s| match &s.launch {
            Some(Attempt { number: n, action, stage }) if *n == number => match stage {
                Stage::Failed(why) => Launch::Failed(why.clone()),
                Stage::Taken { loads, .. } | Stage::InGame { loads } if matches!(action, Action::Load(..)) && unread(*loads) => Launch::Failed(UNREAD.into()),
                Stage::LoadedInGame { loads } if unread(*loads) => Launch::Failed(UNREAD.into()),
                Stage::Asked => Launch::Asked,
                Stage::Taken { .. } | Stage::InGame { .. } | Stage::LoadedInGame { .. } => Launch::Waiting,
            },
            _ => Launch::Failed("another load or start was asked for".into()),
        })
    }

    /// At a game frame, after the map manager's: runs the in-game load
    /// remote control asked for, if there is one; then the frame is over. A
    /// load or start that ran in the main menu was taken over by the game.
    pub fn game_frame(&self, game: &mut impl Game) -> bool {
        let Some((number, name)) = self.with(|s| match &mut s.launch {
            Some(Attempt { number, action: Action::LoadInGame(name), stage: Stage::Asked }) => Some((*number, name.clone())),
            Some(Attempt { stage, .. }) => {
                if let Stage::Taken { loads, .. } = *stage {
                    *stage = Stage::InGame { loads };
                }
                None
            }
            None => None,
        }) else {
            return false;
        };
        let loads = game.reads().loads;
        let stage = match self.load_in_game(game, &name) {
            Ok(()) => {
                log!("menu: loading {name} in the session's place started");
                Stage::LoadedInGame { loads }
            }
            Err(why) => {
                log!("menu: loading {name} in the session's place failed: {why}");
                Stage::Failed(why)
            }
        };
        self.settle(number, stage);
        true
    }

    /// The game's in-game load of the save `name`, and what came of it.
    fn load_in_game(&self, game: &mut impl Game, name: &str) -> Result<(), String> {
        self.with(|s| s.stopped = None);
        match Loaded::of(game.load_in_game(name)?) {
            Loaded::Read => Ok(()),
            Loaded::Nothing => Err(self.with(|s| s.stopped.take()).unwrap_or_else(|| "the game took no save name".into())),
            Loaded::NoFile => Err("the game found no such save".into()),
            Loaded::Refused => Err("the game refused the save's header after it had reset the world".into()),
            Loaded::Other(n) => Err(format!("the game's in-game load answered {n}")),
        }
    }

    /// At the end of the main menu's frame: runs the load or start asked
    /// for, then follows the game taking it up, and gives it up when the main
    /// menu shows with nothing started from it, or when the game takes no
    /// step for [`QUIET`]. A new game asked for from a session runs at the
    /// first frame of the main menu its return came to: the last step of its
    /// plan.
    pub fn main_menu_frame(&self, game: &mut impl Game) {
        enum Now {
            Run(Action),
            Follow(Action, Steps, Instant, usize),
        }
        let Some((number, now)) = self.with(|s| match &mut s.launch {
            // Asked for in a session that ended before its next game frame.
            Some(Attempt { action: Action::LoadInGame(name), stage: stage @ Stage::Asked, .. }) => {
                log!("menu: loading {name} in the session's place failed: the session ended first");
                *stage = Stage::Failed("the session ended before the load could run".into());
                None
            }
            Some(Attempt { number, action, stage: Stage::Asked }) => Some((*number, Now::Run(action.clone()))),
            Some(Attempt { number, action, stage: Stage::Taken { seen, since, loads } }) => Some((*number, Now::Follow(action.clone(), *seen, *since, *loads))),
            _ => None,
        }) else {
            return;
        };
        let stage = match now {
            Now::Run(action) => run(game, &action),
            Now::Follow(action, seen, since, loads) => follow(game, &action, seen, since, loads),
        };
        self.settle(number, stage);
    }

    /// Where the attempt `number` got to, unless another was asked for
    /// meanwhile.
    fn settle(&self, number: u64, stage: Stage) {
        self.with(|s| {
            if let Some(a) = s.launch.as_mut().filter(|a| a.number == number) {
                a.stage = stage;
            }
        });
    }

    /// Every in-game load, whoever asked for it: the steps [`plan`] gives
    /// for it, around the game's own. Answers what to give the game's caller:
    /// what the game's load answered, or 1, as the game's load answers when
    /// it read nothing, when the course stopped it before it, which changed
    /// nothing.
    pub fn in_game_load(&self, game: &mut impl Game) -> u32 {
        let ironman = game.ironman();
        let steps = plan(Destination::Save, ironman, game.rules());
        log!("leave: an in-game load, {}ironman: {steps:?}", if ironman { "" } else { "not " });
        match run_load(game, steps.unwrap_or_default()) {
            Ok(code) => code,
            Err(why) => {
                log!("leave: the load did not happen: {why}");
                self.with(|s| s.stopped = Some(why));
                1
            }
        }
    }

    /// In the map manager's frame, where it writes the ironman exit save as
    /// it returns to the main menu, whoever asked for the return: takes the
    /// steps up to the restart it asks for next. A new game asked for from
    /// the session is what the return leaves it for, when it can be.
    pub fn returning(&self, game: &mut impl Game) {
        let rules = game.rules();
        let new_game = self.with(|s| s.asked(|a| matches!(a, Action::Start(_))));
        let to = returning_to(new_game, rules);
        let rest = run_return(game, plan(to, game.ironman(), rules));
        log!("leave: returning to the main menu{}, then: {rest:?}", if to == Destination::NewGame { " for a new game" } else { "" });
        self.with(|s| s.rest = Some(rest));
    }

    /// The game's restart: with the rest of the return the map manager's
    /// frame acted on, if it did, the game restarts, or returns in process,
    /// or stays in the session. Answers what to give the game.
    pub fn restart(&self, game: &mut impl Game) -> usize {
        let rest = self.with(|s| s.rest.take());
        if let Some(Err(why)) = &rest {
            log!("leave: the session stays: {why}");
        }
        let (answer, outcome) = run_restart(game, rest);
        if let Some(outcome) = outcome {
            self.with(|s| {
                if let Err(why) = &outcome {
                    // A new game asked for from the session does not come either.
                    if let Some(a @ Attempt { action: Action::Start(_), stage: Stage::Asked, .. }) = &mut s.launch {
                        a.stage = Stage::Failed(why.clone());
                    }
                }
                if let Some(number) = s.return_asked.take() {
                    s.returned = Some((number, outcome));
                }
            });
        }
        answer
    }
}

/// Runs a load or start in the main menu's frame.
fn run(game: &mut impl Game, action: &Action) -> Stage {
    let result = game.first_page().and_then(|()| match game.carry_out(action) {
        Ok(()) => game.steps(),
        Err(why) => {
            game.back_to_first_page();
            Err(why)
        }
    });
    match result {
        Ok(seen) => {
            log!("menu: {} started", action.describe());
            Stage::Taken { seen, since: game.now(), loads: seen.loads }
        }
        Err(why) => {
            log!("menu: {} failed: {why}", action.describe());
            Stage::Failed(why)
        }
    }
}

/// Follows the game taking up a load or start that ran, at a main menu's
/// frame: the main menu still shows, so the game has not switched to the
/// game yet.
fn follow(game: &mut impl Game, action: &Action, seen: Steps, since: Instant, loads: usize) -> Stage {
    let why = match game.steps() {
        // Not the main menu the framework knows: nothing is touched.
        Err(why) => why,
        Ok(now) => match judge(&seen, &now, game.reads().reading, game.now().saturating_duration_since(since)) {
            Judged::Step => return Stage::Taken { seen: now, since: game.now(), loads },
            Judged::Wait => return Stage::Taken { seen, since, loads },
            Judged::Open => {
                game.not_continuing();
                "the game did not start it: the main menu is open".to_string()
            }
            Judged::Quiet => {
                game.back_to_first_page();
                format!("the game did not start it: it took no step towards it for {} s", QUIET.as_secs())
            }
        },
    };
    log!("menu: {} failed: {why}", action.describe());
    Stage::Failed(why)
}

/// Takes the steps of an in-game load: what the game's load answered, or why
/// the framework stopped the load before it, which changed nothing.
fn run_load(game: &mut impl Game, steps: Vec<Step>) -> Result<u32, String> {
    for step in steps {
        match step {
            Step::ExitSave => game.exit_save().inspect_err(|_| game.save_failed())?,
            Step::Ended(end) => game.end(end),
            Step::Load => return Ok(game.load()),
            Step::Restart | Step::Return | Step::Start => {}
        }
    }
    // As the game's load answers when it read nothing.
    Ok(1)
}

/// Where the return the map manager's frame acts on leaves the session for:
/// a new game when remote control asked for one from it (`menu.start`),
/// which starts at the new main menu's first frame; the main menu
/// otherwise. A new game needs the in-process return: with it turned off
/// since the start was asked for, the game restarts, and the start is
/// answered as the process ends.
fn returning_to(new_game_asked: bool, rules: Rules) -> Destination {
    if new_game_asked && rules.return_without_restart {
        Destination::NewGame
    } else {
        Destination::MainMenu
    }
}

/// In the map manager's frame, as it returns: takes the steps up to the
/// restart, and answers the rest.
fn run_return(game: &mut impl Game, steps: Result<Vec<Step>, &'static str>) -> Rest {
    let mut steps = steps.map_err(String::from)?;
    let restart = steps.iter().position(|s| matches!(s, Step::Restart | Step::Return)).unwrap_or(steps.len());
    for step in steps.drain(..restart) {
        match step {
            Step::ExitSave => game.exit_save()?,
            Step::Ended(end) => game.end(end),
            Step::Load | Step::Restart | Step::Return | Step::Start => {}
        }
    }
    Ok(steps)
}

/// At the restart: takes the rest of the return the map manager's frame
/// acted on, if it did. Answers what to give the game, and what came of the
/// return unless the game restarts.
fn run_restart(game: &mut impl Game, rest: Option<Rest>) -> (usize, Option<Result<(), String>>) {
    match rest {
        Some(Err(why)) => {
            game.stay();
            game.save_failed();
            (0, Some(Err(why)))
        }
        // The map manager's flag stays set: the rest of its frame is skipped,
        // and it goes at the application's switch. A start that follows, its
        // last step, stays asked for, and runs at the new main menu's first
        // frame ([`Course::main_menu_frame`]).
        Some(Ok(rest)) if rest.first() == Some(&Step::Return) && game.return_in_process().is_ok() => (0, Some(Ok(()))),
        _ => (game.restart(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Rules = Rules { return_without_restart: true, save_ironman_before_load: true };
    const UNDO: Rules = Rules { return_without_restart: true, save_ironman_before_load: false };
    const RESTART: Rules = Rules { return_without_restart: false, save_ironman_before_load: true };

    #[test]
    fn a_load_or_start_is_followed_by_the_games_steps() {
        let started = Steps { loads: 4, page: 3, started: true, accepted: false, launch: false, switching: false, idler: true };
        let second = Duration::from_secs(1);
        // Just ran, and waiting for the setup's commands.
        assert_eq!(judge(&started, &started, false, second), Judged::Wait);
        // Each step waits again from then on: a save read, the start
        // accepted, the start at the next frame.
        let loaded = Steps { loads: 5, ..started };
        assert_eq!(judge(&started, &loaded, false, QUIET), Judged::Step);
        let accepted = Steps { accepted: true, ..loaded };
        assert_eq!(judge(&loaded, &accepted, false, QUIET + second), Judged::Step);
        // Once accepted, it stays so, whatever page shows.
        let elsewhere = Steps { page: 0, started: false, launch: true, ..accepted };
        assert_eq!(judge(&accepted, &elsewhere, false, second), Judged::Step);
        // No step for QUIET: given up.
        assert_eq!(judge(&started, &started, false, QUIET - second), Judged::Wait);
        assert_eq!(judge(&started, &started, false, QUIET), Judged::Quiet);
        // The setup left before the start was accepted: the main menu is
        // open, unless a save is being read.
        let left = Steps { page: 0, started: false, ..started };
        assert_eq!(judge(&started, &left, false, second), Judged::Open);
        assert_eq!(judge(&started, &left, true, second), Judged::Step);
        // The setup shown again with its start made, by the player, say.
        assert_eq!(judge(&left, &started, false, second), Judged::Step);
        // A game's map manager made, the main menu still showing: never open.
        let taken_over = Steps { idler: false, ..left };
        assert_eq!(judge(&left, &taken_over, false, second), Judged::Step);
        assert_eq!(judge(&taken_over, &taken_over, false, QUIET - second), Judged::Wait);
    }

    #[test]
    fn an_in_game_load_saves_an_ironman_session_first_unless_undoing() {
        let ended = Step::Ended(End::Load);
        assert_eq!(plan(Destination::Save, false, ALL), Ok(vec![ended, Step::Load]));
        assert_eq!(plan(Destination::Save, true, ALL), Ok(vec![Step::ExitSave, ended, Step::Load]));
        assert_eq!(plan(Destination::Save, true, UNDO), Ok(vec![ended, Step::Load]));
        // Restarting has nothing to do with it.
        assert_eq!(plan(Destination::Save, true, RESTART), Ok(vec![Step::ExitSave, ended, Step::Load]));
    }

    #[test]
    fn returning_always_saves_an_ironman_session() {
        let ended = Step::Ended(End::Return);
        assert_eq!(plan(Destination::MainMenu, false, ALL), Ok(vec![ended, Step::Return]));
        assert_eq!(plan(Destination::MainMenu, true, UNDO), Ok(vec![Step::ExitSave, ended, Step::Return]));
        assert_eq!(plan(Destination::MainMenu, true, RESTART), Ok(vec![Step::ExitSave, ended, Step::Restart]));
    }

    #[test]
    fn a_new_game_from_a_session_returns_first_and_needs_no_restart() {
        let ended = Step::Ended(End::NewGame);
        assert_eq!(plan(Destination::NewGame, true, UNDO), Ok(vec![Step::ExitSave, ended, Step::Return, Step::Start]));
        assert_eq!(plan(Destination::NewGame, false, ALL), Ok(vec![ended, Step::Return, Step::Start]));
        assert!(plan(Destination::NewGame, false, RESTART).is_err());
    }

    #[test]
    fn the_session_ends_only_once_its_ironman_exit_save_was_written() {
        // A failed exit save stops leaving where it is: the session must not
        // have been told it ended, as it goes on.
        for to in [Destination::Save, Destination::MainMenu, Destination::NewGame] {
            let steps = plan(to, true, ALL).unwrap();
            let save = steps.iter().position(|s| *s == Step::ExitSave).unwrap();
            let ended = steps.iter().position(|s| matches!(s, Step::Ended(_))).unwrap();
            assert!(save < ended, "{to:?}: {steps:?}");
        }
    }

    /// What the game was asked to do, in order.
    #[derive(Debug, PartialEq, Eq)]
    enum Did {
        CarryOut(Action),
        BackToFirstPage,
        NotContinuing,
        AskReturn,
        LoadInGame(String),
        ExitSave,
        End(End),
        Load,
        ReturnInProcess,
        Restart,
        Stay,
        SaveFailed,
    }

    /// The main menu on its first page, nothing started from it.
    const OPEN: Steps = Steps { loads: 2, page: 0, started: false, accepted: false, launch: false, switching: false, idler: true };
    /// The single player setup entered with its start made.
    const STARTED: Steps = Steps { page: 3, started: true, ..OPEN };

    /// The game, recorded: what its main menu shows, its settings and
    /// session, what fails when told to, and a clock moved by hand. Its
    /// in-game load passes the course's, as the game's passes the hook.
    struct Recorded<'a> {
        course: &'a Course,
        did: Vec<Did>,
        /// What the main menu shows; None when it is not the main menu the
        /// framework knows.
        steps: Option<Steps>,
        /// What the main menu shows once a load or start was carried out.
        after: Steps,
        off_first_page: bool,
        /// Why a load or start, or asking for an in-game load, fails.
        fails: Option<&'static str>,
        reads: Reads,
        start: Instant,
        clock: Duration,
        rules: Rules,
        ironman: bool,
        /// Whether a game is going on, with its map manager.
        in_game: bool,
        /// What the game's in-game load answers.
        loaded: u32,
        save_fails: bool,
        return_fails: bool,
    }

    impl<'a> Recorded<'a> {
        fn new(course: &'a Course) -> Self {
            Self {
                course,
                did: Vec::new(),
                steps: Some(OPEN),
                after: STARTED,
                off_first_page: false,
                fails: None,
                reads: Reads { loads: 2, reading: false, last_read: true },
                start: Instant::now(),
                clock: Duration::ZERO,
                rules: ALL,
                ironman: false,
                in_game: false,
                loaded: 0,
                save_fails: false,
                return_fails: false,
            }
        }

        /// In a session, its map manager there.
        fn in_game(course: &'a Course) -> Self {
            Self { in_game: true, steps: None, ..Self::new(course) }
        }

        /// The map manager's frame acting on a return, then its restart, as
        /// the two hooks take it; then, after an in-process return, the new
        /// main menu's first frame. Answers what the restart gave the game.
        fn return_now(&mut self) -> usize {
            let course = self.course;
            course.returning(self);
            let answer = course.restart(self);
            if self.did.last() == Some(&Did::ReturnInProcess) {
                self.in_game = false;
                self.steps = Some(OPEN);
                course.main_menu_frame(self);
            }
            answer
        }
    }

    impl Game for Recorded<'_> {
        fn steps(&mut self) -> Result<Steps, String> {
            self.steps.ok_or_else(|| "unexpected main menu object".into())
        }
        fn first_page(&mut self) -> Result<(), String> {
            self.steps()?;
            match self.off_first_page {
                true => Err("the main menu left its first page before the game could take it up".into()),
                false => Ok(()),
            }
        }
        fn carry_out(&mut self, action: &Action) -> Result<(), String> {
            self.did.push(Did::CarryOut(action.clone()));
            match self.fails {
                Some(why) => Err(why.into()),
                None => {
                    self.steps = Some(self.after);
                    Ok(())
                }
            }
        }
        fn back_to_first_page(&mut self) {
            self.did.push(Did::BackToFirstPage);
        }
        fn not_continuing(&mut self) {
            self.did.push(Did::NotContinuing);
        }
        fn reads(&self) -> Reads {
            self.reads
        }
        fn now(&self) -> Instant {
            self.start + self.clock
        }
        fn rules(&self) -> Rules {
            self.rules
        }
        fn ironman(&self) -> bool {
            self.ironman
        }
        fn ask_return(&mut self) -> GResult<()> {
            if !self.in_game {
                return fail(Status::NotInGame, "no game is going on");
            }
            self.did.push(Did::AskReturn);
            Ok(())
        }
        fn load_in_game(&mut self, name: &str) -> Result<u32, String> {
            self.did.push(Did::LoadInGame(name.into()));
            if let Some(why) = self.fails {
                return Err(why.into());
            }
            let course = self.course;
            Ok(course.in_game_load(self))
        }
        fn exit_save(&mut self) -> Result<(), String> {
            self.did.push(Did::ExitSave);
            if self.save_fails {
                Err("no room on the disk".into())
            } else {
                Ok(())
            }
        }
        fn end(&mut self, end: End) {
            self.did.push(Did::End(end));
        }
        fn load(&mut self) -> u32 {
            self.did.push(Did::Load);
            self.loaded
        }
        fn return_in_process(&mut self) -> Result<(), String> {
            self.did.push(Did::ReturnInProcess);
            if self.return_fails {
                Err("no memory for the main menu".into())
            } else {
                Ok(())
            }
        }
        fn restart(&mut self) -> usize {
            self.did.push(Did::Restart);
            7
        }
        fn stay(&mut self) {
            self.did.push(Did::Stay);
        }
        fn save_failed(&mut self) {
            self.did.push(Did::SaveFailed);
        }
    }

    fn load() -> Action {
        Action::Load("save games/Aragon.eu4".into(), Tag(0x5a))
    }

    #[test]
    fn a_load_from_the_main_menu_runs_at_its_next_frame_and_waits_for_the_game() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        let number = course.ask(load());
        assert_eq!(course.outcome(number, game.reads), Launch::Asked);
        assert!(course.starting(&mut game));
        course.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(load())]);
        assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
        // The game reads the save, and reads it whole.
        game.reads = Reads { loads: 3, reading: true, last_read: false };
        game.steps = Some(Steps { loads: 3, ..STARTED });
        course.main_menu_frame(&mut game);
        game.reads = Reads { loads: 3, reading: false, last_read: true };
        assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
        // Run once only.
        assert_eq!(game.did, [Did::CarryOut(load())]);
    }

    #[test]
    fn a_load_or_start_the_game_took_over_is_not_judged_by_the_main_menu_after_a_return() {
        for action in [Action::Start(*b"FRA"), load()] {
            let course = Course::new();
            let mut game = Recorded::new(&course);
            let number = course.ask(action.clone());
            course.main_menu_frame(&mut game);
            // The game takes it over: a game's frame shows, which it does not take.
            assert!(!course.game_frame(&mut game));
            // Back at a new main menu, nothing started from it.
            game.steps = Some(OPEN);
            course.main_menu_frame(&mut game);
            assert_eq!(game.did, [Did::CarryOut(action.clone())]);
            assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
            assert!(!course.starting(&mut game));
            // A save the game could not read still fails a load.
            game.reads = Reads { loads: 3, reading: false, last_read: false };
            let unread = if matches!(action, Action::Load(..)) { Launch::Failed(UNREAD.into()) } else { Launch::Waiting };
            assert_eq!(course.outcome(number, game.reads), unread);
        }
    }

    #[test]
    fn a_load_or_start_that_does_not_go_through_puts_the_main_menu_back() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        game.fails = Some("the single player setup did not take the country");
        let number = course.ask(Action::Start(*b"FRA"));
        course.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"FRA")), Did::BackToFirstPage]);
        assert_eq!(course.outcome(number, game.reads), Launch::Failed("the single player setup did not take the country".into()));
        assert!(!course.starting(&mut game));
    }

    #[test]
    fn a_main_menu_that_left_its_first_page_or_is_not_known_is_left_untouched() {
        for off_first_page in [true, false] {
            let course = Course::new();
            let mut game = Recorded::new(&course);
            game.off_first_page = off_first_page;
            game.steps = if off_first_page { Some(OPEN) } else { None };
            let why = game.first_page().unwrap_err();
            let number = course.ask(load());
            course.main_menu_frame(&mut game);
            assert_eq!(game.did, []);
            assert_eq!(course.outcome(number, game.reads), Launch::Failed(why));
        }
    }

    #[test]
    fn a_start_the_game_takes_no_step_towards_is_given_up_after_a_quiet_spell() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        let number = course.ask(Action::Start(*b"CAS"));
        course.main_menu_frame(&mut game);
        // A step a while in waits again from then on.
        game.clock = Duration::from_secs(20);
        game.steps = Some(Steps { accepted: true, ..STARTED });
        course.main_menu_frame(&mut game);
        game.clock = Duration::from_secs(45);
        course.main_menu_frame(&mut game);
        assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
        game.clock = Duration::from_secs(20) + QUIET;
        course.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"CAS")), Did::BackToFirstPage]);
        assert_eq!(course.outcome(number, game.reads), Launch::Failed("the game did not start it: it took no step towards it for 30 s".into()));
    }

    #[test]
    fn a_setup_the_player_left_gives_the_start_up_and_keeps_its_page() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        let number = course.ask(Action::Start(*b"CAS"));
        course.main_menu_frame(&mut game);
        game.steps = Some(Steps { page: 1, ..OPEN });
        course.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"CAS")), Did::NotContinuing]);
        assert_eq!(course.outcome(number, game.reads), Launch::Failed("the game did not start it: the main menu is open".into()));
        // Given up once: the frames after leave it.
        course.main_menu_frame(&mut game);
        assert_eq!(game.did.len(), 2);
    }

    #[test]
    fn a_save_the_game_could_not_read_fails_its_load_though_a_game_starts() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        let number = course.ask(load());
        course.main_menu_frame(&mut game);
        game.reads = Reads { loads: 3, reading: false, last_read: false };
        assert_eq!(course.outcome(number, game.reads), Launch::Failed(UNREAD.into()));
        // A new game reads no save of its own: whatever was read last is not its.
        let number = course.ask(Action::Start(*b"CAS"));
        game.reads = Reads { loads: 2, reading: false, last_read: true };
        course.main_menu_frame(&mut game);
        game.reads = Reads { loads: 3, reading: false, last_read: false };
        assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
    }

    #[test]
    fn one_waiting_hears_only_of_its_own_load_or_start() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        let first = course.ask(load());
        let second = course.ask(Action::Start(*b"CAS"));
        assert_eq!(course.last(), second);
        assert_eq!(course.outcome(first, game.reads), Launch::Failed("another load or start was asked for".into()));
        course.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"CAS"))]);
    }

    #[test]
    fn the_main_menu_is_open_unless_something_is_being_started_from_it() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        assert_eq!(course.place(At::MainMenu(1), &mut game), Place::MainMenu);
        assert_eq!(course.place(At::Elsewhere, &mut game), Place::Starting);
        // A save being read, whoever loads it.
        game.reads.reading = true;
        assert_eq!(course.place(At::MainMenu(1), &mut game), Place::Starting);
        game.reads.reading = false;
        // The player's own start, or a main menu that reads as none.
        game.steps = Some(STARTED);
        assert!(course.starting(&mut game));
        game.steps = None;
        assert!(course.starting(&mut game));
        game.steps = Some(OPEN);
        course.ask(load());
        assert!(course.starting(&mut game));
    }

    #[test]
    fn an_in_game_load_runs_at_the_next_game_frame_through_the_games_load() {
        let course = Course::new();
        let mut game = Recorded::in_game(&course);
        let number = course.ask(Action::LoadInGame("Aragon".into()));
        assert!(course.loading_in_game());
        assert_eq!(course.place(At::Game, &mut game), Place::Starting);
        assert!(course.game_frame(&mut game));
        assert!(!course.game_frame(&mut game));
        assert_eq!(game.did, [Did::LoadInGame("Aragon".into()), Did::End(End::Load), Did::Load]);
        assert_eq!(course.place(At::Game, &mut game), Place::Game);
        assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
        // The game's load read the save, but could not.
        game.reads = Reads { loads: 3, reading: false, last_read: false };
        assert_eq!(course.outcome(number, game.reads), Launch::Failed(UNREAD.into()));
    }

    #[test]
    fn an_ironman_in_game_load_is_saved_ended_and_loaded_in_that_order() {
        let course = Course::new();
        let mut game = Recorded { ironman: true, ..Recorded::in_game(&course) };
        let number = course.ask(Action::LoadInGame("Castilla".into()));
        assert!(course.game_frame(&mut game));
        assert_eq!(game.did, [Did::LoadInGame("Castilla".into()), Did::ExitSave, Did::End(End::Load), Did::Load]);
        assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
        // Undoing: the session as the save holds it, no exit save.
        game.did.clear();
        game.rules = UNDO;
        course.ask(Action::LoadInGame("Castilla".into()));
        course.game_frame(&mut game);
        assert_eq!(game.did, [Did::LoadInGame("Castilla".into()), Did::End(End::Load), Did::Load]);
    }

    #[test]
    fn an_in_game_load_whose_exit_save_fails_changes_nothing_and_says_why() {
        let course = Course::new();
        let mut game = Recorded { ironman: true, save_fails: true, ..Recorded::in_game(&course) };
        let number = course.ask(Action::LoadInGame("Castilla".into()));
        assert!(course.game_frame(&mut game));
        // The game's load never ran, and the session was not told it ended.
        assert_eq!(game.did, [Did::LoadInGame("Castilla".into()), Did::ExitSave, Did::SaveFailed]);
        assert_eq!(course.outcome(number, game.reads), Launch::Failed("no room on the disk".into()));
        // The player's own load stopped so answers the game as nothing read.
        game.did.clear();
        assert_eq!(course.in_game_load(&mut game), 1);
        assert_eq!(game.did, [Did::ExitSave, Did::SaveFailed]);
    }

    #[test]
    fn an_in_game_load_the_game_refuses_fails_with_its_reason() {
        for (loaded, why) in
            [(2, "the game found no such save"), (3, "the game refused the save's header after it had reset the world"), (1, "the game took no save name")]
        {
            let course = Course::new();
            let mut game = Recorded { loaded, ..Recorded::in_game(&course) };
            let number = course.ask(Action::LoadInGame("Aragon".into()));
            assert!(course.game_frame(&mut game));
            assert_eq!(course.outcome(number, game.reads), Launch::Failed(why.into()));
        }
        // Not asked at all: no map manager, say.
        let course = Course::new();
        let mut game = Recorded { fails: Some("no game is going on"), ..Recorded::in_game(&course) };
        let number = course.ask(Action::LoadInGame("Aragon".into()));
        assert!(course.game_frame(&mut game));
        assert_eq!(game.did, [Did::LoadInGame("Aragon".into())]);
        assert_eq!(course.outcome(number, game.reads), Launch::Failed("no game is going on".into()));
    }

    #[test]
    fn an_in_game_load_asked_in_a_session_that_ended_first_fails_at_the_main_menu() {
        let course = Course::new();
        let mut game = Recorded::new(&course);
        let number = course.ask(Action::LoadInGame("Aragon".into()));
        course.main_menu_frame(&mut game);
        assert!(!course.game_frame(&mut game));
        assert_eq!(game.did, []);
        assert_eq!(course.outcome(number, game.reads), Launch::Failed("the session ended before the load could run".into()));
    }

    #[test]
    fn a_return_without_restarting_is_made_in_process_and_answered_once_the_main_menu_is_up() {
        let course = Course::new();
        let mut game = Recorded { ironman: true, ..Recorded::in_game(&course) };
        let number = course.return_to_main_menu(&mut game).unwrap().unwrap();
        assert_eq!(course.returned(number, || false), Returned::Waiting);
        course.returning(&mut game);
        assert_eq!(course.restart(&mut game), 0);
        assert_eq!(game.did, [Did::AskReturn, Did::ExitSave, Did::End(End::Return), Did::ReturnInProcess]);
        // Set off, and answered once the main menu is up.
        assert_eq!(course.returned(number, || false), Returned::Waiting);
        assert_eq!(course.returned(number, || true), Returned::Done);
    }

    #[test]
    fn returns_asked_for_while_one_is_pending_are_answered_with_it() {
        let course = Course::new();
        let mut game = Recorded { ironman: true, save_fails: true, ..Recorded::in_game(&course) };
        let first = course.return_to_main_menu(&mut game).unwrap().unwrap();
        let second = course.return_to_main_menu(&mut game).unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(game.return_now(), 0);
        // The exit save failed: the session stays, and both hear why.
        assert_eq!(game.did, [Did::AskReturn, Did::AskReturn, Did::ExitSave, Did::Stay, Did::SaveFailed]);
        assert_eq!(course.returned(first, || true), Returned::Failed("no room on the disk".into()));
        // Once it was acted on, another is another return.
        game.save_fails = false;
        let third = course.return_to_main_menu(&mut game).unwrap().unwrap();
        assert_ne!(third, first);
        assert_eq!(course.returned(third, || true), Returned::Waiting);
        game.return_now();
        assert_eq!(course.returned(third, || true), Returned::Done);
    }

    #[test]
    fn a_return_with_restarting_restarts_the_game_unanswered() {
        let course = Course::new();
        let mut game = Recorded { ironman: true, rules: RESTART, ..Recorded::in_game(&course) };
        assert_eq!(course.return_to_main_menu(&mut game).unwrap(), None);
        assert_eq!(game.return_now(), 7);
        assert_eq!(game.did, [Did::AskReturn, Did::ExitSave, Did::End(End::Return), Did::Restart]);
    }

    #[test]
    fn an_in_process_return_that_cannot_be_made_restarts_the_game() {
        let course = Course::new();
        let mut game = Recorded { return_fails: true, ..Recorded::in_game(&course) };
        let number = course.return_to_main_menu(&mut game).unwrap().unwrap();
        assert_eq!(game.return_now(), 7);
        assert_eq!(game.did, [Did::AskReturn, Did::End(End::Return), Did::ReturnInProcess, Did::Restart]);
        // Answered as the process ends.
        assert_eq!(course.returned(number, || true), Returned::Waiting);
    }

    #[test]
    fn a_return_the_player_asked_for_is_taken_alike_and_answers_no_request() {
        let course = Course::new();
        let mut game = Recorded { ironman: true, save_fails: true, ..Recorded::in_game(&course) };
        assert_eq!(game.return_now(), 0);
        assert_eq!(game.did, [Did::ExitSave, Did::Stay, Did::SaveFailed]);
        let number = course.return_to_main_menu(&mut game).unwrap().unwrap();
        assert_eq!(course.returned(number, || true), Returned::Waiting);
    }

    #[test]
    fn a_restart_no_return_asked_for_is_the_games_own() {
        let course = Course::new();
        let mut game = Recorded::in_game(&course);
        assert_eq!(course.restart(&mut game), 7);
        assert_eq!(game.did, [Did::Restart]);
    }

    #[test]
    fn a_new_game_from_a_session_returns_then_starts_at_the_new_main_menu() {
        let course = Course::new();
        let mut game = Recorded::in_game(&course);
        let number = course.new_game_from_session(&mut game, *b"FRA", || Ok(())).unwrap();
        assert_eq!(course.last(), number);
        assert_eq!(course.place(At::Game, &mut game), Place::Game);
        assert_eq!(game.return_now(), 0);
        assert_eq!(game.did, [Did::AskReturn, Did::End(End::NewGame), Did::ReturnInProcess, Did::CarryOut(Action::Start(*b"FRA"))]);
        assert_eq!(course.outcome(number, game.reads), Launch::Waiting);
    }

    #[test]
    fn a_new_game_and_a_return_asked_for_together_are_both_answered() {
        let course = Course::new();
        let mut game = Recorded::in_game(&course);
        let returned = course.return_to_main_menu(&mut game).unwrap().unwrap();
        let started = course.new_game_from_session(&mut game, *b"FRA", || Ok(())).unwrap();
        assert_ne!(returned, started);
        game.return_now();
        // One return, for the new game, which follows it.
        assert_eq!(game.did, [Did::AskReturn, Did::AskReturn, Did::End(End::NewGame), Did::ReturnInProcess, Did::CarryOut(Action::Start(*b"FRA"))]);
        assert_eq!(course.returned(returned, || true), Returned::Done);
        assert_eq!(course.outcome(started, game.reads), Launch::Waiting);
    }

    #[test]
    fn a_new_game_from_an_ironman_session_whose_exit_save_fails_keeps_the_session_and_says_why() {
        let course = Course::new();
        let mut game = Recorded { ironman: true, save_fails: true, ..Recorded::in_game(&course) };
        let number = course.new_game_from_session(&mut game, *b"FRA", || Ok(())).unwrap();
        assert_eq!(game.return_now(), 0);
        // Never told it ended, as it goes on; the game neither restarts nor starts.
        assert_eq!(game.did, [Did::AskReturn, Did::ExitSave, Did::Stay, Did::SaveFailed]);
        assert_eq!(course.outcome(number, game.reads), Launch::Failed("no room on the disk".into()));
        assert!(!course.game_frame(&mut game));
        // At a main menu later on, it does not run.
        game.steps = Some(OPEN);
        course.main_menu_frame(&mut game);
        assert_eq!(game.did.len(), 4);
    }

    #[test]
    fn a_new_game_from_a_session_needs_the_in_process_return_and_the_country() {
        let course = Course::new();
        let mut game = Recorded { rules: RESTART, ..Recorded::in_game(&course) };
        let refused = course.new_game_from_session(&mut game, *b"FRA", || fail(Status::NotFound, "no such country"));
        assert_eq!(refused.unwrap_err().status, Status::Unavailable);
        game.rules = ALL;
        let refused = course.new_game_from_session(&mut game, *b"XXX", || fail(Status::NotFound, "no such country"));
        assert_eq!(refused.unwrap_err().status, Status::NotFound);
        // Nothing asked of the game, nothing to start.
        assert_eq!(game.did, []);
        assert_eq!(course.last(), 0);
        // No game going on, as after the session ended.
        game.in_game = false;
        assert_eq!(course.new_game_from_session(&mut game, *b"FRA", || Ok(())).unwrap_err().status, Status::NotInGame);
        assert_eq!(course.return_to_main_menu(&mut game).unwrap_err().status, Status::NotInGame);
    }

    #[test]
    fn a_new_game_from_a_session_with_restarting_turned_on_since_restarts_the_game() {
        let course = Course::new();
        let mut game = Recorded::in_game(&course);
        let number = course.new_game_from_session(&mut game, *b"FRA", || Ok(())).unwrap();
        game.rules = RESTART;
        assert_eq!(game.return_now(), 7);
        // Left for the main menu, and answered as the process ends.
        assert_eq!(game.did, [Did::AskReturn, Did::End(End::Return), Did::Restart]);
        assert_eq!(course.outcome(number, game.reads), Launch::Asked);
    }
}
