//! The main menu, for remote control (ADR 0015): where the game is, the
//! saves, loading one, starting a new game and returning to the main menu
//! (docs/research/main-menu.md).
//!
//! Each does what the game's own flows do, on the main thread:
//!
//! * a save is loaded into the single player setup and played as the
//!   continue box does it;
//! * a new game starts as the `-start_tag` launch option makes the single
//!   player setup start one: the country is put where that option leaves it,
//!   and the setup, on being entered, selects it and starts;
//! * returning to the main menu sets the flag the game's own question sets,
//!   and the game's next frame starts `eu4.exe` again.
//!
//! The main menu's part runs in the main menu's frame ([`main_menu_frame`]),
//! where the game's own clicks run, not at the safe point that asked for it,
//! which is inside the frame being presented.
mod save_file;

use super::{fail, game, Caller, GResult, Status, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        application,
        campaign::{classes as campaign_classes, countries as country_list, country as country_fields, globals, map_manager, world},
        gui::{classes as gui_classes, main_menu},
        setup,
    },
    points::Point,
    Result,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

/// `(main menu, page, animate)`: shows a page of the main menu.
const SHOW_PAGE: usize = 0x10daf60;
/// `(setup)`: loads the save [`setup::SAVE_PATH`] names into the setup (by
/// a command, at a later frame) and selects [`setup::SAVE_PLAYER`]'s
/// country, or the world's player for an ironman save.
const CHOOSE_SAVE: usize = 0x1130f40;
/// `(setup, true)`: starts the game as the continue box does.
const CONTINUE: usize = 0x1120740;
/// `(world, tag)`: makes the country the player's.
const SET_PLAYER: usize = 0x763840;

/// Where the safe point that runs remote control is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// A game's frame (also while the player only observes).
    Game,
    /// The main menu's frame ran since the last frame was presented; the
    /// main menu object.
    MainMenu(usize),
    /// Neither: a game being set up or loaded, or a frame that is not ready.
    Elsewhere,
}

const ELSEWHERE: u8 = 0;
const MAIN_MENU: u8 = 1;
const GAME: u8 = 2;
static AT: AtomicU8 = AtomicU8::new(ELSEWHERE);
static FRONTEND: AtomicUsize = AtomicUsize::new(0);

/// Says where the safe point that is about to run remote control is.
pub(crate) fn set_at(at: At) {
    let (kind, frontend) = match at {
        At::Game => (GAME, 0),
        At::MainMenu(frontend) => (MAIN_MENU, frontend),
        At::Elsewhere => (ELSEWHERE, 0),
    };
    FRONTEND.store(frontend, Ordering::Relaxed);
    AT.store(kind, Ordering::Relaxed);
}

fn at() -> At {
    match AT.load(Ordering::Relaxed) {
        GAME => At::Game,
        MAIN_MENU => At::MainMenu(FRONTEND.load(Ordering::Relaxed)),
        _ => At::Elsewhere,
    }
}

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
    /// when it cannot be read (or for an observer's save).
    pub player: Option<[u8; 3]>,
    pub date: Option<i32>,
    /// The mods it was made with that the game did not start with, by name:
    /// [`load`] refuses it while there are any.
    pub missing_mods: Vec<String>,
}

/// What came of a load or start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Launch {
    /// It has not run yet, or the game is starting.
    #[default]
    Waiting,
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

/// How long the main menu's frames may go on, after a load or start ran in
/// one, with none of the game's steps towards a game ([`Steps`]) changing,
/// before the load or start is given up. The game took its steps at most
/// 1.8 s apart in the runs (the setup's commands run at a later frame), and
/// a save being read, which no frame shows, is a step of its own; 30 s
/// leaves a slow machine more than 15 times that, and a client still hears
/// within half a minute of a start the game will not carry out.
const QUIET: Duration = Duration::from_secs(30);

#[derive(Clone)]
enum Action {
    /// The save's PhysFS path, and its player's country.
    Load(String, Tag),
    Start([u8; 3]),
}

