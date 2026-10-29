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
}

/// What came of the last load or start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Launch {
    /// It has not run yet, or the game is starting.
    #[default]
    Waiting,
    /// It failed, and why.
    Failed(String),
}

/// How long after a load or start the main menu may stay as it was before
/// the game takes it up: its start command runs at a later frame.
const GRACE: Duration = Duration::from_secs(5);

#[derive(Clone)]
enum Action {
    /// The save's PhysFS path, and its player's country.
    Load(String, Tag),
    Start([u8; 3]),
}

struct Attempt {
    action: Action,
    /// When it ran in the main menu's frame, and how.
    ran: Option<(Instant, std::result::Result<(), String>)>,
}

/// The last load or start asked for.
static ATTEMPT: Mutex<Option<Attempt>> = Mutex::new(None);

fn attempt<T>(f: impl FnOnce(&mut Option<Attempt>) -> T) -> T {
    f(&mut ATTEMPT.lock().unwrap_or_else(|e| e.into_inner()))
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
/// control asked for one a moment ago, or the game's own flags say so (an
/// accepted start, the start at the next frame, an idler switch pending), or
/// a save is being read.
pub(crate) fn starting(m: &Memory, frontend: usize) -> bool {
    let asked = attempt(|a| match a {
        Some(Attempt { ran: None, .. }) => true,
        Some(Attempt { ran: Some((when, Ok(()))), .. }) => when.elapsed() < GRACE,
        _ => false,
    });
    asked || crate::saves::load_in_progress() || game_starting(m, frontend).unwrap_or(false)
}

/// The game's own flags for a game being started from the main menu.
fn game_starting(m: &Memory, frontend: usize) -> Result<bool> {
    let application = m.ptr(frontend + main_menu::APPLICATION)?;
    Ok(m.u8(frontend + main_menu::LAUNCH_ACCEPTED)? != 0
        || m.u8(frontend + main_menu::LAUNCH)? != 0
        || (application != 0 && m.u8(application + application::SWITCH_PENDING)? != 0))
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
        saves.push(Save { name: name.to_string(), file: file.clone(), bytes: meta.len(), modified, player: header.player, date: header.date });
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
        return *header;
    }
    let header = save_file::read(path).unwrap_or_default();
    READ.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(key, header);
    header
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
    if m.u32(frontend + main_menu::PAGE)? != 0 {
        return fail(Status::Unavailable, "only from the main menu's first page");
    }
    Ok(frontend)
}

/// Loads the save `name` from the main menu and plays it, at the main
/// menu's next frame; [`launch`] says what came of it.
pub fn load(_: Caller, name: &str) -> GResult<()> {
    let m = game()?;
    let file = save_file(name)?;
    first_page(m)?;
    let header = save_file::read(&save_games()?.join(&file)).map_err(|e| super::Fail { status: Status::Rejected, message: format!("the save cannot be read: {e}") })?;
    let Some(letters) = header.player else {
        return fail(Status::Rejected, "the save has no country to play");
    };
    let player = find(m, letters)?;
    attempt(|a| *a = Some(Attempt { action: Action::Load(format!("save games/{file}"), player), ran: None }));
    Ok(())
}

/// Starts a new game as the country `letters` from the main menu, at the
/// main menu's next frame; [`launch`] says what came of it.
pub fn start(_: Caller, letters: [u8; 3]) -> GResult<()> {
    let m = game()?;
    first_page(m)?;
    find(m, letters)?;
    attempt(|a| *a = Some(Attempt { action: Action::Start(letters), ran: None }));
    Ok(())
}

/// What came of the last load or start, at a safe point outside a game's
/// frame.
pub fn launch(_: Caller) -> Launch {
    let ran = attempt(|a| a.as_ref().map(|a| a.ran.clone()));
    match ran {
        None => Launch::Failed("nothing is being loaded or started".into()),
        Some(None) => Launch::Waiting,
        Some(Some((_, Err(why)))) => Launch::Failed(why),
        Some(Some((when, Ok(())))) => match (at(), crate::memory()) {
            (At::MainMenu(frontend), Some(m)) if when.elapsed() >= GRACE && !game_starting(m, frontend).unwrap_or(false) => {
                Launch::Failed("the game did not start it: the main menu is open".into())
            }
            _ => Launch::Waiting,
        },
    }
}

/// At the end of the main menu's frame: runs the load or start asked for.
pub(crate) fn main_menu_frame(m: &Memory, frontend: usize) {
    let Some(action) = attempt(|a| match a {
        Some(Attempt { action, ran: None }) => Some(action.clone()),
        _ => None,
    }) else {
        return;
    };
    let result = if m.ptr(frontend).ok() != Some(m.at(gui_classes::MAIN_MENU)) {
        Err("unexpected main menu object".to_string())
    } else if m.u32(frontend + main_menu::PAGE).ok() != Some(0) {
        Err("the main menu left its first page before the game could take it up".to_string())
    } else {
        match &action {
            Action::Load(path, player) => load_now(m, frontend, path, *player),
            Action::Start(letters) => start_now(m, frontend, *letters),
        }
    };
    match &result {
        Ok(()) => log!("menu: {} started", describe(&action)),
        Err(why) => {
            log!("menu: {} failed: {why}", describe(&action));
            let _ = m.write(frontend + main_menu::CONTINUING, &[0]);
            // Back to the first page, where the next one can begin.
            if m.u32(frontend + main_menu::PAGE).is_ok_and(|page| page != 0) {
                let _ = m.call(m.at(SHOW_PAGE), &[frontend, 0, 1]);
            }
        }
    }
    attempt(|a| *a = Some(Attempt { action, ran: Some((Instant::now(), result)) }));
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
/// player confirms it: the game's next frame starts `eu4.exe` again.
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
    use super::save_name;

    #[test]
    fn a_save_is_named_without_its_extension() {
        assert_eq!(save_name("Aragon-1.eu4"), Some("Aragon-1"));
        assert_eq!(save_name("奥斯曼1444_11_11.EU4"), Some("奥斯曼1444_11_11"));
        assert_eq!(save_name(".eu4"), None);
        assert_eq!(save_name("notes.txt"), None);
        assert_eq!(save_name("中.eu"), None);
    }
}
