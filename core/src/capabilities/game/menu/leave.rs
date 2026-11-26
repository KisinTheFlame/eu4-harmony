//! Leaving a session (docs/game/program/saves.md): for the main menu, for a save, or
//! for a new game. `super::course::plan` lays out the steps from where the
//! session goes, whether it is ironman and the framework settings; the
//! player's ways out and remote control's take the same steps.
//!
//! An in-game load (GLOSSARY) is the game's own, `0x5d0580`, which every
//! in-game load goes through: the load question's, the game console's and
//! remote control's. The framework replaces it to take the steps around it.
//! When the settings ask for it, the ironman exit save is written first;
//! only once it was does the session end, extensions hearing it, and the
//! load go on. The world reset inside the game's load never writes one of
//! its own, which the game would write whatever came of it
//! (ADR in-game-load-in-ironman, docs/game/interface/main-menu.md).
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
//! switch, which lets the map manager go. Both the loading screen it
//! presents as it takes over (held back so the still notice stays until
//! its first frame) and the new main menu's black fade-in and residual
//! loading screen slide-out are skipped on the new main menu's first
//! frame, so the map appears and page 0's panel slide-in and camera glide
//! begin at once; startup and sub-page switches keep the game's
//! transitions. The single player setup's back button, which restarts the
//! game too, goes back the same way.
//!
//! What the game's world reset and the map manager's destructor leave of a
//! session, the framework's repairs put right (`crate::repairs`).
//!
//! The steps are laid out and taken as a whole by `super::course`, over what
//! each does in the game; here are the game's own steps and the hooks that
//! hand them over (`super`'s `InGame`).
use crate::base::called_by_game;
use crate::{native::game_string, native::hooks::Call, native::memory::Memory, notice::Message, native::objects::{
        application,
        campaign::{load_request, map_manager as manager_fields, world},
        gui,
    }, native::points::{Function, GameFunction, Point}, repairs, session::sessions, text::unicode::game_text};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

use super::{
    course::{Loaded, Rules},
    map_manager, InGame, COURSE,
};

/// `(request) -> u32`: the in-game load (see [`Loaded`]).
const IN_GAME_LOAD: GameFunction<1> = GameFunction::at(0x5d0580);
/// `(map manager, save)`: the world reset of an in-game load; with `save`
/// set it writes the ironman exit save first.
const RESET: GameFunction<2> = GameFunction::at(0x826c20);
/// `()`: the ironman exit save, of an ironman session only.
const EXIT_SAVE: GameFunction<0> = GameFunction::at(0x5cc160);
/// `(save request) -> u32`: writes a save, 0 when it was written.
const WRITE_SAVE: usize = 0x5cbde0;
/// `(map manager)`: what the load question does once the load returned.
const AFTER_LOAD: GameFunction<1> = GameFunction::at(0x824810);
/// `()`: what the load question puts in [`load_request::CONTEXT`].
const LOAD_CONTEXT: GameFunction<0> = GameFunction::at(0x8bf160);
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
const MAIN_MENU: GameFunction<4> = GameFunction::at(0x10d23c0);
const MAIN_MENU_SIZE: usize = 0xb48;
/// `(application, &idler, keep)`: the application's idler switch, at its
/// next frame; with `keep` clear it lets the idler it leaves go.
const SWITCH_IDLER: GameFunction<3> = GameFunction::at(0x14c2530);
/// `(main menu)`: the main menu taking over at the idler switch, its method
/// at `+0x30`. As at startup, it presents a frame of the loading screen, a
/// painting over the whole screen (docs/game/interface/main-menu.md).
const TAKE_OVER: usize = 0x10d5bc0;
/// `(task)`: waits for a task of the game's to be done.
const WAIT_TASK: GameFunction<1> = GameFunction::at(0x17139c0);
/// `(application)`: the continue information from the newest save.
const CONTINUE: GameFunction<1> = GameFunction::at(0x212810);
/// `(application, without history)`: the world reset as a new start finds
/// it; it reads the game's history unless told not to.
const RESET_GAME: GameFunction<2> = GameFunction::at(0x210000);
/// The single player setup's back button, where it asks for the restart
/// itself (`mov word ptr [application + 0x83], 0x101`, the application in
/// `rbx`); and what follows.
const SETUP_BACK_RESTART: usize = 0x111dfd7;
const SETUP_BACK_RESTART_DONE: usize = 0x111dfe0;
/// `NFrontend.FADE_IN_DONE_TIME` and `GUI_START_MOVE_TIME` (f32 globals).
const FADE_IN_DONE_TIME: usize = 0x233e778;
const GUI_START_MOVE_TIME: usize = 0x233e774;
/// `(dt: f32) -> u8`: shifts the loading screen window's elements off-screen
/// and returns whether it is still sliding out.
const SLIDE_LOAD_SCREEN: usize = 0x11bbd10;
/// Main menu intro elapsed timer (f32).
const MAIN_MENU_INTRO_ELAPSED: usize = 0x720;