/// What the main menu's frame sees of a game being started from it; each
/// change is a step towards the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Steps {
    /// The saves the game began to read: a save is read with no frame
    /// presented, so only the count shows it.
    loads: usize,
    page: u32,
    /// The single player setup shows, and its start was made: by remote
    /// control or by the player.
    started: bool,
    /// The main menu accepted the start (its start command ran).
    accepted: bool,
    /// The game starts at the main menu's next frame.
    launch: bool,
    /// The application's idler switch is pending.
    switching: bool,
    /// The main menu is the world's idler (in the world's map manager slot,
    /// `campaign::world::MAP_MANAGER`): a game's map manager takes the slot
    /// when it is made, at the start, before the main menu goes.
    idler: bool,
}

impl Steps {
    fn starting(&self) -> bool {
        self.started || self.accepted || self.launch || self.switching || !self.idler
    }
}

enum Stage {
    /// Runs at the main menu's next frame.
    Asked,
    /// Ran there, and the game takes it up: what the main menu's frame saw
    /// last, and since when; and how many saves the game had begun to read
    /// when it ran.
    Taken { seen: Steps, since: Instant, loads: usize },
    Failed(String),
}

struct Attempt {
    /// Counts the attempts, so that one waiting is not told what came of
    /// another.
    number: u64,
    action: Action,
    stage: Stage,
}

/// The last load or start asked for.
static ATTEMPT: Mutex<Option<Attempt>> = Mutex::new(None);

