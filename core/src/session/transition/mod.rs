//! Player and remote-control session transitions: leaving a session (in-game
//! load, in-process return to the main menu, setup back button, restart) and
//! launching a save or new game from the main menu.
//!
//! All session-transition hooks and state machines live here beside
//! [`super::sessions`] so `capabilities::game::menu` stays a thin capability
//! front-end and `capabilities` needs no dependency on `repairs`.
mod course;
mod leave;

use crate::{
    base::status::{fail, GResult, Status},
    base::Result,
    native::{
        game_string,
        memory::Memory,
        objects::{
            application,
            campaign::{classes as campaign_classes, countries as country_list, country as country_fields, globals, map_manager as manager_fields, world},
            gui::{classes as gui_classes, main_menu},
            setup,
        },
        points::{GameFunction, Point},
    },
    session::{
        saves::file as save_file,
        sessions::{self, At, End},
    },
    world::Tag,
};
use course::{Action, Course, Reads, Rules, Steps};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock,
};
use std::time::Instant;

/// `(main menu, page, animate)`: shows a page of the main menu.
const SHOW_PAGE: GameFunction<3> = GameFunction::at(0x10daf60);
/// `(setup)`: loads the save [`setup::SAVE_PATH`] names into the setup and
/// selects [`setup::SAVE_PLAYER`]'s country, or the world's player for ironman.
const CHOOSE_SAVE: GameFunction<1> = GameFunction::at(0x1130f40);
/// `(setup, true)`: starts the game as the continue box does.
const CONTINUE: GameFunction<2> = GameFunction::at(0x1120740);
/// `(world, tag)`: makes the country the player's.
const SET_PLAYER: GameFunction<2> = GameFunction::at(0x763840);
/// `(world, &tag, kind)`: the tag of that kind for the letters.
const KIND_TAG: GameFunction<3> = GameFunction::at(0x76ee40);
/// `(setup, &tag, false)`: selects the country in the setup.
const SELECT: GameFunction<3> = GameFunction::at(0x111cb80);
/// `(setup)`: starts the game from the single player setup.
const START: GameFunction<1> = GameFunction::at(0x1120280);

/// Callbacks wired once by `app` so `session::transition` never depends on
/// `framework_ui` or `capabilities`.
pub(crate) struct TransitionHooks {
    pub(crate) ui_still: fn(&'static str),
    pub(crate) in_day: fn() -> bool,
    pub(crate) mode_before_load: fn(&Memory, usize) -> Option<i32>,
    pub(crate) restore_after_load: fn(&Memory, usize, i32),
    pub(crate) pause_game: fn(&Memory) -> GResult<bool>,
    pub(crate) is_still: fn(&Memory) -> bool,
}

static HOOKS: OnceLock<TransitionHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: TransitionHooks) {
    let _ = HOOKS.set(hooks);
}

pub(super) fn hooks() -> Option<&'static TransitionHooks> {
    HOOKS.get()
}

/// Where the game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    MainMenu,
    /// A game is being set up or loaded.
    Starting,
    Game,
}

/// What came of a load or start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Launch {
    /// It ran, and the game is starting.
    #[default]
    Waiting,
    /// It has not run yet: a game frame now is still the session it leaves.
    Asked,
    /// It failed, and why.
    Failed(String),
}

/// What came of a return to the main menu so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Returned {
    Waiting,
    /// The session stays, and why: its ironman exit save failed, say.
    Failed(String),
    /// The main menu is up, in this process.
    Done,
}

/// Every request made of the main menu and of a session, and its course.
static COURSE: Course = Course::new();

/// Set from the UI thread when the player accepts an immediate restart from
/// the main menu; consumed on the main thread's next main-menu frame.
static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Schedules a graceful restart of `eu4.exe -skiplauncher` on the next
/// main-menu frame. Refuses when not in the main menu.
pub(crate) fn request_restart() -> std::result::Result<(), String> {
    if !matches!(sessions::at(), At::MainMenu(_)) {
        return Err("restart is only available from the main menu".into());
    }
    RESTART_REQUESTED.store(true, Ordering::Release);
    Ok(())
}

/// Where the game is.
pub(crate) fn place(m: &Memory) -> Place {
    let at = sessions::at();
    COURSE.place(at, &mut InGame::at(m, at))
}

