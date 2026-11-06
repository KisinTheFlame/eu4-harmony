//! Leaving a session (docs/sessions.md): for the main menu, for a save, or
//! for a new game. [`plan`] lays out the steps from where the session goes,
//! whether it is ironman and the framework settings; the player's ways out
//! and remote control's take the same steps.
//!
//! An in-game load (GLOSSARY) is the game's own, `0x5d0580`, which every
//! in-game load goes through: the load question's, the game console's and
//! remote control's. The framework replaces it to take the steps around it.
//! When the settings ask for it, the ironman exit save is written first;
//! only once it was does the session end, extensions hearing it, and the
//! load go on. The world reset inside the game's load never writes one of
//! its own, which the game would write whatever came of it
//! (ADR in-game-load-in-ironman, docs/research/main-menu.md).
//!
//! In ironman the game's load button is turned on, and its tooltip no
//! longer says that ironman cannot load; when the ironman exit save comes
//! first, the load question says so instead of warning that the current
//! game is lost.
//!
//! Returning to the main menu goes through the map manager's frame, which
//! writes the ironman exit save and asks for the game's restart. The
//! framework takes the session's end and the ironman exit save there, and,
//! with "return without restarting" on, answers the restart with an
//! in-process return (ADR return-to-the-main-menu-in-process): the session is
//! taken down as the in-game load takes it down, its interface, then its world
//! reset, here with the game's history; then a new main menu, made as startup
//! makes it after its own reset, takes the application over by its idler
//! switch, which lets the map manager go. The loading screen it presents as
//! it takes over, as at startup, is held back: the still notice stays until
//! its first frame. The single player setup's back
//! button, which restarts the game too, goes back the same way.
//!
//! What the game's world reset and the map manager's destructor leave of a
//! session, the framework's repairs put right (`crate::repairs`).
//!
//! The steps are taken here, as a whole, over [`Game`], what each does in
//! the game: [`run_load`] at an in-game load, and a return across the two
//! hooks it passes, [`run_return`] in the map manager's frame up to the
//! restart, whose rest [`Returns`] keeps for [`run_restart`]. The hooks only
//! hand over what they hold ([`InGame`]); the tests record the game instead.
use crate::{
    game_string,
    hooks::HookFrame,
    memory::Memory,
    notice::Message,
    objects::{
        application,
        campaign::{classes, globals, load_request, map_manager as manager_fields, world},
        gui,
    },
    points::{Function, Point},
    repairs,
    sessions::{self, End},
    unicode::game_text,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Mutex,
};

/// `(request) -> u32`: the in-game load (see [`Loaded`]).
const IN_GAME_LOAD: usize = 0x5d0580;
/// `(map manager, save)`: the world reset of an in-game load; with `save`
/// set it writes the ironman exit save first.
const RESET: usize = 0x826c20;
/// `()`: the ironman exit save, of an ironman session only.
const EXIT_SAVE: usize = 0x5cc160;
/// `(save request) -> u32`: writes a save, 0 when it was written.
const WRITE_SAVE: usize = 0x5cbde0;
/// `(map manager)`: what the load question does once the load returned.
const AFTER_LOAD: usize = 0x824810;
/// `()`: what the load question puts in [`load_request::CONTEXT`].
const LOAD_CONTEXT: usize = 0x8bf160;
/// Where the load question looks its text up, `CONFIRMLOADSAVETEXT` with
/// the save's name, which `rdi` points at; and what follows.
const QUESTION_TEXT: usize = 0x11be26e;
const QUESTION_TEXT_DONE: usize = 0x11be273;
/// The world reset's `call <ironman exit save>`, made when its second
/// argument is set; and what follows. That argument also spares the reset
/// the game's history, which a save brings anyway.
const RESET_EXIT_SAVE: usize = 0x826e4f;
const RESET_EXIT_SAVE_DONE: usize = 0x826e54;
/// The save the game makes of an ironman session as it sets the session up,
/// in its interface's rebuild (`0x81af10`), `call <ironman exit save>`; and
/// what follows.
const SETUP_SAVE: usize = 0x81d8c8;
const SETUP_SAVE_DONE: usize = 0x81d8cd;
/// The map manager's frame returning to the main menu: its `call <ironman
/// exit save>`, and what follows.
const RETURN_EXIT_SAVE: usize = 0x815cc3;
const RETURN_EXIT_SAVE_DONE: usize = 0x815cc8;
/// `(application, extra argument)`: asks for the restart, which starts
/// `eu4.exe` again at the end of the main loop; only the return calls it.
const RESTART: usize = 0x14c2500;
/// `(memory, application::MAIN_MENU_ARGS, application)`: makes the main
/// menu, of this size.
const MAIN_MENU: usize = 0x10d23c0;
const MAIN_MENU_SIZE: usize = 0xb48;
/// `(application, &idler, keep)`: the application's idler switch, at its
/// next frame; with `keep` clear it lets the idler it leaves go.
const SWITCH_IDLER: usize = 0x14c2530;
/// `(main menu)`: the main menu taking over at the idler switch, its method
/// at `+0x30`. As at startup, it presents a frame of the loading screen, a
/// painting over the whole screen (docs/research/main-menu.md).
const TAKE_OVER: usize = 0x10d5bc0;
/// `(task)`: waits for a task of the game's to be done.
const WAIT_TASK: usize = 0x17139c0;
/// `(application)`: the continue information from the newest save.
const CONTINUE: usize = 0x212810;
/// `(application, without history)`: the world reset as a new start finds
/// it; it reads the game's history unless told not to.
const RESET_GAME: usize = 0x210000;
/// The single player setup's back button, where it asks for the restart
/// itself (`mov word ptr [application + 0x83], 0x101`, the application in
/// `rbx`); and what follows.
const SETUP_BACK_RESTART: usize = 0x111dfd7;
const SETUP_BACK_RESTART_DONE: usize = 0x111dfe0;

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

