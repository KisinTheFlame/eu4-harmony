//! The main menu capabilities, for remote control
//! (ADR remote-control-uses-capabilities): listing saves and delegating
//! session-transition requests (place, load, start, return) to
//! [`crate::session::transition`].
use super::{fail, game, Caller, GResult, Status};
use crate::{
    native::{memory::Memory, objects::campaign::world, points::Point},
    session::{
        saves::file as save_file,
        sessions::{self, At},
        transition,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::UNIX_EPOCH,
};

pub use crate::session::transition::{Launch, Place, Returned};

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

/// Where the game is.
pub fn place(_: Caller) -> GResult<Place> {
    Ok(transition::place(game()?))
}

static SAVE_GAMES: OnceLock<std::result::Result<PathBuf, String>> = OnceLock::new();

/// The save games folder, in the user directory.
fn save_games() -> GResult<PathBuf> {
    let dir = SAVE_GAMES.get_or_init(|| Ok(eu4_harmony_folders::UserFolder::new(crate::base::paths::user_dir(&crate::base::paths::game_dir()?)?).save_games()));
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
        let missing_mods = missing_mods(&header, crate::base::mods::playset());
        saves.push(Save { name: name.to_string(), file: file.clone(), bytes: meta.len(), modified, player: header.player, date: header.date, missing_mods });
    }
    saves.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    Ok(saves)
}

/// What the header of the save at `path` says, read once for each size and
/// time it was written at.
fn header(path: &Path, bytes: u64, modified: u64) -> save_file::Header {
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

fn missing_mods(header: &save_file::Header, playset: &[String]) -> Vec<String> {
    header
        .mods
        .iter()
        .filter(|m| !playset.iter().any(|p| crate::base::mods::same_descriptor(&m.file, p)))
        .map(|m| if m.name.is_empty() { m.file.clone() } else { m.name.clone() })
        .collect()
}

fn save_name(file: &str) -> Option<&str> {
    let stem = file.len().checked_sub(4).filter(|&n| file.is_char_boundary(n))?;
    file[stem..].eq_ignore_ascii_case(".eu4").then(|| &file[..stem]).filter(|name| !name.is_empty())
}

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

/// Loads the save `name` and plays it.
pub fn load(_: Caller, name: &str) -> GResult<ExpectedProbe> {
    let m = game()?;
    let in_game = sessions::at() == At::Game;
    if in_game {
        if transition::loading_in_game() {
            return fail(Status::Unavailable, "a save is already being loaded");
        }
    } else {
        transition::first_page(m)?;
    }
    let file = save_file(name)?;
    let path = save_games()?.join(&file);
    let header = read_header(&path)?;
    let Some(letters) = player_of(&header) else {
        return fail(Status::Rejected, "the save has no player");
    };
    let missing = missing_mods(&header, crate::base::mods::playset());
    if !missing.is_empty() {
        return fail(
            Status::Rejected,
            format!(
                "the save was made with mods the game did not start with: {}; loading it can crash the game. Add them to the playset in the launcher and start the game again, or load another save",
                missing.join(", ")
            ),
        );
    }
    let player = transition::find_country(m, letters)?;
    transition::ask_load(in_game, save_name(&file).unwrap_or(&file).to_string(), format!("save games/{file}"), player);
    Ok(Box::new(move || {
        let header = read_header(&path)?;
        match player_of(&header) {
            Some(player) => Ok(Expected { player, date: header.date }),
            None => fail(Status::Rejected, "the save the game loaded has no player"),
        }
    }))
}

fn player_of(header: &save_file::Header) -> Option<[u8; 3]> {
    header.player.or(header.observing.then_some(save_file::OBSERVER))
}

fn read_header(path: &Path) -> GResult<save_file::Header> {
    save_file::read(path).map_err(|e| super::Fail { status: Status::Rejected, message: format!("the save cannot be read: {e}") })
}

/// Starts a new game as `country` (or in observe mode when `None`).
pub fn start(_: Caller, country: Option<[u8; 3]>) -> GResult<()> {
    let m = game()?;
    let letters = country.unwrap_or(save_file::OBSERVER);
    if sessions::at() == At::Game {
        let known = || if country.is_some() { transition::find_country(m, letters).map(drop) } else { Ok(()) };
        transition::new_game_from_session(m, letters, known)?;
        return Ok(());
    }
    transition::first_page(m)?;
    if country.is_none() {
        if m.u8(transition::world_now(m)? + world::IRONMAN)? != 0 {
            return fail(Status::Rejected, "an ironman game cannot be observed; turn ironman off in the single player setup");
        }
        log!("menu: a new game in observe mode");
        transition::ask_start(letters);
        return Ok(());
    }
    let tag = transition::find_country(m, letters)?;
    if super::geography::owned_provinces(m, transition::world_now(m)?, tag)? == 0 {
        let name = String::from_utf8_lossy(&letters).into_owned();
        return fail(
            Status::Rejected,
            format!("{name} has no land at the start date, so it cannot be played from it; started as {name}, the game would only be observed"),
        );
    }
    transition::ask_start(letters);
    Ok(())
}

/// The last load or start asked for, to ask [`launch`] about.
pub fn last(_: Caller) -> u64 {
    transition::last()
}

/// What came of the load or start `number`.
pub fn launch(_: Caller, number: u64) -> Launch {
    transition::launch(number)
}

/// What came of the return asked for as `number`.
pub fn returned(_: Caller, number: u64) -> Returned {
    transition::returned(number)
}

/// Returns to the main menu from an active session.
pub fn return_to_main_menu(_: Caller) -> GResult<Option<u64>> {
    transition::return_to_main_menu(game()?)
}

/// Pauses an active session before a transition, returning whether it was running.
pub(crate) fn pause_for_transition(m: &Memory) -> GResult<bool> {
    let world = super::world_context_any(m)?;
    let ran = !super::paused(m, &world)?;
    if ran {
        super::time::pause_game(m, &world)?;
    }
    Ok(ran)
}

/// Whether the active session is paused with no daily update running.
pub(crate) fn is_still_for_transition(m: &Memory) -> bool {
    let Ok(world) = super::world_context_any(m) else { return false };
    let busy = !m.u8(world.world + world::DAILY_UPDATE).is_ok_and(|b| b == 0);
    super::paused(m, &world).unwrap_or(false) && !busy
}

pub fn points() -> Vec<Point> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::{missing_mods, save_name};
    use crate::session::saves::file as save_file;

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