/// Whether a game is being started from the main menu `frontend`.
pub(crate) fn starting(m: &Memory, frontend: usize) -> bool {
    COURSE.starting(&mut InGame { frontend, ..InGame::new(m) })
}

/// Whether an in-game load is already pending or running.
pub(crate) fn loading_in_game() -> bool {
    COURSE.loading_in_game()
}

/// Queues a load from the main menu or in-game.
pub(crate) fn ask_load(in_game: bool, save_name: String, path: String, player: Tag) {
    COURSE.ask(if in_game { Action::LoadInGame(save_name) } else { Action::Load(path, player) });
}

/// Queues a new game start from the main menu.
pub(crate) fn ask_start(letters: [u8; 3]) {
    COURSE.ask(Action::Start(letters));
}

/// Starts a new game from an active session (returns in-process, then starts).
pub(crate) fn new_game_from_session(m: &Memory, letters: [u8; 3], check_country: impl FnOnce() -> GResult<()>) -> GResult<u64> {
    COURSE.new_game_from_session(&mut InGame::new(m), letters, check_country)
}

/// The last load or start asked for, to query [`launch`] about.
pub(crate) fn last() -> u64 {
    COURSE.last()
}

/// What came of the load or start `number`.
pub(crate) fn launch(number: u64) -> Launch {
    COURSE.outcome(number, reads())
}

/// Returns to the main menu from an active session.
pub(crate) fn return_to_main_menu(m: &Memory) -> GResult<Option<u64>> {
    COURSE.return_to_main_menu(&mut InGame::new(m))
}

/// What came of the return asked for as `number`.
pub(crate) fn returned(number: u64) -> Returned {
    COURSE.returned(number, || crate::native::memory().is_some_and(|m| place(m) == Place::MainMenu))
}

/// At a game frame, after the map manager's: runs any pending in-game load.
pub(crate) fn game_frame(m: &Memory) -> bool {
    COURSE.game_frame(&mut InGame::new(m))
}

/// A daily update just finished: resets the stillness count before leaving.
pub(crate) fn day_ran() {
    COURSE.day_ran();
}

/// At the end of the main menu's frame: runs any pending restart, load, or start.
pub(crate) fn main_menu_frame(m: &Memory, frontend: usize) {
    leave::main_menu_frame(frontend);
    if RESTART_REQUESTED.swap(false, Ordering::AcqRel) {
        if let Err(e) = leave::request_restart_from_main_menu(m, frontend) {
            log!("leave: graceful restart from main menu failed: {e}");
            crate::base::notice::post("restart failed", crate::base::notice::Repeat::Always, crate::base::notice::Message::new("EU4HARMONY_RESTART_FAILED"));
        }
        return;
    }
    COURSE.main_menu_frame(&mut InGame { frontend, ..InGame::new(m) });
}

/// The main menu at this safe point, on its first page, with nothing starting.
pub(crate) fn first_page(m: &Memory) -> GResult<usize> {
    let At::MainMenu(frontend) = sessions::at() else {
        return fail(Status::Unavailable, "only in the main menu");
    };
    if m.ptr(frontend)? != m.at(gui_classes::MAIN_MENU) {
        return Err("unexpected main menu object".to_string().into());
    }
    if starting(m, frontend) {
        return fail(Status::Unavailable, "a game is already being started");
    }
    if m.u32(frontend + main_menu::PAGE)? != main_menu::FIRST_PAGE {
        return fail(Status::Unavailable, "only from the main menu's first page");
    }
    Ok(frontend)
}

/// The world, when there is one.
pub(crate) fn world_now(m: &Memory) -> Result<usize> {
    let world = m.ptr(m.at(globals::WORLD))?;
    if world == 0 {
        return Err("no world".into());
    }
    Ok(world)
}

/// The session's map manager, checked by its class; None outside a game.
pub(super) fn map_manager(m: &Memory) -> Result<Option<usize>> {
    let world = m.ptr(m.at(globals::WORLD))?;
    let manager = if world == 0 { 0 } else { m.ptr(world + world::MAP_MANAGER)? };
    Ok((manager != 0 && m.ptr(manager)? == m.at(campaign_classes::MAP_MANAGER)).then_some(manager))
}