/// What the steps of leaving a session do: the game's own in the game
/// ([`InGame`]), recorded in the tests.
trait Game {
    /// Writes the ironman exit save, or says why it was not written.
    fn exit_save(&mut self) -> Result<(), String>;
    /// The session ends, for `end` (`crate::sessions::ending`).
    fn end(&mut self, end: End);
    /// The game's in-game load, and what it answered.
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

/// What a return to the main menu still has to take at the restart the map
/// manager's frame asks for next: its steps from there, or why the session
/// stays.
type Rest = Result<Vec<Step>, String>;

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
        // and it goes at the application's switch. A start that follows is
        // made at the new main menu's first frame (`super::main_menu_frame`).
        Some(Ok(rest)) if rest.first() == Some(&Step::Return) && game.return_in_process().is_ok() => (0, Some(Ok(()))),
        _ => (game.restart(), None),
    }
}

/// The returns to the main menu remote control asks for, and the rest of the
/// one the map manager's frame acted on, for the restart it asks for next.
struct Returns {
    /// How many remote control asked for.
    asked: u64,
    /// What came of the last one the map manager's frame acted on, whoever
    /// asked for it: how many remote control had asked for by then, and its
    /// outcome.
    last: Option<(u64, Result<(), String>)>,
    rest: Option<Rest>,
}

impl Returns {
    /// Numbers a return remote control asks for. One asked for while another
    /// waits for the map manager's frame to act on it is that same return,
    /// and takes its number, so that both are answered.
    fn ask(&mut self) -> u64 {
        if self.last.as_ref().map_or(0, |(number, _)| *number) == self.asked {
            self.asked += 1;
        }
        self.asked
    }

    /// The map manager's frame acted on a return, whoever asked for it.
    fn acted_on(&mut self, outcome: Result<(), String>) {
        self.last = Some((self.asked, outcome));
    }

    /// What came of the return `number`, once the map manager's frame acted
    /// on it.
    fn outcome(&self, number: u64) -> Option<Result<(), String>> {
        self.last.as_ref().filter(|(n, _)| *n == number).map(|(_, outcome)| outcome.clone())
    }
}

static RETURNS: Mutex<Returns> = Mutex::new(Returns { asked: 0, last: None, rest: None });

