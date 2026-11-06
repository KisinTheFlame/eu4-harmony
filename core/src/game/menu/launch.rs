//! Starting a game (docs/research/main-menu.md): a save loaded or a new game
//! started from the main menu, or a save loaded in a session's place, as a
//! whole. [`Launches`] keeps the last one asked for and takes it through its
//! course, frame by frame, over [`Menu`], what each step does in the game:
//! it runs at the main menu's next frame ([`Launches::main_menu_frame`]), an
//! in-game load at the next game frame ([`Launches::game_frame`]); then it is
//! followed as the game takes it up, until it fails or the game takes over.
//! The hooks and capabilities only hand over what they hold (`super`'s
//! `InMenu`); the tests record the game instead.
use super::{Launch, Place};
use crate::{game::Tag, sessions::At};
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

/// What the steps of starting a game do: the game's own in the game
/// (`super`'s `InMenu`), recorded in the tests.
pub(super) trait Menu {
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
    /// The game's in-game load of the save `name`, in the session's place.
    fn load_in_game(&mut self, name: &str) -> Result<(), String>;
    fn reads(&self) -> Reads;
    fn now(&self) -> Instant;
}

enum Stage {
    /// Runs at the main menu's next frame, or an in-game load at the next
    /// game frame.
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
    /// Counts the attempts, so that one waiting is not told what came of
    /// another.
    number: u64,
    action: Action,
    stage: Stage,
}

/// The last load or start asked for. Each step takes it, decides, and lets
/// it go while the game is called, which may call back into the framework.
pub(super) struct Launches(Mutex<Option<Attempt>>);

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

impl Launches {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn with<T>(&self, f: impl FnOnce(&mut Option<Attempt>) -> T) -> T {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Asks for `action`, to run at the main menu's next frame, or at the
    /// next game frame for an in-game load; answers its number.
    pub fn ask(&self, action: Action) -> u64 {
        self.with(|a| {
            let number = a.as_ref().map_or(1, |a| a.number + 1);
            *a = Some(Attempt { number, action, stage: Stage::Asked });
            number
        })
    }

    /// The last load or start asked for.
    pub fn last(&self) -> u64 {
        self.with(|a| a.as_ref().map_or(0, |a| a.number))
    }

    fn asked(&self, of: impl Fn(&Action) -> bool) -> bool {
        self.with(|a| matches!(a, Some(Attempt { action, stage: Stage::Asked, .. }) if of(action)))
    }

    /// Whether remote control asked for a new game from the session, which
    /// the return the map manager's frame acts on next leaves the session for.
    pub fn new_game_asked(&self) -> bool {
        self.asked(|a| matches!(a, Action::Start(_)))
    }

    /// Whether a load remote control asked for from a session has not run yet.
    pub fn loading_in_game(&self) -> bool {
        self.asked(|a| matches!(a, Action::LoadInGame(_)))
    }

    /// The map manager's frame acted on a return and the session stays, for
    /// `why`: a new game asked for from the session does not come either.
    pub fn start_failed(&self, why: &str) {
        self.with(|a| {
            if let Some(a @ Attempt { action: Action::Start(_), stage: Stage::Asked, .. }) = a {
                a.stage = Stage::Failed(why.to_string());
            }
        });
    }

    /// Whether a game is being started from the main menu: remote control
    /// asked for one that has not run yet, a save is being read, or the
    /// game's own state says so (see [`Steps`]), whoever started it. A main
    /// menu that no longer reads as one, as when a game takes over within the
    /// frame, is not open either.
    pub fn starting(&self, menu: &mut impl Menu) -> bool {
        self.asked(|_| true) || menu.reads().reading || menu.steps().map_or(true, |s| s.starting())
    }

    /// Where the game is, at the safe point `at` (`menu` being the main
    /// menu there, if it is).
    pub fn place(&self, at: At, menu: &mut impl Menu) -> Place {
        match at {
            At::Game if self.loading_in_game() => Place::Starting,
            At::Game => Place::Game,
            At::MainMenu(_) if !self.starting(menu) => Place::MainMenu,
            At::MainMenu(_) | At::Elsewhere => Place::Starting,
        }
    }