/// The country whose tag is `letters`, among the countries the world has.
pub(crate) fn find_country(m: &Memory, letters: [u8; 3]) -> GResult<Tag> {
    let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
    if manager == 0 {
        return fail(Status::NotReady, "no countries yet");
    }
    let list = m.ptr(manager + country_list::LIST)?;
    let count = m.i32(manager + country_list::COUNT)?;
    if !(0..=32767).contains(&count) {
        return Err("country array bound".to_string().into());
    }
    for index in 0..count as usize {
        let country = m.ptr(list + index * 8)?;
        if country == 0 {
            continue;
        }
        let tag = Tag(m.u64(country + country_fields::TAG)?);
        let player = tag.normal() || (letters == save_file::OBSERVER && tag.valid() && tag.kind() == world::OBSERVE);
        if player && tag.letters() == letters && tag.index() as usize == index {
            return Ok(tag);
        }
    }
    Err(eu4_harmony_remote::checks::countries::not_found(&String::from_utf8_lossy(&letters)).into())
}

struct InGame<'a> {
    m: &'a Memory,
    frontend: usize,
    request: usize,
    app: usize,
    argument: usize,
}

impl<'a> InGame<'a> {
    fn new(m: &'a Memory) -> Self {
        Self { m, frontend: 0, request: 0, app: 0, argument: 0 }
    }

    fn at(m: &'a Memory, at: At) -> Self {
        Self { frontend: if let At::MainMenu(frontend) = at { frontend } else { 0 }, ..Self::new(m) }
    }
}

impl course::Game for InGame<'_> {
    fn steps(&mut self) -> std::result::Result<Steps, String> {
        steps(self.m, self.frontend)
    }

    fn first_page(&mut self) -> std::result::Result<(), String> {
        let (m, frontend) = (self.m, self.frontend);
        if m.ptr(frontend).ok() != Some(m.at(gui_classes::MAIN_MENU)) {
            Err("unexpected main menu object".into())
        } else if m.u32(frontend + main_menu::PAGE).ok() != Some(main_menu::FIRST_PAGE) {
            Err("the main menu left its first page before the game could take it up".into())
        } else {
            Ok(())
        }
    }

    fn carry_out(&mut self, action: &Action) -> std::result::Result<(), String> {
        match action {
            Action::Load(path, player) => load_now(self.m, self.frontend, path, *player),
            Action::Start(letters) => start_now(self.m, self.frontend, *letters),
            Action::LoadInGame(_) => Err("an in-game load runs only in a session".into()),
        }
    }

    fn back_to_first_page(&mut self) {
        back_to_first_page(self.m, self.frontend);
    }

    fn not_continuing(&mut self) {
        let _ = self.m.write(self.frontend + main_menu::CONTINUING, &[0]);
    }

    fn reads(&self) -> Reads {
        reads()
    }

    fn going_on(&self) -> bool {
        sessions::going_on()
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn rules(&self) -> Rules {
        Rules::now()
    }

    fn ironman(&self) -> bool {
        leave::ironman(self.m)
    }

    fn ask_return(&mut self) -> GResult<()> {
        ask_return(self.m)
    }

    fn pause(&mut self) -> GResult<bool> {
        match hooks() {
            Some(h) => (h.pause_game)(self.m),
            None => Ok(false),
        }
    }

    fn still(&mut self) -> bool {
        hooks().is_some_and(|h| (h.is_still)(self.m))
    }

    fn load_in_game(&mut self, name: &str) -> std::result::Result<u32, String> {
        leave::load_in_game(self.m, name)
    }

    fn exit_save(&mut self) -> std::result::Result<(), String> {
        leave::exit_save(self.m)
    }

    fn end(&mut self, end: End) {
        sessions::ending(end);
    }

    fn load(&mut self) -> u32 {
        leave::load(self.m, self.request)
    }

    fn return_in_process(&mut self) -> std::result::Result<(), String> {
        leave::return_in_process(self.m, self.app)
    }

    fn restart(&mut self) -> usize {
        leave::restart_game(self.app, self.argument)
    }

    fn stay(&mut self) {
        leave::stay(self.m);
    }

    fn save_failed(&mut self) {
        leave::save_failed();
    }
}

fn reads() -> Reads {
    Reads { loads: sessions::loads(), reading: sessions::loading(), last_read: sessions::last_load_read() }
}