fn returns<T>(f: impl FnOnce(&mut Returns) -> T) -> T {
    f(&mut RETURNS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Numbers a return to the main menu remote control asked for.
pub(super) fn ask_return() -> u64 {
    returns(Returns::ask)
}

/// What came of the return remote control asked for as `number`, once the
/// map manager's frame acted on it: an in-process return was set off, or the
/// session stays, and why.
pub(super) fn return_outcome(number: u64) -> Option<Result<(), String>> {
    returns(|r| r.outcome(number))
}

/// Whether the session is ironman.
fn ironman(m: &Memory) -> bool {
    m.ptr(m.at(globals::WORLD)).and_then(|w| if w == 0 { Ok(0) } else { m.u8(w + world::IRONMAN) }).is_ok_and(|b| b != 0)
}

/// How many saves the game wrote, and what the last one answered.
static WRITTEN: Mutex<(u64, u32)> = Mutex::new((0, 0));

extern "C" fn save_written(frame: *mut HookFrame) {
    crate::guarded("save written", || {
        let h = unsafe { &*frame };
        let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
        *written = (written.0 + 1, if h.fault == 0 { h.result as u32 } else { u32::MAX });
    });
}

/// Writes the ironman exit save, and says why when it was not written.
fn exit_save(m: &Memory) -> Result<(), String> {
    let before = WRITTEN.lock().unwrap_or_else(|e| e.into_inner()).0;
    m.call(m.at(EXIT_SAVE), &[]).map_err(|e| format!("the ironman exit save faulted: {e}"))?;
    let (count, result) = *WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    match (count > before, result) {
        (true, 0) => Ok(()),
        (true, code) => Err(format!("the game could not write the ironman exit save (it answered {code})")),
        // A cloud save goes another way, which the framework does not see.
        (false, _) => {
            log!("leave: the ironman exit save wrote nothing the framework sees");
            Ok(())
        }
    }
}

/// The world reset inside an in-game load writes no ironman exit save of its
/// own while this is set, a byte: the steps around the load decide.
static QUIET_RESET: AtomicBool = AtomicBool::new(false);
/// The game's ironman exit save, which the stubs call.
static EXIT_SAVE_AT: AtomicUsize = AtomicUsize::new(0);

/// While an in-game load runs, the save the game makes as it sets an ironman
/// session up waits: the load still holds the file it read, which that save
/// would replace, and the replacing fails (`SETUP_SAVE`). Set when it waited.
static LOADING_IN_GAME: AtomicBool = AtomicBool::new(false);
static SETUP_SAVE_WAITED: AtomicBool = AtomicBool::new(false);

crate::resume!(SETUP_SAVE_RESUME: SETUP_SAVE_NEXT = SETUP_SAVE_DONE);

/// Replaces the session setup's `call <ironman exit save>`: made unless an
/// in-game load runs, which makes it once it has let the file go.
#[unsafe(naked)]
extern "C" fn setup_save_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rip + {loading}], 0",
        "je 2f",
        "mov byte ptr [rip + {waited}], 1",
        "jmp qword ptr [rip + {next}]",
        "2:",
        "call qword ptr [rip + {save}]",
        "jmp qword ptr [rip + {next}]",
        loading = sym LOADING_IN_GAME,
        waited = sym SETUP_SAVE_WAITED,
        save = sym EXIT_SAVE_AT,
        next = sym SETUP_SAVE_NEXT,
    );
}

crate::resume!(RESET_RESUME: RESET_NEXT = RESET_EXIT_SAVE_DONE);

/// Replaces the world reset's `call <ironman exit save>`: made unless
/// [`QUIET_RESET`] is set.
#[unsafe(naked)]
extern "C" fn reset_exit_save_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rip + {quiet}], 0",
        "jne 2f",
        "call qword ptr [rip + {save}]",
        "2:",
        "jmp qword ptr [rip + {next}]",
        quiet = sym QUIET_RESET,
        save = sym EXIT_SAVE_AT,
        next = sym RESET_NEXT,
    );
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
    fn of(code: u32) -> Self {
        match code {
            0 => Self::Read,
            1 => Self::Nothing,
            2 => Self::NoFile,
            3 => Self::Refused,
            n => Self::Other(n),
        }
    }
}

type InGameLoad = unsafe extern "C" fn(usize) -> u32;
static ORIGINAL: Function<InGameLoad> = Function::new();
/// Why the framework stopped the last in-game load, if it did.
static STOPPED: Mutex<Option<String>> = Mutex::new(None);
static FAILED_SAVES: AtomicU64 = AtomicU64::new(0);

/// The game's own steps of leaving a session, at the hook that takes them.
struct InGame<'a> {
    m: &'a Memory,
    /// The in-game load's request, at the in-game load.
    request: usize,
    /// The application and the restart's argument, at the restart.
    app: usize,
    argument: usize,
}

impl Game for InGame<'_> {
    fn exit_save(&mut self) -> Result<(), String> {
        exit_save(self.m)
    }

    fn end(&mut self, end: End) {
        sessions::ending(end);
    }

    fn load(&mut self) -> u32 {
        let m = self.m;
        QUIET_RESET.store(true, Ordering::Relaxed);
        LOADING_IN_GAME.store(true, Ordering::Relaxed);
        SETUP_SAVE_WAITED.store(false, Ordering::Relaxed);
        let took = Took::begin();
        let code = unsafe { ORIGINAL.get()(self.request) };
        LOADING_IN_GAME.store(false, Ordering::Relaxed);
        QUIET_RESET.store(false, Ordering::Relaxed);
        log!("leave: the in-game load answered {:?}, {}; {}", Loaded::of(code), took.end(), crate::heap::split());
        if SETUP_SAVE_WAITED.swap(false, Ordering::Relaxed) {
            log!("leave: the save the game makes as it sets the loaded ironman session up, now that the load let its file go");
            if let Err(e) = m.call(m.at(EXIT_SAVE), &[]) {
                log!("leave: that save faulted: {e}");
            }
        }
        code
    }

    fn return_in_process(&mut self) -> Result<(), String> {
        match in_process_return(self.m, self.app) {
            Ok(()) => {
                log!("leave: the main menu takes over at the application's next switch; {}", crate::heap::split());
                Ok(())
            }
            Err(why) => {
                log!("leave: no in-process return ({why}); the game restarts");
                Err(why)
            }
        }
    }

    fn restart(&mut self) -> usize {
        unsafe { ORIGINAL_RESTART.get()(self.app, self.argument) }
    }

    fn stay(&mut self) {
        if let Ok(manager) = map_manager(self.m) {
            let _ = self.m.write(manager + manager_fields::RETURN, &[0]);
        }
    }

    fn save_failed(&mut self) {
        let n = FAILED_SAVES.fetch_add(1, Ordering::Relaxed);
        crate::notice::post(&format!("ironman save failed:{n}"), Message::new("EU4HARMONY_IRONMAN_SAVE_FAILED"));
    }
}

