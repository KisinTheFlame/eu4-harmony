//! The main menu, for remote control (ADR remote-control-uses-capabilities):
//! where the game is, the saves, loading one, starting a new game and returning
//! to the main menu (docs/research/main-menu.md).
//!
//! Each does what the game's own flows do, on the main thread:
//!
//! * a save is loaded from the main menu into the single player setup and
//!   played as the continue box does it, and from a session by the game's
//!   in-game load, as its load question does it ([`leave`]);
//! * a new game starts as the `-start_tag` launch option makes the single
//!   player setup start one: the country is put where that option leaves it,
//!   and the setup, on being entered, selects it and starts;
//! * returning to the main menu sets the flag the game's own question sets,
//!   and the game's next frame starts `eu4.exe` again.
//!
//! The main menu's part runs in the main menu's frame ([`main_menu_frame`]),
//! where the game's own clicks run, not at the safe point that asked for it,
//! which is inside the frame being presented; an in-game load runs at the
//! next game frame, after the map manager's ([`game_frame`]).
//!
//! Each request is taken from asking to settling by [`course`], over what
//! each step does in the game ([`InGame`]).
mod course;
pub mod leave;

use super::{fail, game, Caller, GResult, Status, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        application,
        campaign::{classes as campaign_classes, countries as country_list, country as country_fields, globals, map_manager as manager_fields, world},
        gui::{classes as gui_classes, main_menu},
        setup,
    },
    points::{GameFunction, Point},
    saves::file as save_file,
    Result,
};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Instant, UNIX_EPOCH},
};

/// `(main menu, page, animate)`: shows a page of the main menu.
const SHOW_PAGE: GameFunction<3> = GameFunction::at(0x10daf60);
/// `(setup)`: loads the save [`setup::SAVE_PATH`] names into the setup (by
/// a command, at a later frame) and selects [`setup::SAVE_PLAYER`]'s
/// country, or the world's player for an ironman save.
const CHOOSE_SAVE: GameFunction<1> = GameFunction::at(0x1130f40);
/// `(setup, true)`: starts the game as the continue box does.
const CONTINUE: GameFunction<2> = GameFunction::at(0x1120740);
/// `(world, tag)`: makes the country the player's.
const SET_PLAYER: GameFunction<2> = GameFunction::at(0x763840);
/// `(world, &tag, kind)`: the tag of that kind for the letters, made when
/// there is none; the setup's observe button (`0x11374d0`) asks it for
/// `---` of the observer's kind, and makes the player the tag it points to.
const KIND_TAG: GameFunction<3> = GameFunction::at(0x76ee40);
/// `(setup, &tag, false)`: selects the country in the setup, posting the
/// command that makes it the player's, as entering the setup with a
/// `-start_tag` country does (`0x110e871`).
const SELECT: GameFunction<3> = GameFunction::at(0x111cb80);
/// `(setup)`: starts the game, as entering the setup with a `-start_tag`
/// country does once it selected it (`0x110e88c`).
const START: GameFunction<1> = GameFunction::at(0x1120280);

use crate::sessions::{self, At, End};
use course::{Action, Course, Reads, Rules, Steps};

/// Where the game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    MainMenu,
    /// A game is being set up or loaded.
    Starting,
    Game,
}

/// A save in the save games folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Save {
    /// The file's name without `.eu4`.
    pub name: String,
    pub file: String,
    pub bytes: u64,
    /// Seconds since 1970.
    pub modified: u64,
    /// The country it is played as, and its date, as its header says; None
    /// when it cannot be read, or for a save of observe mode.
    pub player: Option<[u8; 3]>,
    pub date: Option<i32>,
    /// The mods it was made with that the game did not start with, by name:
    /// [`load`] refuses it while there are any.
    pub missing_mods: Vec<String>,
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

/// What a save being loaded says it is: the player and the date the game
/// should come to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expected {
    pub player: [u8; 3],
    pub date: Option<i32>,
}

/// What a save being loaded says it is, read once the game came: an in-game
/// load from an ironman session writes the session's ironman exit save
/// first, which may be the very save being loaded.
pub type ExpectedProbe = Box<dyn FnMut() -> GResult<Expected> + Send>;

/// Every request made of the main menu and of a session, and its course.
static COURSE: Course = Course::new();