fn attempt<T>(f: impl FnOnce(&mut Option<Attempt>) -> T) -> T {
    f(&mut ATTEMPT.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Asks for `action`, to run at the main menu's next frame.
fn ask(action: Action) {
    attempt(|a| {
        let number = a.as_ref().map_or(1, |a| a.number + 1);
        *a = Some(Attempt { number, action, stage: Stage::Asked });
    });
}

/// Where the game is.
pub fn place(_: Caller) -> GResult<Place> {
    let m = game()?;
    Ok(match at() {
        At::Game => Place::Game,
        At::MainMenu(frontend) if !starting(m, frontend) => Place::MainMenu,
        At::MainMenu(_) | At::Elsewhere => Place::Starting,
    })
}

/// Whether a game is being started from the main menu `frontend`: remote
/// control asked for one that has not run yet, a save is being read, or the
/// game's own state says so (see [`Steps`]), whoever started it. A main menu
/// that no longer reads as one, as when a game takes over within the frame,
/// is not open either.
pub(crate) fn starting(m: &Memory, frontend: usize) -> bool {
    let asked = attempt(|a| matches!(a, Some(Attempt { stage: Stage::Asked, .. })));
    asked || crate::saves::load_in_progress() || steps(m, frontend).map_or(true, |s| s.starting())
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
        loads: crate::saves::loads(),
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
    let dir = SAVE_GAMES.get_or_init(|| Ok(crate::paths::user_dir(&crate::paths::game_dir()?)?.join("save games")));
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
    let At::MainMenu(frontend) = at() else {
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

/// Loads the save `name` from the main menu and plays it, at the main
/// menu's next frame; [`launch`] says what came of it, and the save's
/// header what the game should come to.
pub fn load(_: Caller, name: &str) -> GResult<Expected> {
    let m = game()?;
    // Where the game is first: in a game, any save is refused alike.
    first_page(m)?;
    let file = save_file(name)?;
    let header = save_file::read(&save_games()?.join(&file)).map_err(|e| super::Fail { status: Status::Rejected, message: format!("the save cannot be read: {e}") })?;
    let Some(letters) = header.player else {
        return fail(Status::Rejected, "the save has no country to play");
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
    ask(Action::Load(format!("save games/{file}"), player));
    Ok(Expected { player: letters, date: header.date })
}

/// Starts a new game as the country `letters` from the main menu, at the
/// main menu's next frame; [`launch`] says what came of it. Refused for a
/// country with no land at the start date, which the single player setup
/// does not offer: started so, the game is left to the player to observe.
pub fn start(_: Caller, letters: [u8; 3]) -> GResult<()> {
    let m = game()?;
    first_page(m)?;
    let tag = find(m, letters)?;
    if super::world::owned_provinces(m, world_now(m)?, tag)? == 0 {
        let name = String::from_utf8_lossy(&letters).into_owned();
        return fail(Status::Rejected, format!("{name} has no land at the start date, so it cannot be played from it; started as {name}, the game would only be observed"));
    }
    ask(Action::Start(letters));
    Ok(())
}

/// The last load or start asked for, to ask [`launch`] about.
pub fn last(_: Caller) -> u64 {
    attempt(|a| a.as_ref().map_or(0, |a| a.number))
}

/// What came of the load or start `number` ([`last`]), at a safe point
/// outside a game's frame.
pub fn launch(_: Caller, number: u64) -> Launch {
    attempt(|a| match a {
        Some(Attempt { number: n, action, stage }) if *n == number => match stage {
            Stage::Failed(why) => Launch::Failed(why.clone()),
            Stage::Taken { loads, .. } if matches!(action, Action::Load(..)) && unread(*loads) => Launch::Failed(UNREAD.into()),
            Stage::Asked | Stage::Taken { .. } => Launch::Waiting,
        },
        _ => Launch::Failed("another load or start was asked for".into()),
    })
}

const UNREAD: &str = "the game could not read the save: it starts a game anyway, from what it read of it (menu and session say where it is)";

/// Whether the game read a save since a load ran, `loads` saves read
/// before, and could not read it: its loader said it failed. The game goes
/// on and starts a game from what it read (seen in the game with a save
/// whose game state was damaged).
fn unread(loads: usize) -> bool {
    crate::saves::loads() > loads && !crate::saves::load_in_progress() && !crate::saves::last_load_read()
}

/// At the end of the main menu's frame: runs the load or start asked for,
/// then follows the game taking it up, and gives it up when the main menu
/// shows with nothing started from it, or when the game takes no step for
/// [`QUIET`].
pub(crate) fn main_menu_frame(m: &Memory, frontend: usize) {
    enum Now {
        Run(Action),
        Follow(Action, Steps, Instant, usize),
    }
    let Some((number, now)) = attempt(|a| match a {
        Some(Attempt { number, action, stage: Stage::Asked }) => Some((*number, Now::Run(action.clone()))),
        Some(Attempt { number, action, stage: Stage::Taken { seen, since, loads } }) => Some((*number, Now::Follow(action.clone(), *seen, *since, *loads))),
        _ => None,
    }) else {
        return;
    };
    let stage = match now {
        Now::Run(action) => run(m, frontend, &action),
        Now::Follow(action, seen, since, loads) => follow(m, frontend, &action, seen, since, loads),
    };
    attempt(|a| {
        if let Some(a) = a.as_mut().filter(|a| a.number == number) {
            a.stage = stage;
        }
    });
}

/// Runs a load or start in the main menu's frame.
fn run(m: &Memory, frontend: usize, action: &Action) -> Stage {
    let result = if m.ptr(frontend).ok() != Some(m.at(gui_classes::MAIN_MENU)) {
        // Not the main menu the framework knows: nothing is touched.
        Err("unexpected main menu object".to_string())
    } else if m.u32(frontend + main_menu::PAGE).ok() != Some(main_menu::FIRST_PAGE) {
        // The player went to another page meanwhile, and stays there.
        Err("the main menu left its first page before the game could take it up".to_string())
    } else {
        let result = match action {
            Action::Load(path, player) => load_now(m, frontend, path, *player),
            Action::Start(letters) => start_now(m, frontend, *letters),
        };
        match result {
            Ok(()) => steps(m, frontend),
            Err(why) => {
                back_to_first_page(m, frontend);
                Err(why)
            }
        }
    };
    match result {
        Ok(seen) => {
            log!("menu: {} started", describe(action));
            Stage::Taken { seen, since: Instant::now(), loads: seen.loads }
        }
        Err(why) => {
            log!("menu: {} failed: {why}", describe(action));
            Stage::Failed(why)
        }
    }
}

/// Follows the game taking up a load or start that ran, at a main menu's
/// frame: the main menu still shows, so the game has not switched to the
/// game yet.
fn follow(m: &Memory, frontend: usize, action: &Action, seen: Steps, since: Instant, loads: usize) -> Stage {
    let why = match steps(m, frontend) {
        // Not the main menu the framework knows: nothing is touched.
        Err(e) => e.to_string(),
        Ok(now) => match judge(&seen, &now, crate::saves::load_in_progress(), since.elapsed()) {
            Judged::Step => return Stage::Taken { seen: now, since: Instant::now(), loads },
            Judged::Wait => return Stage::Taken { seen, since, loads },
            Judged::Open => {
                // The single player setup was left, by the player: the page
                // stays.
                let _ = m.write(frontend + main_menu::CONTINUING, &[0]);
                "the game did not start it: the main menu is open".to_string()
            }
            Judged::Quiet => {
                back_to_first_page(m, frontend);
                format!("the game did not start it: it took no step towards it for {} s", QUIET.as_secs())
            }
        },
    };
    log!("menu: {} failed: {why}", describe(action));
    Stage::Failed(why)
}

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
        let _ = m.call(m.at(SHOW_PAGE), &[frontend, main_menu::FIRST_PAGE as usize, 1]);
    }
}

fn describe(action: &Action) -> String {
    match action {
        Action::Load(path, _) => format!("loading {path}"),
        Action::Start(letters) => format!("a new game as {}", String::from_utf8_lossy(letters)),
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

fn world_now(m: &Memory) -> Result<usize> {
    let world = m.ptr(m.at(globals::WORLD))?;
    if world == 0 {
        return Err("no world".into());
    }
    Ok(world)
}

/// As the `-start_tag` launch option: the country where the single player
/// setup looks for it, then the setup entered, which selects it and starts.
fn start_now(m: &Memory, frontend: usize, letters: [u8; 3]) -> Result<()> {
    m.write(world_now(m)? + world::MULTIPLAYER, &0i32.to_le_bytes())?;
    // Continuing a save, the setup would leave the country where it is.
    m.write(frontend + main_menu::CONTINUING, &[0])?;
    let start_tag = m.at(setup::START_TAG);
    set_text(m, start_tag, &letters)?;
    // Without the setup's animation, as the continue box shows it.
    let shown = m.call(m.at(SHOW_PAGE), &[frontend, main_menu::SETUP_PAGE as usize, 0]);
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

/// As the continue box, for the save at `path` played as `player`, all at
/// one frame: the single player setup entered, the save and its player
/// written into it, the save chosen (the setup loads it by a command, at a
/// later frame), the country made the player, the game started, by a
/// command that runs after the load.
fn load_now(m: &Memory, frontend: usize, path: &str, player: Tag) -> Result<()> {
    let world = world_now(m)?;
    m.write(world + world::MULTIPLAYER, &0i32.to_le_bytes())?;
    m.write(world + world::SETUP, &1i32.to_le_bytes())?;
    m.write(frontend + main_menu::CONTINUING, &[1])?;
    m.call(m.at(SHOW_PAGE), &[frontend, main_menu::SETUP_PAGE as usize, 0])?;
    let page = setup(m, frontend)?;
    m.write(page + setup::SAVE_INDEX, &(-1i32).to_le_bytes())?;
    set_text(m, page + setup::SAVE_PATH, path.as_bytes())?;
    m.write(page + setup::CLOUD_SAVE, &[0])?;
    m.write(page + setup::SAVE_PLAYER, &player.0.to_le_bytes())?;
    m.call(m.at(CHOOSE_SAVE), &[page])?;
    m.call(m.at(SET_PLAYER), &[world, player.0 as usize])?;
    m.call(m.at(CONTINUE), &[page, 1])?;
    if m.u8(page + setup::STARTED)? == 0 {
        return Err("the single player setup did not start the game".into());
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
        if tag.normal() && tag.letters() == letters && tag.index() as usize == index {
            return Ok(tag);
        }
    }
    fail(Status::NotFound, format!("no country {}", String::from_utf8_lossy(&letters)))
}

/// Returns to the main menu, as the game's own question does when the
/// player confirms it: the game's next frame starts `eu4.exe` again. It
/// needs the game's map manager, not a game with a player: it also works
/// while the player only observes, say after their country was annexed.
pub fn return_to_main_menu(_: Caller) -> GResult<()> {
    let m = game()?;
    let world = m.ptr(m.at(globals::WORLD))?;
    let manager = if world == 0 { 0 } else { m.ptr(world + world::MAP_MANAGER)? };
    if manager == 0 || m.ptr(manager)? != m.at(campaign_classes::MAP_MANAGER) {
        return fail(Status::NotInGame, "no game is going on");
    }
    m.write(manager + map_manager::RETURN, &[1])?;
    log!("menu: returning to the main menu");
    Ok(())
}

/// The code the main menu capabilities rely on; the fields they use are in
/// `objects` (`gui::main_menu`, `setup`, `application`, `campaign::world`,
/// `campaign::map_manager`).
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(SHOW_PAGE, "48 89 5c 24 10 48 89 74 24 20 55 57 41 54 41 56 41 57 48 8d 6c 24 e0 48", "main menu show page"),
        Point::keep(CHOOSE_SAVE, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec", "single player setup choose save"),
        Point::keep(CONTINUE, "48 89 5c 24 08 55 56 57 41 56 41 57 48 8d 6c 24 c9 48 81 ec 00 01 00 00", "single player setup continue"),
        Point::keep(SET_PLAYER, "40 57 48 83 ec 20 48 8b c2 48 89 91 60 1e 00 00 48 c1 e8 38 48 8b f9 84", "world set player").uses(&[world::PLAYER]),
        // Entering the single player setup with a `-start_tag` country: it
        // selects the country, empties the text and starts.
        Point::keep(
            0x110e863usize,
            "45 33 c0 48 8d 95 f8 00 00 00 48 8d 4e d8 e8 0a e3 00 00 45 33 c0 48 8d 15 d0 98 b2 00 48 8b cf e8 88 68 f8 fe 48 8d 4e d8 e8 ef 19 01 00",
            "start tag select and start",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{judge, missing_mods, save_file, save_name, Judged, Steps, QUIET};
    use std::time::Duration;

    #[test]
    fn a_save_misses_the_mods_it_was_made_with_that_the_playset_lacks() {
        let save_mod = |file: &str, name: &str| save_file::SaveMod { file: file.into(), name: name.into() };
        let header = save_file::Header {
            mods: vec![save_mod("mod/ugc_3047072888.mod", "彼岸乐园"), save_mod("mod/ugc_2976470733.mod", "Chinese Language Mod for 1.37"), save_mod("mod/local.mod", "")],
            ..Default::default()
        };
        let playset = ["mod/ugc_253263609.mod".to_string(), "MOD/ugc_2976470733.mod".to_string()];
        assert_eq!(missing_mods(&header, &playset), ["彼岸乐园", "mod/local.mod"]);
        // Mods the playset has and the save lacks are no matter.
        assert_eq!(missing_mods(&save_file::Header::default(), &playset), Vec::<String>::new());
        assert_eq!(missing_mods(&header, &[]).len(), 3);
    }

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
    fn a_save_is_named_without_its_extension() {
        assert_eq!(save_name("Aragon-1.eu4"), Some("Aragon-1"));
        assert_eq!(save_name("奥斯曼1444_11_11.EU4"), Some("奥斯曼1444_11_11"));
        assert_eq!(save_name(".eu4"), None);
        assert_eq!(save_name("notes.txt"), None);
        assert_eq!(save_name("中.eu"), None);
    }
}