/// Every in-game load: the steps [`plan`] gives for it, around the game's own.
unsafe extern "C" fn in_game_load(request: usize) -> u32 {
    let Some(m) = crate::memory().copied() else { return ORIGINAL.get()(request) };
    crate::guarded("in-game load", || {
        crate::framework_ui::still("EU4HARMONY_STILL_LOAD");
        let ironman = ironman(&m);
        let steps = plan(Destination::Save, ironman, Rules::now());
        log!("leave: an in-game load, {}ironman: {steps:?}", if ironman { "" } else { "not " });
        match run_load(&mut InGame { m: &m, request, app: 0, argument: 0 }, steps.unwrap_or_default()) {
            Ok(code) => code,
            Err(why) => {
                log!("leave: the load did not happen: {why}");
                *STOPPED.lock().unwrap_or_else(|e| e.into_inner()) = Some(why);
                1
            }
        }
    })
}

/// The load question's text: the game's, unless the ironman exit save comes
/// before the load, which the framework's says. `name` is the save's name,
/// a C string of game text.
extern "C" fn question_text(out: usize, key: usize, values: usize, name: usize) {
    crate::guarded("load question", || {
        let m = crate::hooked_memory();
        if let Err(e) = crate::game_string::localize_into(&m, out, key, values, 1) {
            log!("leave: the load question's text: {e}");
            return;
        }
        if !(ironman(&m) && Rules::now().save_ironman_before_load) {
            return;
        }
        let saved = (|| {
            let end = (0..1024).find(|&i| m.u8(name + i).is_ok_and(|b| b == 0)).ok_or("the save's name has no end")?;
            let name: String = game_text::chars(&m.bytes(name, end)?).map(|(c, _)| c).collect();
            let words = crate::loc::framework(crate::lang::player(), "EU4HARMONY_CONFIRM_LOAD_IRONMAN", &[("NAME", &name)]);
            game_string::read(&m, out)?;
            unsafe { game_string::truncate(out as *mut u8, 0) };
            game_string::append_checked(&m, out, &game_text::from_str(&words))
        })();
        if let Err(e) = saved {
            log!("leave: the load question keeps the game's text: {e}");
        }
    });
}

crate::resume!(QUESTION_RESUME: QUESTION_NEXT = QUESTION_TEXT_DONE);

/// Replaces the load question's `call <localisation lookup>` for its text.
#[unsafe(naked)]
extern "C" fn question_text_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov r9, rdi", call text),
        "jmp qword ptr [rip + {next}]",
        text = sym question_text,
        next = sym QUESTION_NEXT,
    );
}

/// The session's map manager.
fn map_manager(m: &Memory) -> crate::Result<usize> {
    let world = m.ptr(m.at(globals::WORLD))?;
    let manager = if world == 0 { 0 } else { m.ptr(world + world::MAP_MANAGER)? };
    if manager == 0 || m.ptr(manager)? != m.at(classes::MAP_MANAGER) {
        return Err("no game is going on".into());
    }
    Ok(manager)
}