/// Where the game is.
pub fn place(_: Caller) -> GResult<Place> {
    let m = game()?;
    let at = sessions::at();
    Ok(COURSE.place(at, &mut InGame::at(m, at)))
}

/// Whether a game is being started from the main menu `frontend`
/// ([`Course::starting`]).
pub(crate) fn starting(m: &Memory, frontend: usize) -> bool {
    COURSE.starting(&mut InGame { frontend, ..InGame::new(m) })
}

/// The game's own steps of a request and of leaving a session, at the hook
/// or capability that takes them: `frontend` is the main menu object, 0
/// outside the main menu; `request` the in-game load's, at the in-game load;
/// `app` and `argument` the restart's, at the restart.
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
            // The player went to another page meanwhile, and stays there.
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

/// What the main menu `frontend` shows of a game being started from it.
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

static SAVE_GAMES: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();

/// The save games folder, in the user directory.
fn save_games() -> GResult<PathBuf> {
    let dir = SAVE_GAMES.get_or_init(|| Ok(eu4_harmony_folders::UserFolder::new(crate::paths::user_dir(&crate::paths::game_dir()?)?).save_games()));
    dir.clone().map_err(|e| format!("the user directory: {e}").into())
}

/// The saves in the save games folder, newest first.
pub fn saves(_: Caller) -> GResult<Vec<Save>> {
    let dir = save_games()?;
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display()).into()),
    };
    let mut saves = Vec::new();
    for entry in entries.flatten() {
        let Ok(file) = entry.file_name().into_string() else { continue };
        let Some(name) = save_name(&file) else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let header = header(&entry.path(), meta.len(), modified);
        let missing_mods = missing_mods(&header, crate::mods::playset());
        saves.push(Save { name: name.to_string(), file: file.clone(), bytes: meta.len(), modified, player: header.player, date: header.date, missing_mods });
    }
    saves.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    Ok(saves)
}

/// What the header of the save at `path` says, read once for each size and
/// time it was written at.
fn header(path: &std::path::Path, bytes: u64, modified: u64) -> save_file::Header {
    type Key = (PathBuf, u64, u64);
    static READ: Mutex<Option<std::collections::HashMap<Key, save_file::Header>>> = Mutex::new(None);
    let key = (path.to_path_buf(), bytes, modified);
    if let Some(header) = READ.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).get(&key) {
        return header.clone();
    }
    let header = save_file::read(path).unwrap_or_default();
    READ.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(key, header.clone());
    header
}

/// The mods a save was made with that `playset`, the mods the game started
/// with, lacks: their names, or their descriptors when they have none.
///
/// A save made with a gameplay mod the playset lacked crashed the game as it
/// loaded (docs/research/main-menu.md), and which mods change the game
/// cannot be told from their names, so every one missing counts. A mod the
/// playset has and the save lacks does not: the saves of the runs loaded
/// with mods added to the playset since.
fn missing_mods(header: &save_file::Header, playset: &[String]) -> Vec<String> {
    header
        .mods
        .iter()
        .filter(|m| !playset.iter().any(|p| crate::mods::same_descriptor(&m.file, p)))
        .map(|m| if m.name.is_empty() { m.file.clone() } else { m.name.clone() })
        .collect()
}

/// The name of a save file: its name without `.eu4`, when it has that.
fn save_name(file: &str) -> Option<&str> {
    let stem = file.len().checked_sub(4).filter(|&n| file.is_char_boundary(n))?;
    file[stem..].eq_ignore_ascii_case(".eu4").then(|| &file[..stem]).filter(|name| !name.is_empty())
}

/// The file of the save named `name` (as [`saves`] lists it, `.eu4` allowed).
fn save_file(name: &str) -> GResult<String> {
    let name = save_name(name).unwrap_or(name);
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
        return fail(Status::InvalidArgument, format!("{name:?} is not the name of a save"));
    }
    let file = format!("{name}.eu4");
    if !save_games()?.join(&file).is_file() {
        return fail(Status::NotFound, format!("no save {name:?} in the save games folder"));
    }
    Ok(file)
}