/// Whether the session is ironman.
pub(super) fn ironman(m: &Memory) -> bool {
    super::world_now(m).and_then(|w| m.u8(w + world::IRONMAN)).is_ok_and(|b| b != 0)
}

/// How many saves the game wrote, and what the last one answered.
static WRITTEN: Mutex<(u64, u32)> = Mutex::new((0, 0));

fn save_written(call: &mut Call) {
    let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    *written = (written.0 + 1, call.result_as::<u32>().unwrap_or(u32::MAX));
}

/// Writes the ironman exit save, and says why when it was not written.
pub(super) fn exit_save(m: &Memory) -> Result<(), String> {
    let before = WRITTEN.lock().unwrap_or_else(|e| e.into_inner()).0;
    EXIT_SAVE.call(m, []).map_err(|e| format!("the ironman exit save faulted: {e}"))?;
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

resume!(SETUP_SAVE_RESUME: SETUP_SAVE_NEXT = SETUP_SAVE_DONE);

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

resume!(RESET_RESUME: RESET_NEXT = RESET_EXIT_SAVE_DONE);

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

type InGameLoad = unsafe extern "C" fn(usize) -> u32;
static ORIGINAL: Function<InGameLoad> = Function::new();

/// The game's own in-game load of the `request`, inside the hook: the world
/// reset inside it writes no ironman exit save of its own, the save the game
/// makes as it sets an ironman session up waits until the load let its file
/// go, and a successful load restores the player's map mode that `0x5d05ce`
/// reset to terrain. What it answered.
pub(super) fn load(m: &Memory, request: usize) -> u32 {
    let manager = m.ptr(request + load_request::MAP_MANAGER).ok().filter(|&p| p != 0);
    let saved_mode = manager.and_then(|manager| crate::capabilities::game::map::mode_before_load(m, manager));
    QUIET_RESET.store(true, Ordering::Relaxed);
    LOADING_IN_GAME.store(true, Ordering::Relaxed);
    SETUP_SAVE_WAITED.store(false, Ordering::Relaxed);
    let took = Took::begin();
    let code = unsafe { ORIGINAL.get()(request) };
    LOADING_IN_GAME.store(false, Ordering::Relaxed);
    QUIET_RESET.store(false, Ordering::Relaxed);
    log!("leave: the in-game load answered {:?}, {}; {}", Loaded::of(code), took.end(), crate::base::heap::split());
    if SETUP_SAVE_WAITED.swap(false, Ordering::Relaxed) {
        log!("leave: the save the game makes as it sets the loaded ironman session up, now that the load let its file go");
        if let Err(e) = EXIT_SAVE.call(m, []) {
            log!("leave: that save faulted: {e}");
        }
    }
    if Loaded::of(code) == Loaded::Read && sessions::last_load_read() {
        if let (Some(manager), Some(mode)) = (manager, saved_mode) {
            crate::capabilities::game::map::restore_after_load(m, manager, mode);
        }
    }
    code
}

/// The in-process return (GLOSSARY), at the restart of the application
/// `app`, or why it could not be made, which leaves the game to restart.
pub(super) fn return_in_process(m: &Memory, app: usize) -> Result<(), String> {
    match in_process_return(m, app) {
        Ok(()) => {
            log!("leave: the main menu takes over at the application's next switch; {}", crate::base::heap::split());
            Ok(())
        }
        Err(why) => {
            log!("leave: no in-process return ({why}); the game restarts");
            Err(why)
        }
    }
}

/// The game's own restart of the application `app`, and what it answered.
pub(super) fn restart_game(app: usize, argument: usize) -> usize {
    unsafe { ORIGINAL_RESTART.get()(app, argument) }
}

/// The session stays: the map manager no longer returns.
pub(super) fn stay(m: &Memory) {
    if let Ok(Some(manager)) = map_manager(m) {
        let _ = m.write(manager + manager_fields::RETURN, &[0]);
    }
}

/// The player hears that the ironman exit save failed.
pub(super) fn save_failed() {
    crate::base::notice::post("ironman save failed", crate::base::notice::Repeat::Always, Message::new("EU4HARMONY_IRONMAN_SAVE_FAILED"));
}

/// Every in-game load: its steps, around the game's own (`super::course`).
#[called_by_game]
unsafe extern "C" fn in_game_load(request: usize) -> u32 {
    let Some(m) = crate::native::memory().copied() else { return ORIGINAL.get()(request) };
    crate::base::guarded("in-game load", || {
        if crate::capabilities::game::signals::in_day() {
            log!("leave: refused an in-game load while a daily update is on the stack");
            return 1;
        }
        if let Some(h) = crate::capabilities::hooks() {
            (h.ui_still)("EU4HARMONY_STILL_LOAD");
        }
        COURSE.in_game_load(&mut InGame { request, ..InGame::new(&m) })
    })
}

/// The load question's text: the game's, unless the ironman exit save comes
/// before the load, which the framework's says. `name` is the save's name,
/// a C string of game text.
#[called_by_game]
extern "C" fn question_text(out: usize, key: usize, values: usize, name: usize) {
    let m = crate::native::hooked_memory();
    if let Err(e) = crate::native::game_string::localize_into(&m, out, key, values, 1) {
        log!("leave: the load question's text: {e}");
        return;
    }
    if !(ironman(&m) && Rules::now().save_ironman_before_load) {
        return;
    }
    let saved = (|| {
        let end = (0..1024).find(|&i| m.u8(name + i).is_ok_and(|b| b == 0)).ok_or("the save's name has no end")?;
        let name: String = game_text::chars(&m.bytes(name, end)?).map(|(c, _)| c).collect();
        let words = crate::base::loc::framework(crate::base::lang::player(), "EU4HARMONY_CONFIRM_LOAD_IRONMAN", &[("NAME", &name)]);
        game_string::read(&m, out)?;
        unsafe { game_string::truncate(out as *mut u8, 0) };
        game_string::append_checked(&m, out, &game_text::from_str(&words))
    })();
    if let Err(e) = saved {
        log!("leave: the load question keeps the game's text: {e}");
    }
}

resume!(QUESTION_RESUME: QUESTION_NEXT = QUESTION_TEXT_DONE);

/// Replaces the load question's `call <localisation lookup>` for its text.
#[unsafe(naked)]
extern "C" fn question_text_stub() {
    std::arch::naked_asm!(
        call_rust!("mov r9, rdi", call text),
        "jmp qword ptr [rip + {next}]",
        text = sym question_text,
        next = sym QUESTION_NEXT,
    );
}

/// Loads the save `name` (in `save games`, without `.eu4`) in place of the
/// session, as the load question does when the player accepts it; at a game
/// frame, after the map manager's. What the game's in-game load answered,
/// which went through [`in_game_load`].
pub(super) fn load_in_game(m: &Memory, name: &str) -> Result<u32, String> {
    let manager = map_manager(m)?.ok_or("no game is going on")?;
    let mut request = [0usize; load_request::SIZE / 8];
    request[(load_request::NAME + game_string::CAPACITY) / 8] = game_string::INLINE;
    let at = request.as_mut_ptr() as usize;
    game_string::append_checked(m, at + load_request::NAME, name.as_bytes())?;
    request[load_request::MAP_MANAGER / 8] = manager;
    let context = LOAD_CONTEXT.call(m, []);
    let loaded = context.and_then(|context| {
        request[load_request::CONTEXT / 8] = context;
        IN_GAME_LOAD.call(m, [at])
    });
    let after = loaded.is_ok().then(|| AFTER_LOAD.call(m, [manager]));
    let destroyed = game_string::destroy_checked(m, at + load_request::NAME);
    let code = loaded?;
    after.transpose()?;
    destroyed?;
    Ok(code as u32)
}

/// In the map manager's frame, where it writes the ironman exit save as it
/// returns to the main menu: the steps up to the restart it asks for next.
#[called_by_game]
extern "C" fn returning() {
    if let Some(h) = crate::capabilities::hooks() {
        (h.ui_still)("EU4HARMONY_STILL_RETURN");
    }
    let m = crate::native::hooked_memory();
    COURSE.returning(&mut InGame::new(&m));
}

resume!(RETURN_RESUME: RETURN_NEXT = RETURN_EXIT_SAVE_DONE);

/// Replaces the return's `call <ironman exit save>`.
#[unsafe(naked)]
extern "C" fn returning_stub() {
    std::arch::naked_asm!(call_rust!(call returning), "jmp qword ptr [rip + {next}]", returning = sym returning, next = sym RETURN_NEXT,);
}

type Restart = unsafe extern "C" fn(usize, usize) -> usize;
static ORIGINAL_RESTART: Function<Restart> = Function::new();

/// The return's restart: with the rest [`returning`] left, the game
/// restarts, or returns in process, or stays in the session.
#[called_by_game]
unsafe extern "C" fn restart(app: usize, argument: usize) -> usize {
    let Some(m) = crate::native::memory().copied() else { return ORIGINAL_RESTART.get()(app, argument) };
    crate::base::guarded("return to the main menu", || COURSE.restart(&mut InGame { app, argument, ..InGame::new(&m) }))
}

/// Takes the session down as the in-game load does before it reads a save,
/// with the game's history, then makes a main menu as startup does and
/// switches the application to it.
fn in_process_return(m: &Memory, app: usize) -> crate::base::Result<()> {
    let manager = map_manager(m)?.ok_or("no game is going on")?;
    let interface = repairs::SessionInterface::before_reset(m, manager)?;
    take_the_interface_down(m, manager)?;
    // The in-game load's world reset, here with the history, so the main
    // menu is made on a world as a new start has it, as at startup. With its
    // second argument clear it writes no ironman exit save, which was
    // written already. It still uses the session's interface, which goes
    // with the map manager.
    let took = Took::begin();
    RESET.call(m, [manager, 0])?;
    log!("leave: the world reset with the history, {}", took.end());
    sessions::reset_with_history();
    interface.let_go_with_map_manager(m)?;
    main_menu(m, app)
}

/// Takes down what of the session's interface the in-game load takes down
/// before its world reset: among it the diplomatic message icons, whose
/// windows stay shown otherwise, drawing a sprite whose texture the other
/// message icons let go (docs/game/interface/main-menu.md). The game's in-game load
/// does that before it looks at its save's name, and given none stops there
/// and answers 1 (`0x5d06c4`): so it is called with no name.
fn take_the_interface_down(m: &Memory, manager: usize) -> crate::base::Result<()> {
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
fn main_menu(m: &Memory, app: usize) -> crate::base::Result<()> {
    let (first, second) = (m.ptr(app + application::MAIN_MENU_ARGS.0)?, m.ptr(app + application::MAIN_MENU_ARGS.1)?);
    let memory = crate::native::game_heap::new(m, MAIN_MENU_SIZE)?;
    if memory == 0 {
        return Err("no memory for the main menu".into());
    }
    MAIN_MENU.call(m, [memory, first, second, app])?;
    let mut idler = memory;
    SWITCH_IDLER.call(m, [app, &mut idler as *mut usize as usize, 0])?;
    RETURNED.store(memory, Ordering::Relaxed);
    // The continue box offers the save the game wrote last, as after a restart.
    let task = m.ptr(app + application::CONTINUE_TASK)?;
    if task != 0 {
        WAIT_TASK.call(m, [task])?;
    }
    CONTINUE.call(m, [app])?;
    Ok(())
}

/// The main menu made by [`main_menu`], from when it was asked to switch until
/// it took over; 0 otherwise.
static RETURNED: AtomicUsize = AtomicUsize::new(0);
/// The returned main menu whose black fade-in and residual loading screen are
/// being skipped on its first frame; 0 otherwise.
static SKIP_INTRO: AtomicUsize = AtomicUsize::new(0);

type SlideLoadScreen = unsafe extern "C" fn(f32) -> u8;
static ORIGINAL_SLIDE_LOAD_SCREEN: Function<SlideLoadScreen> = Function::new();

/// Elapsed intro time past both the fade-out (`fade_in_done`) and the slide-out
/// start threshold (`gui_start_move`) so Frame 1 hides `Background` and ends the
/// loading screen immediately.
fn intro_elapsed(fade_in_done: f32, gui_start_move: f32) -> f32 {
    let fade = if fade_in_done.is_finite() { fade_in_done.max(0.0) } else { 0.0 };
    let start = if gui_start_move.is_finite() { gui_start_move.max(0.0) } else { 0.0 };
    let target = fade.max(start);
    (target + 1.0).max(target * 1.01)
}

/// Configures `frontend` to skip its intro fade and reach `ShowPage` on Frame 1.
fn arm_skip_intro(m: &Memory, frontend: usize) -> crate::base::Result<()> {
    if m.ptr(frontend)? != m.at(gui::classes::MAIN_MENU) {
        return Err("unexpected main menu object".into());
    }
    if m.u32(frontend + gui::main_menu::PAGE)? != gui::main_menu::FIRST_PAGE {
        return Ok(());
    }
    let fade = f32::from_bits(m.u32(m.at(FADE_IN_DONE_TIME))?);
    let gui_start = f32::from_bits(m.u32(m.at(GUI_START_MOVE_TIME))?);
    let elapsed = intro_elapsed(fade, gui_start);
    m.write(frontend + MAIN_MENU_INTRO_ELAPSED, &elapsed.to_le_bytes())?;
    SKIP_INTRO.store(frontend, Ordering::Relaxed);
    Ok(())
}

/// As a main menu takes over: one an in-process return made presents the
/// loading screen as at startup, which is held back, so the still notice
/// stays until the main menu's first frame.
fn taking_over(call: &mut Call) {
    let frontend = call.arg(0);
    if frontend != 0 && RETURNED.load(Ordering::Relaxed) == frontend {
        if let Some(h) = crate::capabilities::hooks() {
            (h.ui_hold)();
        }
    }
}

fn took_over(call: &mut Call) {
    let frontend = call.arg(0);
    if frontend != 0 && RETURNED.compare_exchange(frontend, 0, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
        log!(
            "leave: the main menu took over, {} of its presents held back",
            crate::capabilities::hooks().map_or(0, |h| (h.ui_let_go)())
        );
        if !call.faulted() {
            if let Err(e) = arm_skip_intro(&call.memory(), frontend) {
                log!("leave: keeping the main menu's intro: {e}");
            }
        }
    }
}

/// Reports the loading screen window finished sliding off-screen immediately
/// on Frame 1 of an in-process return so `0x11bbbc0` destroys it and opens
/// page 0 (with its panel slide-in) on that frame.
#[called_by_game]
unsafe extern "C" fn slide_load_screen(dt: f32) -> u8 {
    if crate::base::thread_id() == crate::base::main_thread() && SKIP_INTRO.swap(0, Ordering::Relaxed) != 0 {
        log!("leave: skipped the main menu's fade and loading screen");
        return 0;
    }
    ORIGINAL_SLIDE_LOAD_SCREEN.get()(dt)
}

/// Fallback at the end of a main menu frame so a skipped intro cannot leak
/// into a later sub-page return if Frame 1 did not reach the loading screen
/// slide check.
pub(super) fn main_menu_frame(frontend: usize) {
    if frontend != 0 && SKIP_INTRO.compare_exchange(frontend, 0, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
        log!("leave: main menu intro skip was not taken on its first frame");
    }
}

/// The single player setup's back button, about to ask for the restart:
/// with "return without restarting" on, the world is reset with the history
/// instead, as the setup may have changed it, and a new main menu takes
/// over; answers whether it did.
#[called_by_game]
extern "C" fn setup_back(app: usize) -> bool {
    if !Rules::now().return_without_restart {
        return false;
    }
    if let Some(h) = crate::capabilities::hooks() {
        (h.ui_still)("EU4HARMONY_STILL_RETURN");
    }
    let m = crate::native::hooked_memory();
    let took = Took::begin();
    let back = RESET_GAME.call(&m, [app, 0]).and_then(|_| {
        log!("leave: the single player setup was left; the world reset with the history, {}", took.end());
        sessions::reset_with_history();
        main_menu(&m, app)
    });
    match back {
        Ok(()) => true,
        Err(why) => {
            log!("leave: no in-process return from the single player setup ({why}); the game restarts");
            false
        }
    }
}

resume!(SETUP_BACK_RESUME: SETUP_BACK_NEXT = SETUP_BACK_RESTART_DONE);

/// Replaces the setup's back button's request for the restart: made unless
/// [`setup_back`] went back in process.
#[unsafe(naked)]
extern "C" fn setup_back_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rcx, rbx", call back, "cmp byte ptr [rsp + 0x48], 0"),
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
    memory: crate::base::heap::Change,
}

impl Took {
    fn begin() -> Self {
        Self { at: std::time::Instant::now(), memory: crate::base::heap::Change::begin() }
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
        // Before it looks at its save's name, the in-game load resets the map
        // mode to terrain and takes the session's interface down, the
        // diplomatic message icons among it; with no name it stops there and
        // answers 1.
        Point::keep(0x5d05c3usize, "45 33 c9 45 33 c0 33 d2 49 8b ce e8 1d 1a 25 00", "in-game load resets map mode to terrain"),
        Point::keep(
            0x81d41ausize,
            "41 80 be 60 04 00 00 00 75 12 41 b1 01 45 33 c0 41 8b 56 2c 49 8b ce e8 ba 4b 00 00",
            "interface build skips map mode refresh during in-game load",
        )
        .uses(&[0x460, manager_fields::MODE]),
        Point::keep(0x5d05d3usize, "49 8b 8e 00 14 00 00 e8 11 f7 82 00", "in-game load empties the diplomatic message icons")
            .uses(&[manager_fields::MESSAGE_ICON_HOLDER]),
        Point::keep(0x5d06c4usize, "48 83 7c 24 58 00 75 48", "in-game load without a name"),
        Point::keep(0x5d070ausize, "b8 01 00 00 00 e9 48 08 00 00", "in-game load answers 1 without a name"),
        // Skipping the main menu's black fade-in and residual loading screen on
        // an in-process return (docs/game/interface/main-menu.md): the fade
        // timer, the intro threshold, and the loading screen's slide-out and
        // teardown.
        Point::keep(0x10d30edusize, "f3 0f 10 05 83 b6 26 01 0f 28 f0 f3 41 0f 5c b6 20 07 00 00 f3 0f 5e f0", "main menu fade timer")
            .uses(&[FADE_IN_DONE_TIME, MAIN_MENU_INTRO_ELAPSED]),
        Point::keep(0x10d3464usize, "f3 41 0f 10 86 20 07 00 00 0f 2f 05 00 b3 26 01 76 3a", "main menu intro start threshold")
            .uses(&[MAIN_MENU_INTRO_ELAPSED, GUI_START_MOVE_TIME]),
        Point::keep(0x10d34cdusize, "f3 41 0f 10 86 10 03 00 00 e8 35 88 0e 00 84 c0 0f 85 54 02 00 00", "main menu load screen slide out"),
        Point::keep(
            0x10d34f6usize,
            "e8 c5 86 0e 00 84 c0 74 1d 41 b0 01 41 8b 96 00 09 00 00 49 8b ce e8 4f 7a 00 00",
            "main menu opens page after load screen",
        )
        .uses(&[gui::main_menu::PAGE]),
        Point::replace(
            SLIDE_LOAD_SCREEN,
            "48 89 5c 24 08 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8b ec 48 83 ec 60",
            "slide loading screen",
            slide_load_screen as SlideLoadScreen,
            &ORIGINAL_SLIDE_LOAD_SCREEN,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_game::buffers::{Image, Object};

    #[test]
    fn intro_elapsed_clears_fade_and_slide_thresholds() {
        for (fade, start) in [
            (2.2, 1.4),
            (1.4, 3.0),
            (0.0, 0.0),
            (-1.0, -2.0),
            (1e9, 1e9),
            (f32::NAN, f32::INFINITY),
            (2.2, f32::NEG_INFINITY),
        ] {
            let elapsed = intro_elapsed(fade, start);
            assert!(elapsed.is_finite(), "non-finite elapsed for ({fade}, {start}): {elapsed}");
            if start.is_finite() {
                assert!(elapsed > start, "elapsed {elapsed} <= gui_start {start}");
            }
            if fade.is_finite() && fade > 0.0 {
                let alpha = (fade - elapsed) / fade;
                assert!(alpha <= 0.05, "alpha {alpha} > 0.05 for fade {fade}");
            }
        }
    }

    #[test]
    fn arm_skip_intro_only_on_first_page() {
        SKIP_INTRO.store(0, Ordering::Relaxed);
        let image = Image::new();
        image.i32(FADE_IN_DONE_TIME, 2.2f32.to_bits() as i32).i32(GUI_START_MOVE_TIME, 1.4f32.to_bits() as i32);
        let m = image.memory();

        let setup_page = Object::new(MAIN_MENU_INTRO_ELAPSED.max(gui::main_menu::PAGE) + 8)
            .u64(0, m.at(gui::classes::MAIN_MENU) as u64)
            .u32(gui::main_menu::PAGE, gui::main_menu::SETUP_PAGE);
        assert_eq!(arm_skip_intro(m, setup_page.at()), Ok(()));
        assert_eq!(m.u32(setup_page.at() + MAIN_MENU_INTRO_ELAPSED).unwrap(), 0.0f32.to_bits());
        assert_eq!(SKIP_INTRO.load(Ordering::Relaxed), 0);

        let first_page = Object::new(MAIN_MENU_INTRO_ELAPSED.max(gui::main_menu::PAGE) + 8)
            .u64(0, m.at(gui::classes::MAIN_MENU) as u64)
            .u32(gui::main_menu::PAGE, gui::main_menu::FIRST_PAGE);
        assert_eq!(arm_skip_intro(m, first_page.at()), Ok(()));
        assert!(f32::from_bits(m.u32(first_page.at() + MAIN_MENU_INTRO_ELAPSED).unwrap()) > 2.2);
        assert_eq!(SKIP_INTRO.load(Ordering::Relaxed), first_page.at());

        main_menu_frame(first_page.at());
        assert_eq!(SKIP_INTRO.load(Ordering::Relaxed), 0);
    }
}