/// Loads the save `name` (in `save games`, without `.eu4`) in place of the
/// session, as the load question does when the player accepts it; at a game
/// frame, after the map manager's.
pub fn load_in_game(m: &Memory, name: &str) -> Result<(), String> {
    let manager = map_manager(m)?;
    let mut request = [0usize; load_request::SIZE / 8];
    request[(load_request::NAME + game_string::CAPACITY) / 8] = game_string::INLINE;
    let at = request.as_mut_ptr() as usize;
    game_string::append_checked(m, at + load_request::NAME, name.as_bytes())?;
    request[load_request::MAP_MANAGER / 8] = manager;
    let context = m.call(m.at(LOAD_CONTEXT), &[]);
    let loaded = context.and_then(|context| {
        request[load_request::CONTEXT / 8] = context;
        *STOPPED.lock().unwrap_or_else(|e| e.into_inner()) = None;
        m.call(m.at(IN_GAME_LOAD), &[at])
    });
    let after = loaded.is_ok().then(|| m.call(m.at(AFTER_LOAD), &[manager]));
    let destroyed = game_string::destroy_checked(m, at + load_request::NAME);
    let code = loaded?;
    after.transpose()?;
    destroyed?;
    match Loaded::of(code as u32) {
        Loaded::Read => Ok(()),
        Loaded::Nothing => Err(STOPPED.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_else(|| "the game took no save name".into())),
        Loaded::NoFile => Err("the game found no such save".into()),
        Loaded::Refused => Err("the game refused the save's header after it had reset the world".into()),
        Loaded::Other(n) => Err(format!("the game's in-game load answered {n}")),
    }
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

/// In the map manager's frame, where it writes the ironman exit save as it
/// returns to the main menu: the steps up to the restart it asks for next.
extern "C" fn returning() {
    crate::guarded("return to the main menu", || {
        crate::framework_ui::still("EU4HARMONY_STILL_RETURN");
        let m = crate::hooked_memory();
        let rules = Rules::now();
        let to = returning_to(super::new_game_asked(), rules);
        let rest = run_return(&mut InGame { m: &m, request: 0, app: 0, argument: 0 }, plan(to, ironman(&m), rules));
        log!("leave: returning to the main menu{}, then: {rest:?}", if to == Destination::NewGame { " for a new game" } else { "" });
        returns(|r| r.rest = Some(rest));
    });
}

crate::resume!(RETURN_RESUME: RETURN_NEXT = RETURN_EXIT_SAVE_DONE);

/// Replaces the return's `call <ironman exit save>`.
#[unsafe(naked)]
extern "C" fn returning_stub() {
    std::arch::naked_asm!(crate::call_rust!(call returning), "jmp qword ptr [rip + {next}]", returning = sym returning, next = sym RETURN_NEXT,);
}

type Restart = unsafe extern "C" fn(usize, usize) -> usize;
static ORIGINAL_RESTART: Function<Restart> = Function::new();

/// The return's restart: with the rest [`returning`] left, the game
/// restarts, or returns in process, or stays in the session.
unsafe extern "C" fn restart(app: usize, argument: usize) -> usize {
    let rest = returns(|r| r.rest.take());
    let Some(m) = crate::memory().copied() else { return ORIGINAL_RESTART.get()(app, argument) };
    crate::guarded("return to the main menu", || {
        if let Some(Err(why)) = &rest {
            log!("leave: the session stays: {why}");
        }
        let (answer, outcome) = run_restart(&mut InGame { m: &m, request: 0, app, argument }, rest);
        if let Some(outcome) = outcome {
            if let Err(why) = &outcome {
                // A new game asked for from the session does not come either.
                super::start_failed(why);
            }
            returns(|r| r.acted_on(outcome));
        }
        answer
    })
}

/// Takes the session down as the in-game load does before it reads a save,
/// with the game's history, then makes a main menu as startup does and
/// switches the application to it.
fn in_process_return(m: &Memory, app: usize) -> crate::Result<()> {
    let manager = map_manager(m)?;
    let interface = repairs::SessionInterface::before_reset(m, manager)?;
    take_the_interface_down(m, manager)?;
    // The in-game load's world reset, here with the history, so the main
    // menu is made on a world as a new start has it, as at startup. With its
    // second argument clear it writes no ironman exit save, which was
    // written already. It still uses the session's interface, which goes
    // with the map manager.
    let took = Took::begin();
    m.call(m.at(RESET), &[manager, 0])?;
    log!("leave: the world reset with the history, {}", took.end());
    sessions::world_reset(m);
    interface.let_go_with_map_manager(m)?;
    main_menu(m, app)
}

/// Takes down what of the session's interface the in-game load takes down
/// before its world reset: among it the diplomatic message icons, whose
/// windows stay shown otherwise, drawing a sprite whose texture the other
/// message icons let go (docs/research/main-menu.md). The game's in-game load
/// does that before it looks at its save's name, and given none stops there
/// and answers 1 (`0x5d06c4`): so it is called with no name.
fn take_the_interface_down(m: &Memory, manager: usize) -> crate::Result<()> {
    let mut request = [0usize; load_request::SIZE / 8];
    request[(load_request::NAME + game_string::CAPACITY) / 8] = game_string::INLINE;
    request[load_request::MAP_MANAGER / 8] = manager;
    let code = m.call(ORIGINAL.get() as usize, &[request.as_mut_ptr() as usize])?;
    match Loaded::of(code as u32) {
        Loaded::Nothing => Ok(()),
        other => Err(format!("the in-game load without a save answered {other:?}")),
    }
}

/// Makes a main menu as startup does, switches the application to it at its
/// next frame, which lets the idler it leaves go, and reads the continue
/// information again.
fn main_menu(m: &Memory, app: usize) -> crate::Result<()> {
    let (first, second) = (m.ptr(app + application::MAIN_MENU_ARGS.0)?, m.ptr(app + application::MAIN_MENU_ARGS.1)?);
    let memory = crate::game_heap::new(m, MAIN_MENU_SIZE)?;
    if memory == 0 {
        return Err("no memory for the main menu".into());
    }
    m.call(m.at(MAIN_MENU), &[memory, first, second, app])?;
    let mut idler = memory;
    m.call(m.at(SWITCH_IDLER), &[app, &mut idler as *mut usize as usize, 0])?;
    RETURNED.store(true, Ordering::Relaxed);
    // The continue box offers the save the game wrote last, as after a restart.
    let task = m.ptr(app + application::CONTINUE_TASK)?;
    if task != 0 {
        m.call(m.at(WAIT_TASK), &[task])?;
    }
    m.call(m.at(CONTINUE), &[app])?;
    Ok(())
}

/// Set from when [`main_menu`] asked for the switch to the main menu it made
/// until that main menu took over.
static RETURNED: AtomicBool = AtomicBool::new(false);

/// As a main menu takes over: one an in-process return made presents the
/// loading screen as at startup, which is held back, so the still notice
/// stays until the main menu's first frame.
extern "C" fn taking_over(_frame: *mut HookFrame) {
    crate::guarded("main menu taking over", || {
        if RETURNED.load(Ordering::Relaxed) {
            crate::framework_ui::hold();
        }
    });
}

extern "C" fn took_over(_frame: *mut HookFrame) {
    crate::guarded("main menu taking over", || {
        if RETURNED.swap(false, Ordering::Relaxed) {
            log!("leave: the main menu took over, {} of its presents held back", crate::framework_ui::let_go());
        }
    });
}

/// The single player setup's back button, about to ask for the restart:
/// with "return without restarting" on, the world is reset with the history
/// instead, as the setup may have changed it, and a new main menu takes
/// over; answers whether it did.
extern "C" fn setup_back(app: usize) -> bool {
    crate::guarded("single player setup back", || {
        if !Rules::now().return_without_restart {
            return false;
        }
        crate::framework_ui::still("EU4HARMONY_STILL_RETURN");
        let m = crate::hooked_memory();
        let took = Took::begin();
        let back = m.call(m.at(RESET_GAME), &[app, 0]).and_then(|_| {
            log!("leave: the single player setup was left; the world reset with the history, {}", took.end());
            sessions::world_reset(&m);
            main_menu(&m, app)
        });
        match back {
            Ok(()) => true,
            Err(why) => {
                log!("leave: no in-process return from the single player setup ({why}); the game restarts");
                false
            }
        }
    })
}

crate::resume!(SETUP_BACK_RESUME: SETUP_BACK_NEXT = SETUP_BACK_RESTART_DONE);

/// Replaces the setup's back button's request for the restart: made unless
/// [`setup_back`] went back in process.
#[unsafe(naked)]
extern "C" fn setup_back_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rbx", call back, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "mov word ptr [rbx + 0x83], 0x101",
        "2:",
        "jmp qword ptr [rip + {next}]",
        back = sym setup_back,
        next = sym SETUP_BACK_NEXT,
    );
}