fn steps(m: &Memory, frontend: usize) -> Result<Steps> {
    if m.ptr(frontend)? != m.at(gui_classes::MAIN_MENU) {
        return Err("unexpected main menu object".into());
    }
    let page = m.u32(frontend + main_menu::PAGE)?;
    let application = m.ptr(frontend + main_menu::APPLICATION)?;
    let world = m.ptr(m.at(globals::WORLD))?;
    Ok(Steps {
        loads: sessions::loads(),
        page,
        started: page == main_menu::SETUP_PAGE && setup(m, frontend).and_then(|setup| m.u8(setup + setup::STARTED)).is_ok_and(|b| b != 0),
        accepted: m.u8(frontend + main_menu::LAUNCH_ACCEPTED)? != 0,
        launch: m.u8(frontend + main_menu::LAUNCH)? != 0,
        switching: application != 0 && m.u8(application + application::SWITCH_PENDING)? != 0,
        idler: world != 0 && m.ptr(world + world::MAP_MANAGER)? == frontend,
    })
}

fn back_to_first_page(m: &Memory, frontend: usize) {
    let _ = m.write(frontend + main_menu::CONTINUING, &[0]);
    if m.u8(frontend + main_menu::LAUNCH).is_ok_and(|launch| launch == 0) {
        let _ = m.write(frontend + main_menu::LAUNCH_ACCEPTED, &[0]);
    }
    if m.u32(frontend + main_menu::PAGE).is_ok_and(|page| page == main_menu::SETUP_PAGE) {
        let _ = SHOW_PAGE.call(m, [frontend, main_menu::FIRST_PAGE as usize, 1]);
    }
}

fn setup(m: &Memory, frontend: usize) -> Result<usize> {
    if m.u32(frontend + main_menu::PAGE)? != main_menu::SETUP_PAGE {
        return Err("the main menu did not enter the single player setup".into());
    }
    let interface = m.ptr(m.ptr(frontend + main_menu::PAGES)? + main_menu::SETUP_PAGE as usize * 8)?;
    let page = interface.checked_sub(setup::INTERFACE).filter(|_| interface != 0).ok_or("no single player setup")?;
    if m.ptr(page)? != m.at(setup::CLASS) || m.ptr(interface)? != m.at(setup::INTERFACE_CLASS) || m.ptr(page + setup::MAIN_MENU)? != frontend {
        return Err("unexpected single player setup".into());
    }
    Ok(page)
}

fn start_now(m: &Memory, frontend: usize, letters: [u8; 3]) -> Result<()> {
    m.write(world_now(m)? + world::MULTIPLAYER, &0i32.to_le_bytes())?;
    m.write(frontend + main_menu::CONTINUING, &[0])?;
    if letters == save_file::OBSERVER {
        return observe_now(m, frontend);
    }
    let start_tag = m.at(setup::START_TAG);
    set_text(m, start_tag, &letters)?;
    let shown = SHOW_PAGE.call(m, [frontend, main_menu::SETUP_PAGE as usize, 0]);
    let left = game_string::read(m, start_tag);
    if left.as_ref().map_or(true, |text| !text.is_empty()) {
        let _ = set_text(m, start_tag, b"");
    }
    shown?;
    let page = setup(m, frontend)?;
    if !left?.is_empty() {
        return Err("the single player setup did not take the country".into());
    }
    if m.u8(page + setup::STARTED)? == 0 {
        return Err("the single player setup did not start the game".into());
    }
    Ok(())
}

fn observe_now(m: &Memory, frontend: usize) -> Result<()> {
    let start_tag = m.at(setup::START_TAG);
    if !game_string::read(m, start_tag)?.is_empty() {
        set_text(m, start_tag, b"")?;
    }
    SHOW_PAGE.call(m, [frontend, main_menu::SETUP_PAGE as usize, 0])?;
    let page = setup(m, frontend)?;
    let world = world_now(m)?;
    let none = u64::from_le_bytes(*b"---\0\0\0\0\0");
    let observer = KIND_TAG.call(m, [world, &none as *const u64 as usize, world::OBSERVE as usize])?;
    let observer = Tag(m.u64(observer)?);
    if observer.kind() != world::OBSERVE || !observer.valid() {
        return Err(format!("the game gave {:#x} for the observer's tag", observer.0));
    }
    SELECT.call(m, [page, &observer.0 as *const u64 as usize, 0])?;
    START.call(m, [page])?;
    if m.u8(page + setup::STARTED)? == 0 {
        return Err("the single player setup did not start the game".into());
    }
    Ok(())
}