    /// What came of the load or start `number` ([`Launches::last`]), at a
    /// safe point outside a game's frame. A load fails when the game read a
    /// save since it ran and could not read it: its loader said so. The game
    /// goes on and starts a game from what it read (seen in the game with a
    /// save whose game state was damaged).
    pub fn outcome(&self, number: u64, reads: Reads) -> Launch {
        let unread = |loads: usize| reads.loads > loads && !reads.reading && !reads.last_read;
        self.with(|a| match a {
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
    pub fn game_frame(&self, menu: &mut impl Menu) -> bool {
        let Some((number, name)) = self.with(|a| match a {
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
        let loads = menu.reads().loads;
        let stage = match menu.load_in_game(&name) {
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

    /// At the end of the main menu's frame: runs the load or start asked
    /// for, then follows the game taking it up, and gives it up when the main
    /// menu shows with nothing started from it, or when the game takes no
    /// step for [`QUIET`].
    pub fn main_menu_frame(&self, menu: &mut impl Menu) {
        enum Now {
            Run(Action),
            Follow(Action, Steps, Instant, usize),
        }
        let Some((number, now)) = self.with(|a| match a {
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
            Now::Run(action) => run(menu, &action),
            Now::Follow(action, seen, since, loads) => follow(menu, &action, seen, since, loads),
        };
        self.settle(number, stage);
    }

    /// Where the attempt `number` got to, unless another was asked for
    /// meanwhile.
    fn settle(&self, number: u64, stage: Stage) {
        self.with(|a| {
            if let Some(a) = a.as_mut().filter(|a| a.number == number) {
                a.stage = stage;
            }
        });
    }
}

/// Runs a load or start in the main menu's frame.
fn run(menu: &mut impl Menu, action: &Action) -> Stage {
    let result = menu.first_page().and_then(|()| match menu.carry_out(action) {
        Ok(()) => menu.steps(),
        Err(why) => {
            menu.back_to_first_page();
            Err(why)
        }
    });
    match result {
        Ok(seen) => {
            log!("menu: {} started", action.describe());
            Stage::Taken { seen, since: menu.now(), loads: seen.loads }
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
fn follow(menu: &mut impl Menu, action: &Action, seen: Steps, since: Instant, loads: usize) -> Stage {
    let why = match menu.steps() {
        // Not the main menu the framework knows: nothing is touched.
        Err(why) => why,
        Ok(now) => match judge(&seen, &now, menu.reads().reading, menu.now().saturating_duration_since(since)) {
            Judged::Step => return Stage::Taken { seen: now, since: menu.now(), loads },
            Judged::Wait => return Stage::Taken { seen, since, loads },
            Judged::Open => {
                menu.not_continuing();
                "the game did not start it: the main menu is open".to_string()
            }
            Judged::Quiet => {
                menu.back_to_first_page();
                format!("the game did not start it: it took no step towards it for {} s", QUIET.as_secs())
            }
        },
    };
    log!("menu: {} failed: {why}", action.describe());
    Stage::Failed(why)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// What the game was asked to do, in order.
    #[derive(Debug, PartialEq, Eq)]
    enum Did {
        CarryOut(Action),
        BackToFirstPage,
        NotContinuing,
        LoadInGame(String),
    }

    /// The main menu on its first page, nothing started from it.
    const OPEN: Steps = Steps { loads: 2, page: 0, started: false, accepted: false, launch: false, switching: false, idler: true };
    /// The single player setup entered with its start made.
    const STARTED: Steps = Steps { page: 3, started: true, ..OPEN };

    /// The game, recorded: what its main menu shows, what fails when told
    /// to, and a clock moved by hand.
    struct Recorded {
        did: Vec<Did>,
        /// What the main menu shows; None when it is not the main menu the
        /// framework knows.
        steps: Option<Steps>,
        /// What the main menu shows once a load or start was carried out.
        after: Steps,
        off_first_page: bool,
        fails: Option<&'static str>,
        reads: Reads,
        start: Instant,
        clock: Duration,
    }

    impl Recorded {
        fn new() -> Self {
            Self {
                did: Vec::new(),
                steps: Some(OPEN),
                after: STARTED,
                off_first_page: false,
                fails: None,
                reads: Reads { loads: 2, reading: false, last_read: true },
                start: Instant::now(),
                clock: Duration::ZERO,
            }
        }
    }

    impl Menu for Recorded {
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
        fn load_in_game(&mut self, name: &str) -> Result<(), String> {
            self.did.push(Did::LoadInGame(name.into()));
            self.fails.map_or(Ok(()), |why| Err(why.into()))
        }
        fn reads(&self) -> Reads {
            self.reads
        }
        fn now(&self) -> Instant {
            self.start + self.clock
        }
    }

    fn load() -> Action {
        Action::Load("save games/Aragon.eu4".into(), Tag(0x5a))
    }

    #[test]
    fn a_load_from_the_main_menu_runs_at_its_next_frame_and_waits_for_the_game() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let number = launches.ask(load());
        assert_eq!(launches.outcome(number, game.reads), Launch::Asked);
        assert!(launches.starting(&mut game));
        launches.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(load())]);
        assert_eq!(launches.outcome(number, game.reads), Launch::Waiting);
        // The game reads the save, and reads it whole.
        game.reads = Reads { loads: 3, reading: true, last_read: false };
        game.steps = Some(Steps { loads: 3, ..STARTED });
        launches.main_menu_frame(&mut game);
        game.reads = Reads { loads: 3, reading: false, last_read: true };
        assert_eq!(launches.outcome(number, game.reads), Launch::Waiting);
        // Run once only.
        assert_eq!(game.did, [Did::CarryOut(load())]);
    }

    #[test]
    fn a_load_or_start_the_game_took_over_is_not_judged_by_the_main_menu_after_a_return() {
        for action in [Action::Start(*b"FRA"), load()] {
            let (launches, mut game) = (Launches::new(), Recorded::new());
            let number = launches.ask(action.clone());
            launches.main_menu_frame(&mut game);
            // The game takes it over: a game's frame shows, which it does not take.
            assert!(!launches.game_frame(&mut game));
            // Back at a new main menu, nothing started from it.
            game.steps = Some(OPEN);
            launches.main_menu_frame(&mut game);
            assert_eq!(game.did, [Did::CarryOut(action.clone())]);
            assert_eq!(launches.outcome(number, game.reads), Launch::Waiting);
            assert!(!launches.starting(&mut game));
            // A save the game could not read still fails a load.
            game.reads = Reads { loads: 3, reading: false, last_read: false };
            let unread = if matches!(action, Action::Load(..)) { Launch::Failed(UNREAD.into()) } else { Launch::Waiting };
            assert_eq!(launches.outcome(number, game.reads), unread);
        }
    }

    #[test]
    fn a_load_or_start_that_does_not_go_through_puts_the_main_menu_back() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        game.fails = Some("the single player setup did not take the country");
        let number = launches.ask(Action::Start(*b"FRA"));
        launches.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"FRA")), Did::BackToFirstPage]);
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed("the single player setup did not take the country".into()));
        assert!(!launches.starting(&mut game));
    }

    #[test]
    fn a_main_menu_that_left_its_first_page_or_is_not_known_is_left_untouched() {
        for off_first_page in [true, false] {
            let (launches, mut game) = (Launches::new(), Recorded::new());
            game.off_first_page = off_first_page;
            game.steps = if off_first_page { Some(OPEN) } else { None };
            let why = game.first_page().unwrap_err();
            let number = launches.ask(load());
            launches.main_menu_frame(&mut game);
            assert_eq!(game.did, []);
            assert_eq!(launches.outcome(number, game.reads), Launch::Failed(why));
        }
    }

    #[test]
    fn a_start_the_game_takes_no_step_towards_is_given_up_after_a_quiet_spell() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let number = launches.ask(Action::Start(*b"CAS"));
        launches.main_menu_frame(&mut game);
        // A step a while in waits again from then on.
        game.clock = Duration::from_secs(20);
        game.steps = Some(Steps { accepted: true, ..STARTED });
        launches.main_menu_frame(&mut game);
        game.clock = Duration::from_secs(45);
        launches.main_menu_frame(&mut game);
        assert_eq!(launches.outcome(number, game.reads), Launch::Waiting);
        game.clock = Duration::from_secs(20) + QUIET;
        launches.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"CAS")), Did::BackToFirstPage]);
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed("the game did not start it: it took no step towards it for 30 s".into()));
    }

    #[test]
    fn a_setup_the_player_left_gives_the_start_up_and_keeps_its_page() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let number = launches.ask(Action::Start(*b"CAS"));
        launches.main_menu_frame(&mut game);
        game.steps = Some(Steps { page: 1, ..OPEN });
        launches.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"CAS")), Did::NotContinuing]);
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed("the game did not start it: the main menu is open".into()));
        // Given up once: the frames after leave it.
        launches.main_menu_frame(&mut game);
        assert_eq!(game.did.len(), 2);
    }

    #[test]
    fn a_save_the_game_could_not_read_fails_its_load_though_a_game_starts() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let number = launches.ask(load());
        launches.main_menu_frame(&mut game);
        game.reads = Reads { loads: 3, reading: false, last_read: false };
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed(UNREAD.into()));
        // A new game reads no save of its own: whatever was read last is not its.
        let number = launches.ask(Action::Start(*b"CAS"));
        game.reads = Reads { loads: 2, reading: false, last_read: true };
        launches.main_menu_frame(&mut game);
        game.reads = Reads { loads: 3, reading: false, last_read: false };
        assert_eq!(launches.outcome(number, game.reads), Launch::Waiting);
    }

    #[test]
    fn an_in_game_load_runs_at_the_next_game_frame_only() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let number = launches.ask(Action::LoadInGame("Aragon".into()));
        assert!(launches.loading_in_game());
        assert_eq!(launches.place(At::Game, &mut game), Place::Starting);
        assert!(launches.game_frame(&mut game));
        assert!(!launches.game_frame(&mut game));
        assert_eq!(game.did, [Did::LoadInGame("Aragon".into())]);
        assert_eq!(launches.place(At::Game, &mut game), Place::Game);
        assert_eq!(launches.outcome(number, game.reads), Launch::Waiting);
        // The game's load read the save, but could not.
        game.reads = Reads { loads: 3, reading: false, last_read: false };
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed(UNREAD.into()));
    }

    #[test]
    fn an_in_game_load_the_game_refuses_fails_with_its_reason() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        game.fails = Some("the game found no such save");
        let number = launches.ask(Action::LoadInGame("Aragon".into()));
        assert!(launches.game_frame(&mut game));
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed("the game found no such save".into()));
    }

    #[test]
    fn an_in_game_load_asked_in_a_session_that_ended_first_fails_at_the_main_menu() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let number = launches.ask(Action::LoadInGame("Aragon".into()));
        launches.main_menu_frame(&mut game);
        assert!(!launches.game_frame(&mut game));
        assert_eq!(game.did, []);
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed("the session ended before the load could run".into()));
    }

    #[test]
    fn a_new_game_from_a_session_starts_at_the_new_main_menu_or_fails_with_the_return() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let number = launches.ask(Action::Start(*b"FRA"));
        assert!(launches.new_game_asked());
        // The return's ironman exit save failed: the session stays.
        launches.start_failed("the game could not write the ironman exit save");
        assert!(!launches.new_game_asked());
        assert_eq!(launches.outcome(number, game.reads), Launch::Failed("the game could not write the ironman exit save".into()));
        // Returned in process: the new main menu's first frame starts it.
        let number = launches.ask(Action::Start(*b"FRA"));
        launches.main_menu_frame(&mut game);
        launches.start_failed("too late");
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"FRA"))]);
        assert_eq!(launches.outcome(number, game.reads), Launch::Waiting);
    }

    #[test]
    fn one_waiting_hears_only_of_its_own_load_or_start() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        let first = launches.ask(load());
        let second = launches.ask(Action::Start(*b"CAS"));
        assert_eq!(launches.last(), second);
        assert_eq!(launches.outcome(first, game.reads), Launch::Failed("another load or start was asked for".into()));
        launches.main_menu_frame(&mut game);
        assert_eq!(game.did, [Did::CarryOut(Action::Start(*b"CAS"))]);
    }

    #[test]
    fn the_main_menu_is_open_unless_something_is_being_started_from_it() {
        let (launches, mut game) = (Launches::new(), Recorded::new());
        assert_eq!(launches.place(At::MainMenu(1), &mut game), Place::MainMenu);
        assert_eq!(launches.place(At::Elsewhere, &mut game), Place::Starting);
        // A save being read, whoever loads it.
        game.reads.reading = true;
        assert_eq!(launches.place(At::MainMenu(1), &mut game), Place::Starting);
        game.reads.reading = false;
        // The player's own start, or a main menu that reads as none.
        game.steps = Some(STARTED);
        assert!(launches.starting(&mut game));
        game.steps = None;
        assert!(launches.starting(&mut game));
        game.steps = Some(OPEN);
        launches.ask(load());
        assert!(launches.starting(&mut game));
    }
}