/// How long a piece of leaving took, and what it did to memory, for the log.
struct Took {
    at: std::time::Instant,
    memory: crate::heap::Change,
}

impl Took {
    fn begin() -> Self {
        Self { at: std::time::Instant::now(), memory: crate::heap::Change::begin() }
    }

    fn end(&self) -> String {
        format!("{:.2} s, {}", self.at.elapsed().as_secs_f64(), self.memory.end())
    }
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(
            IN_GAME_LOAD,
            "40 53 56 57 41 54 41 55 41 56 41 57 48 81 ec e0 04 00 00 48 8b f1 33 ff",
            "in-game load",
            in_game_load as InGameLoad,
            &ORIGINAL,
        ),
        // The load asks its reset for the ironman exit save, and to keep the
        // game's history unread.
        Point::keep(0x5d0a29usize, "b2 01 48 8b 4e 38 e8 ec 61 25 00", "in-game load reset"),
        Point::keep(RESET, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20 55", "world reset"),
        Point::keep(0x826e4ausize, "40 84 ff 74 05", "world reset exit save asked"),
        Point::jump(RESET_EXIT_SAVE, "e8 0c 53 da ff", "world reset exit save", reset_exit_save_stub as *const () as usize, &RESET_RESUME),
        Point::keep(RESET_EXIT_SAVE_DONE, "48 8b 5e 78", "world reset after its exit save"),
        Point::jump(SETUP_SAVE, "e8 93 e8 da ff", "session setup save", setup_save_stub as *const () as usize, &SETUP_SAVE_RESUME),
        Point::keep(SETUP_SAVE_DONE, "49 83 be e0 12 00 00 00", "session setup after its save"),
        Point::keep(EXIT_SAVE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 48 8b ec", "ironman exit save").recorded(&EXIT_SAVE_AT),
        // Returning to the main menu: the map manager's frame on its flag
        // (`objects::campaign` keeps it), the ironman exit save, then the
        // restart.
        Point::jump(RETURN_EXIT_SAVE, "e8 98 64 db ff", "return exit save", returning_stub as *const () as usize, &RETURN_RESUME),
        Point::keep(RETURN_EXIT_SAVE_DONE, "48 8b 1d 81 62 c1 01", "return asks for the restart"),
        Point::replace(RESTART, "66 c7 81 83 00 00 00 01 01 48 81 c1 88 00 00 00", "restart", restart as Restart, &ORIGINAL_RESTART),
        Point::keep(MAIN_MENU, "48 8b c4 48 89 58 10 48 89 70 18 48 89 78 20 48 89 48 08", "main menu constructor"),
        // Startup makes the main menu of this size.
        Point::keep(0x205830usize, "b9 48 0b 00 00 e8 9a da 82 01", "main menu size"),
        Point::keep(SWITCH_IDLER, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b da c6 41 48 01", "idler switch"),
        Point::method(gui::classes::MAIN_MENU, 0x30, TAKE_OVER, "main menu takes over"),
        Point::observe(
            TAKE_OVER,
            "40 55 53 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 e8 f0 ff ff",
            "main menu takes over",
            1,
            Some(taking_over),
            Some(took_over),
        ),
        Point::keep(WAIT_TASK, "48 89 5c 24 08 57 48 83 ec 20 48 8b 59 10", "wait for a task"),
        Point::keep(CONTINUE, "4c 8b dc 53 48 83 ec 70 48 8b d9", "continue information from the newest save"),
        Point::keep(RESET_GAME, "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 80 fe ff ff", "game reset"),
        // The game reset skips the history when told to.
        Point::keep(0x210485usize, "45 84 ed 0f 85 9f 00 00 00", "game reset without history"),
        // The single player setup's back button: the session's command, then the restart.
        Point::keep(0x111dec0usize, "48 89 5c 24 08 48 89 74 24 18 55 57 41 56 48 8b ec 48 83 ec 70", "setup back"),
        Point::keep(0x111df29usize, "48 8b 1d 20 e0 30 01", "setup back application"),
        Point::jump(SETUP_BACK_RESTART, "66 c7 83 83 00 00 00 01 01", "setup back restart", setup_back_stub as *const () as usize, &SETUP_BACK_RESUME),
        Point::keep(SETUP_BACK_RESTART_DONE, "48 8d 8b 88 00 00 00", "setup back restart argument"),
        Point::observe(WRITE_SAVE, "48 89 5c 24 10 57 48 83 ec 20 48 8b f9", "save write", 1, None, Some(save_written)),
        // A save that was not written answers other than 0, and is reported.
        Point::keep(0x5cbe8dusize, "80 7f 40 00 74 0c 85 db 74 08", "save write result"),
        Point::keep(AFTER_LOAD, "48 89 5c 24 10 48 89 74 24 18 57 48 83 ec 40 48 8b f9", "after in-game load"),
        Point::keep(LOAD_CONTEXT, "48 89 5c 24 10 57 48 83 ec 20 48 8b 05 87 cb b6 01", "load context"),
        // The in-game menu turns its load button off for an ironman session,
        // and its tooltip says why: neither any more.
        Point::write(0x8322cdusize, "80 b9 d0 24 00 00 00 75 0c", "load button in ironman", "80 b9 d0 24 00 00 00 90 90").uses(&[world::IRONMAN]),
        Point::write(0xb7d0bbusize, "84 c0 74 31", "load tooltip in ironman", "84 c0 eb 31"),
        // The load question's text: the save's name in `rdi`, then the key.
        Point::keep(0x11be24dusize, "48 8b d7 48 8d 4d 07", "load question save name"),
        Point::keep(0x11be259usize, "41 b9 01 00 00 00 4c 8d 45 d7 48 8d 15 e6 74 ba 00 48 8d 4d 97", "load question text key"),
        Point::jump(QUESTION_TEXT, "e8 2d 52 2f 00", "load question text", question_text_stub as *const () as usize, &QUESTION_RESUME),
        Point::keep(QUESTION_TEXT_DONE, "c7 45 7f 07 00 00 00", "load question text set"),
        // Before it looks at its save's name, the in-game load takes the
        // session's interface down, the diplomatic message icons among it;
        // with no name it stops there and answers 1.
        Point::keep(0x5d05d3usize, "49 8b 8e 00 14 00 00 e8 11 f7 82 00", "in-game load empties the diplomatic message icons")
            .uses(&[manager_fields::MESSAGE_ICON_HOLDER]),
        Point::keep(0x5d06c4usize, "48 83 7c 24 58 00 75 48", "in-game load without a name"),
        Point::keep(0x5d070ausize, "b8 01 00 00 00 e9 48 08 00 00", "in-game load answers 1 without a name"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Rules = Rules { return_without_restart: true, save_ironman_before_load: true };
    const UNDO: Rules = Rules { return_without_restart: true, save_ironman_before_load: false };
    const RESTART: Rules = Rules { return_without_restart: false, save_ironman_before_load: true };

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
    fn a_return_for_a_new_game_ends_the_session_for_the_new_game() {
        assert_eq!(returning_to(true, ALL), Destination::NewGame);
        assert_eq!(plan(returning_to(true, ALL), false, ALL), Ok(vec![Step::Ended(End::NewGame), Step::Return, Step::Start]));
        assert_eq!(returning_to(false, ALL), Destination::MainMenu);
        // Turned off since the start was asked for: the game restarts, and
        // the start is answered as the process ends.
        assert_eq!(returning_to(true, RESTART), Destination::MainMenu);
    }

    /// What the game was asked to do, in order.
    #[derive(Debug, PartialEq, Eq)]
    enum Did {
        ExitSave,
        End(End),
        Load,
        ReturnInProcess,
        Restart,
        Stay,
        SaveFailed,
    }

    /// The game, recorded: its ironman exit save and in-process return fail
    /// when told to.
    #[derive(Default)]
    struct Recorded {
        did: Vec<Did>,
        save_fails: bool,
        return_fails: bool,
    }

    impl Game for Recorded {
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
            0
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

    /// A return to `to` as the map manager's frame and then its restart take it.
    fn leave(game: &mut Recorded, to: Destination, ironman: bool, rules: Rules) -> (usize, Option<Result<(), String>>) {
        let rest = run_return(game, plan(to, ironman, rules));
        run_restart(game, Some(rest))
    }

    #[test]
    fn an_ironman_in_game_load_is_saved_ended_and_loaded_in_that_order() {
        let mut game = Recorded::default();
        assert_eq!(run_load(&mut game, plan(Destination::Save, true, ALL).unwrap()), Ok(0));
        assert_eq!(game.did, [Did::ExitSave, Did::End(End::Load), Did::Load]);
    }

    #[test]
    fn an_in_game_load_whose_exit_save_fails_changes_nothing() {
        let mut game = Recorded { save_fails: true, ..Recorded::default() };
        assert!(run_load(&mut game, plan(Destination::Save, true, ALL).unwrap()).is_err());
        assert_eq!(game.did, [Did::ExitSave, Did::SaveFailed]);
    }

    #[test]
    fn a_return_without_restarting_is_made_in_process_and_answered() {
        let mut game = Recorded::default();
        assert_eq!(leave(&mut game, Destination::MainMenu, true, ALL), (0, Some(Ok(()))));
        assert_eq!(game.did, [Did::ExitSave, Did::End(End::Return), Did::ReturnInProcess]);
    }

    #[test]
    fn a_return_for_a_new_game_is_made_in_process_and_leaves_the_start_to_the_main_menu() {
        let mut game = Recorded::default();
        assert_eq!(leave(&mut game, Destination::NewGame, false, ALL), (0, Some(Ok(()))));
        assert_eq!(game.did, [Did::End(End::NewGame), Did::ReturnInProcess]);
    }

    #[test]
    fn a_return_with_restarting_restarts_the_game_unanswered() {
        let mut game = Recorded::default();
        assert_eq!(leave(&mut game, Destination::MainMenu, true, RESTART), (7, None));
        assert_eq!(game.did, [Did::ExitSave, Did::End(End::Return), Did::Restart]);
    }

    #[test]
    fn an_in_process_return_that_cannot_be_made_restarts_the_game() {
        let mut game = Recorded { return_fails: true, ..Recorded::default() };
        assert_eq!(leave(&mut game, Destination::MainMenu, false, ALL), (7, None));
        assert_eq!(game.did, [Did::End(End::Return), Did::ReturnInProcess, Did::Restart]);
    }

    #[test]
    fn a_return_whose_exit_save_fails_keeps_the_session_and_says_why() {
        for to in [Destination::MainMenu, Destination::NewGame] {
            let mut game = Recorded { save_fails: true, ..Recorded::default() };
            assert_eq!(leave(&mut game, to, true, ALL), (0, Some(Err("no room on the disk".into()))));
            // Never told it ended, as it goes on; the game does not restart.
            assert_eq!(game.did, [Did::ExitSave, Did::Stay, Did::SaveFailed], "{to:?}");
        }
    }

    #[test]
    fn a_restart_no_return_asked_for_is_the_games_own() {
        let mut game = Recorded::default();
        assert_eq!(run_restart(&mut game, None), (7, None));
        assert_eq!(game.did, [Did::Restart]);
    }

    #[test]
    fn returns_asked_for_while_one_is_pending_are_answered_with_it() {
        let mut returns = Returns { asked: 0, last: None, rest: None };
        let (first, second) = (returns.ask(), returns.ask());
        assert_eq!(first, second);
        assert_eq!(returns.outcome(first), None);
        returns.acted_on(Err("the game could not write the ironman exit save".into()));
        assert_eq!(returns.outcome(first), Some(Err("the game could not write the ironman exit save".into())));
        // Once it was acted on, another is another return.
        let third = returns.ask();
        assert_ne!(third, first);
        assert_eq!(returns.outcome(third), None);
        returns.acted_on(Ok(()));
        assert_eq!(returns.outcome(third), Some(Ok(())));
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
}