/// The main menu at this safe point, on its first page, with nothing being
/// started from it.
fn first_page(m: &Memory) -> GResult<usize> {
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

/// Loads the save `name` and plays it: from the main menu at its next frame,
/// from a session by an in-game load at the next game frame; [`launch`]
/// says what came of it, and the save's header what the game should come to.
pub fn load(_: Caller, name: &str) -> GResult<ExpectedProbe> {
    let m = game()?;
    // Where the game is first: a save is refused alike wherever it cannot be loaded.
    let in_game = sessions::at() == At::Game;
    if in_game {
        if COURSE.loading_in_game() {
            return fail(Status::Unavailable, "a save is already being loaded");
        }
    } else {
        first_page(m)?;
    }
    let file = save_file(name)?;
    let path = save_games()?.join(&file);
    let header = read_header(&path)?;
    let Some(letters) = player_of(&header) else {
        return fail(Status::Rejected, "the save has no player");
    };
    let missing = missing_mods(&header, crate::mods::playset());
    if !missing.is_empty() {
        return fail(
            Status::Rejected,
            format!(
                "the save was made with mods the game did not start with: {}; loading it can crash the game. Add them to the playset in the launcher and start the game again, or load another save",
                missing.join(", ")
            ),
        );
    }
    let player = find(m, letters)?;
    COURSE.ask(if in_game { Action::LoadInGame(save_name(&file).unwrap_or(&file).to_string()) } else { Action::Load(format!("save games/{file}"), player) });
    Ok(Box::new(move || {
        let header = read_header(&path)?;
        match player_of(&header) {
            Some(player) => Ok(Expected { player, date: header.date }),
            None => fail(Status::Rejected, "the save the game loaded has no player"),
        }
    }))
}

/// The player's own tag a save's header says: its country, or in a save of
/// observe mode the observer's.
fn player_of(header: &save_file::Header) -> Option<[u8; 3]> {
    header.player.or(header.observing.then_some(save_file::OBSERVER))
}

fn read_header(path: &Path) -> GResult<save_file::Header> {
    save_file::read(path).map_err(|e| super::Fail { status: Status::Rejected, message: format!("the save cannot be read: {e}") })
}

/// At a game frame, after the map manager's: runs the in-game load remote
/// control asked for, if there is one; then the frame is over.
pub(crate) fn game_frame(m: &Memory) -> bool {
    COURSE.game_frame(&mut InGame::new(m))
}

/// Starts a new game as the country `letters` from the main menu, at the
/// main menu's next frame; [`launch`] says what came of it. Refused for a
/// country with no land at the start date, which the single player setup
/// does not offer: started so, the game is left to the player to observe.
/// From a session, the session is left as a return to the main menu leaves
/// it, ending for the new game (`new_game`), and the game starts at the new
/// main menu's first frame; the land is
/// not looked at then, nor after an in-process return before the single
/// player setup is entered, since the world has no history yet.
pub fn start(_: Caller, country: Option<[u8; 3]>) -> GResult<()> {
    let m = game()?;
    // Observe mode: a new game as the observer's tag, the player's own then.
    let letters = country.unwrap_or(save_file::OBSERVER);
    if sessions::at() == At::Game {
        let known = || if country.is_some() { find(m, letters).map(drop) } else { Ok(()) };
        COURSE.new_game_from_session(&mut InGame::new(m), letters, known)?;
        return Ok(());
    }
    first_page(m)?;
    if country.is_none() {
        // The setup's ironman choice is the main menu world's, which the
        // game does not let observe (FE_NOT_IRONMAN_AND_OBSERVER).
        if m.u8(world_now(m)? + world::IRONMAN)? != 0 {
            return fail(Status::Rejected, "an ironman game cannot be observed; turn ironman off in the single player setup");
        }
        log!("menu: a new game in observe mode");
        COURSE.ask(Action::Start(letters));
        return Ok(());
    }
    let tag = find(m, letters)?;
    if super::world::owned_provinces(m, world_now(m)?, tag)? == 0 {
        let name = String::from_utf8_lossy(&letters).into_owned();
        return fail(
            Status::Rejected,
            format!("{name} has no land at the start date, so it cannot be played from it; started as {name}, the game would only be observed"),
        );
    }
    COURSE.ask(Action::Start(letters));
    Ok(())
}

/// The last load or start asked for, to ask [`launch`] about.
pub fn last(_: Caller) -> u64 {
    COURSE.last()
}

/// What came of the load or start `number` ([`last`]), at a safe point
/// outside a game's frame.
pub fn launch(_: Caller, number: u64) -> Launch {
    COURSE.outcome(number, reads())
}

/// At the end of the main menu's frame: runs the load or start asked for,
/// and follows the game taking it up ([`Course::main_menu_frame`]).
pub(crate) fn main_menu_frame(m: &Memory, frontend: usize) {
    COURSE.main_menu_frame(&mut InGame { frontend, ..InGame::new(m) });
}

/// After a load or start that did not go through: the main menu as the
/// player could use it, nothing continued, no start accepted and, when the
/// single player setup it entered for it still shows, its first page.
/// Another page, which only the player can have chosen, stays.
///
/// A start the game accepted and did not carry out leaves the main menu's
/// flag for it set, which nothing but its constructor clears: `menu` would
/// say `starting` for the rest of the process, and the setup, entered again,
/// would carry the start out at its first frame (`0x11192d5`). Unless the
/// start is to happen at the next frame, the flag is cleared, as the main
/// menu was made.
fn back_to_first_page(m: &Memory, frontend: usize) {
    let _ = m.write(frontend + main_menu::CONTINUING, &[0]);
    if m.u8(frontend + main_menu::LAUNCH).is_ok_and(|launch| launch == 0) {
        let _ = m.write(frontend + main_menu::LAUNCH_ACCEPTED, &[0]);
    }
    if m.u32(frontend + main_menu::PAGE).is_ok_and(|page| page == main_menu::SETUP_PAGE) {
        let _ = SHOW_PAGE.call(m, [frontend, main_menu::FIRST_PAGE as usize, 1]);
    }
}

/// The single player setup page, once the main menu shows it.
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

/// The world, when there is one.
fn world_now(m: &Memory) -> Result<usize> {
    let world = m.ptr(m.at(globals::WORLD))?;
    if world == 0 {
        return Err("no world".into());
    }
    Ok(world)
}

/// The session's map manager, the world's, checked by its class; None when
/// no game is going on.
fn map_manager(m: &Memory) -> Result<Option<usize>> {
    let world = m.ptr(m.at(globals::WORLD))?;
    let manager = if world == 0 { 0 } else { m.ptr(world + world::MAP_MANAGER)? };
    Ok((manager != 0 && m.ptr(manager)? == m.at(campaign_classes::MAP_MANAGER)).then_some(manager))
}

/// As the `-start_tag` launch option: the country where the single player
/// setup looks for it, then the setup entered, which selects it and starts.
fn start_now(m: &Memory, frontend: usize, letters: [u8; 3]) -> Result<()> {
    m.write(world_now(m)? + world::MULTIPLAYER, &0i32.to_le_bytes())?;
    // Continuing a save, the setup would leave the country where it is.
    m.write(frontend + main_menu::CONTINUING, &[0])?;
    if letters == save_file::OBSERVER {
        return observe_now(m, frontend);
    }
    let start_tag = m.at(setup::START_TAG);
    set_text(m, start_tag, &letters)?;
    // Without the setup's animation, as the continue box shows it.
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

/// As the single player setup's observe button and then its start: the
/// setup entered with no country to start as, the observer's tag selected
/// in it as a `-start_tag` country is (the button posts the same command),
/// the game started.
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

/// As the continue box, for the save at `path` played as `player`, all at
/// one frame: the single player setup entered, the save and its player
/// written into it, the save chosen (the setup loads it by a command, at a
/// later frame), the country made the player, the game started, by a
/// command that runs after the load.
///
/// What can fail is checked before the save is chosen. Once it is, the
/// command that loads it is the game's, and nothing takes it back: should
/// what follows fail (a game function faulting, or the setup not starting,
/// which its code always does with a save chosen), the save is still read
/// into the main menu's world at a later frame, where the main menu shows
/// its first page again. That starts no game, and a later load or start
/// replaces it, as the runs saw of a load and a start at separate frames
/// (docs/research/main-menu.md); the failure says so.
fn load_now(m: &Memory, frontend: usize, path: &str, player: Tag) -> Result<()> {
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

/// Makes the game's string at `string` hold `text`, through the game's own
/// append.
fn set_text(m: &Memory, string: usize, text: &[u8]) -> Result<()> {
    game_string::read(m, string)?;
    unsafe { game_string::truncate(string as *mut u8, 0) };
    if text.is_empty() {
        return Ok(());
    }
    game_string::append_checked(m, string, text)
}

/// The country whose tag is `letters`, among the countries the main menu's
/// world has (every tag there is, whether or not it exists at the start).
fn find(m: &Memory, letters: [u8; 3]) -> GResult<Tag> {
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
        // The observer's tag is of the rebels' kind, which no country is.
        let player = tag.normal() || (letters == save_file::OBSERVER && tag.valid() && tag.kind() == world::OBSERVE);
        if player && tag.letters() == letters && tag.index() as usize == index {
            return Ok(tag);
        }
    }
    Err(eu4_harmony_checks::countries::not_found(&String::from_utf8_lossy(&letters)).into())
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

/// What came of the return remote control asked for as `number`.
pub fn returned(_: Caller, number: u64) -> Returned {
    COURSE.returned(number, || matches!(place(Caller::Remote), Ok(Place::MainMenu)))
}

/// Sets the flag the game's own question sets when the player confirms a
/// return: the map manager acts on it at its next frame ([`leave`]).
fn ask_return(m: &Memory) -> GResult<()> {
    let Some(manager) = map_manager(m)? else {
        return fail(Status::NotInGame, "no game is going on");
    };
    m.write(manager + manager_fields::RETURN, &[1])?;
    Ok(())
}

/// Returns to the main menu, as the game's own question does when the
/// player confirms it. With "return without restarting" on it is an
/// in-process return, whose number [`returned`] takes, the same for every
/// request made before the map manager's frame acts on it; otherwise the game's
/// next frame starts `eu4.exe` again (None). It needs the game's map
/// manager, not a game with a player: it also works while the player only
/// observes, say after their country was annexed.
pub fn return_to_main_menu(_: Caller) -> GResult<Option<u64>> {
    COURSE.return_to_main_menu(&mut InGame::new(game()?))
}

/// The code the main menu capabilities rely on; the fields they use are in
/// `objects` (`gui::main_menu`, `setup`, `application`, `campaign::world`,
/// `campaign::map_manager`).
pub fn points() -> Vec<Point> {
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
        // The setup's observe button: the observer's tag, `---` of its kind.
        Point::keep(0x1137502usize, "66 c7 44 24 78 2d 2d c7 44 24 7a 2d 00 00 00", "observe button no tag"),
        Point::keep(0x1137528usize, "41 b0 07 48 8d 54 24 78 e8 0b 79 63 ff", "observe button observer's tag").uses(&[world::OBSERVE as usize]),
        // Entering the single player setup with a `-start_tag` country: it
        // selects the country, empties the text and starts.
        Point::keep(
            0x110e863usize,
            "45 33 c0 48 8d 95 f8 00 00 00 48 8d 4e d8 e8 0a e3 00 00 45 33 c0 48 8d 15 d0 98 b2 00 48 8b cf e8 88 68 f8 fe 48 8d 4e d8 e8 ef 19 01 00",
            "start tag select and start",
        ),
    ]);
    points
}

#[cfg(test)]
mod tests {
    use super::{missing_mods, save_file, save_name};

    #[test]
    fn a_save_misses_the_mods_it_was_made_with_that_the_playset_lacks() {
        let save_mod = |file: &str, name: &str| save_file::SaveMod { file: file.into(), name: name.into() };
        let header = save_file::Header {
            mods: vec![
                save_mod("mod/ugc_3047072888.mod", "彼岸乐园"),
                save_mod("mod/ugc_2976470733.mod", "Chinese Language Mod for 1.37"),
                save_mod("mod/local.mod", ""),
            ],
            ..Default::default()
        };
        let playset = ["mod/ugc_253263609.mod".to_string(), "MOD/ugc_2976470733.mod".to_string()];
        assert_eq!(missing_mods(&header, &playset), ["彼岸乐园", "mod/local.mod"]);
        // Mods the playset has and the save lacks are no matter.
        assert_eq!(missing_mods(&save_file::Header::default(), &playset), Vec::<String>::new());
        assert_eq!(missing_mods(&header, &[]).len(), 3);
    }

    #[test]
    fn a_save_is_named_without_its_extension() {
        assert_eq!(save_name("Aragon-1.eu4"), Some("Aragon-1"));
        assert_eq!(save_name("奥斯曼1444_11_11.EU4"), Some("奥斯曼1444_11_11"));
        assert_eq!(save_name(".eu4"), None);
        assert_eq!(save_name("notes.txt"), None);
        assert_eq!(save_name("中.eu"), None);
    }
}