fn load_now(m: &Memory, frontend: usize, path: &str, player: Tag) -> Result<()> {
    // Wait for any background compression before `CHOOSE_SAVE` (`0x1132ca0`)
    // opens the save file via PhysFS.
    crate::session::saves::wait_background_save();
    let world = world_now(m)?;
    m.write(world + world::MULTIPLAYER, &0i32.to_le_bytes())?;
    m.write(world + world::SETUP, &1i32.to_le_bytes())?;
    m.write(frontend + main_menu::CONTINUING, &[1])?;
    SHOW_PAGE.call(m, [frontend, main_menu::SETUP_PAGE as usize, 0])?;
    let page = setup(m, frontend)?;
    m.write(page + setup::SAVE_INDEX, &(-1i32).to_le_bytes())?;
    set_text(m, page + setup::SAVE_PATH, path.as_bytes())?;
    m.write(page + setup::CLOUD_SAVE, &[0])?;
    m.write(page + setup::SAVE_PLAYER, &player.0.to_le_bytes())?;
    CHOOSE_SAVE.call(m, [page])?;
    let chosen = |why: String| format!("{why}; the game still reads the save into the main menu's world, which starts no game");
    SET_PLAYER.call(m, [world, player.0 as usize]).map_err(chosen)?;
    CONTINUE.call(m, [page, 1]).map_err(chosen)?;
    if m.u8(page + setup::STARTED).map_err(chosen)? == 0 {
        return Err(chosen("the single player setup did not start the game".into()));
    }
    Ok(())
}

fn set_text(m: &Memory, string: usize, text: &[u8]) -> Result<()> {
    game_string::read(m, string)?;
    unsafe { game_string::truncate(string as *mut u8, 0) };
    if text.is_empty() {
        return Ok(());
    }
    game_string::append_checked(m, string, text)
}

fn ask_return(m: &Memory) -> GResult<()> {
    let Some(manager) = map_manager(m)? else {
        return fail(Status::NotInGame, "no game is going on");
    };
    m.write(manager + manager_fields::RETURN, &[1])?;
    Ok(())
}

pub(crate) fn points() -> Vec<Point> {
    let mut points = leave::points();
    points.extend([
        Point::keep(SHOW_PAGE, "48 89 5c 24 10 48 89 74 24 20 55 57 41 54 41 56 41 57 48 8d 6c 24 e0 48", "main menu show page"),
        Point::keep(CHOOSE_SAVE, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec", "single player setup choose save"),
        Point::keep(CONTINUE, "48 89 5c 24 08 55 56 57 41 56 41 57 48 8d 6c 24 c9 48 81 ec 00 01 00 00", "single player setup continue"),
        Point::keep(SET_PLAYER, "40 57 48 83 ec 20 48 8b c2 48 89 91 60 1e 00 00 48 c1 e8 38 48 8b f9 84", "world set player").uses(&[world::PLAYER]),
        Point::keep(SELECT, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 56 41 57 48 8b ec 48 83 ec 70", "single player setup select"),
        Point::keep(KIND_TAG, "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8d 6c 24 d0 48 81 ec 30 01 00 00 45 0f b6 f8", "tag of a kind"),
        Point::keep(
            START,
            "48 89 5c 24 18 55 56 57 41 56 41 57 48 8d 6c 24 a0 48 81 ec 60 01 00 00 48 8b f9 45 33 ff 44 89 bd 90 00 00 00 c6 81 b0 2c 00 00 01",
            "single player setup start",
        )
        .uses(&[setup::STARTED]),
        Point::keep(0x1137502usize, "66 c7 44 24 78 2d 2d c7 44 24 7a 2d 00 00 00", "observe button no tag"),
        Point::keep(0x1137528usize, "41 b0 07 48 8d 54 24 78 e8 0b 79 63 ff", "observe button observer's tag").uses(&[world::OBSERVE as usize]),
        Point::keep(
            0x110e863usize,
            "45 33 c0 48 8d 95 f8 00 00 00 48 8d 4e d8 e8 0a e3 00 00 45 33 c0 48 8d 15 d0 98 b2 00 48 8b cf e8 88 68 f8 fe 48 8d 4e d8 e8 ef 19 01 00",
            "start tag select and start",
        ),
    ]);
    points
}
