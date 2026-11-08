//! Whether a file is in a folder of the game's, answered from the folder's
//! listing (docs/loading.md).
//!
//! Every lookup of a file asks each folder of PhysFS's search path in turn,
//! and for a folder PhysFS asks Windows (`0x1a061d0`, its platform's stat:
//! `GetFileAttributesExW`, then the file's times through the time zone): the
//! game folder, the workshop's mods and the user folder, for each picture,
//! texture and script the game looks for, most of them in none of those
//! folders (docs/research/loading.md).
//!
//! Inside the game folder and the workshop's folder for the game, which
//! nothing changes while the game runs, the framework lists each folder once,
//! the first time a path in it is asked for. A path whose folders or file the
//! listings do not have is answered as Windows answers a missing file:
//! PhysFS's error "not found", and no stat. A path they have, and any path
//! the framework cannot answer for certain (another folder, the framework's
//! own folder, a name Windows would read otherwise), goes to the game's stat
//! as before.
use crate::called_by_game;
use crate::points::{Function, GameFunction, Point};
use crate::speed::{self, SpeedUp};
use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// `(native path in UTF-8, &stat, follow links) -> found`.
const STAT: usize = 0x1a061d0;
/// Where it fails: `GetLastError`, the path let go, the error made PhysFS's
/// (`0x1a06990`: `ERROR_FILE_NOT_FOUND` and `ERROR_PATH_NOT_FOUND` give
/// `PHYSFS_ERR_NOT_FOUND`) and set (`0x19f8510`).
const STAT_FAILS: usize = 0x1a062a3;
const NOT_FOUND_MADE: usize = 0x1a06a00;
pub(super) const SET_ERROR: GameFunction<1> = GameFunction::at(0x19f8510);
pub(super) const NOT_FOUND: usize = 0xb;

/// Checked, the game's stat answers every path the listings answer missing
/// as well, and the game's answer is kept.
pub const SPEED_UP: SpeedUp = SpeedUp { name: "lookups", group: "loading", points, checkable: true };

fn points() -> Vec<Point> {
    vec![
        Point::replace(STAT, "40 55 57 41 54 41 56 41 57 48 81 ec b0 02 00 00", "PhysFS stat", stat as Stat, &ORIGINAL_STAT),
        Point::keep(
            STAT_FAILS,
            "ff 15 ef 03 16 00 48 8b ce 8b d8 e8 fd 34 ff ff 8b cb e8 d6 06 00 00 85 c0 0f 84 02 01 00 00 8b c8 e9 f6 00",
            "PhysFS stat fails",
        ),
        Point::keep(NOT_FOUND_MADE, "b8 0b 00 00 00 c3", "PhysFS's not found from Windows'").uses(&[NOT_FOUND]),
        Point::keep(SET_ERROR, "85 c9 74 7f 57 48 83 ec 20 8b f9 48 89 5c 24 30", "PhysFS sets its error"),
    ]
}

type Stat = extern "C" fn(*const c_char, usize, i32) -> i32;
static ORIGINAL_STAT: Function<Stat> = Function::new();

/// The folders answered from listings, lower case, each ending in `\`; and
/// the framework's own folder, which it writes to, left out.
static ROOTS: OnceLock<(Vec<String>, String)> = OnceLock::new();
/// Each folder's listing, by its path in lower case: its names in lower
/// case, each a file, a folder, or something else (a link); None where it
/// could not be listed.
type Listing = Option<Arc<HashMap<String, Kind>>>;
static LISTINGS: RwLock<Option<HashMap<String, Listing>>> = RwLock::new(None);
static ANSWERED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    File,
    Folder,
    Other,
}

/// The game folder and, beside its library's `common`, the workshop's folder
/// for the game.
pub fn set_roots(game_dir: &Path) {
    let folder = |p: &Path| format!("{}\\", p.to_string_lossy().trim_end_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase());
    let mut roots = vec![folder(game_dir)];
    if let Some(library) = game_dir.parent().and_then(Path::parent) {
        roots.push(folder(&library.join("workshop").join("content").join("236850")));
    }
    let own = folder(&game_dir.join("eu4-harmony"));
    let _ = ROOTS.set((roots, own));
}

#[called_by_game]
extern "C" fn stat(path: *const c_char, st: usize, follow: i32) -> i32 {
    let missing = !path.is_null() && crate::guarded("PhysFS stat", || missing(unsafe { CStr::from_ptr(path) }.to_bytes()) == Some(true));
    if !missing {
        return ORIGINAL_STAT.get()(path, st, follow);
    }
    if speed::checked(&SPEED_UP) {
        let game = ORIGINAL_STAT.get()(path, st, follow);
        speed::compared(&SPEED_UP, game == 0, || format!("{:?} found by the game, missing from the listings", unsafe { CStr::from_ptr(path) }));
        return game;
    }
    let m = crate::hooked_memory();
    let _ = SET_ERROR.call(&m, [NOT_FOUND]);
    let n = ANSWERED.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_power_of_two() && n >= 1024 {
        log!("lookups: {n} paths answered missing from the folders' listings");
    }
    0
}

/// Whether the path is certainly missing (`Some(true)`), certainly there
/// (`Some(false)`), or not for the listings to say (None).
fn missing(path: &[u8]) -> Option<bool> {
    let (roots, own) = ROOTS.get()?;
    let text = std::str::from_utf8(path).ok()?;
    let lower = text.replace('/', "\\").to_ascii_lowercase();
    if lower.starts_with(own.as_str()) {
        return None;
    }
    let root = roots.iter().find(|r| lower.starts_with(r.as_str()))?;
    let rest = &lower[root.len()..];
    // Names Windows reads otherwise than as they are written, below the
    // folder answered for (whose own path may be a short one, `RUNNER~1`).
    if !rest.is_ascii() || rest.contains(['<', '>', '"', '|', '?', '*', '~']) || rest.chars().any(|c| c.is_ascii_control()) {
        return None;
    }
    let parts: Vec<&str> = rest.split('\\').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.ends_with(['.', ' ']) || p.contains(':')) {
        return None;
    }
    let mut folder = root.trim_end_matches('\\').to_string();
    for (i, part) in parts.iter().enumerate() {
        let listing = listing(&folder)?;
        let last = i + 1 == parts.len();
        match listing.get(*part) {
            None => return Some(true),
            Some(Kind::Other) => return None,
            Some(Kind::File) if !last => return Some(true),
            Some(_) if last => return Some(false),
            Some(_) => folder = format!("{folder}\\{part}"),
        }
    }
    None
}

fn listing(folder: &str) -> Listing {
    if let Some(found) = LISTINGS.read().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|l| l.get(folder)) {
        return found.clone();
    }
    let made = list(folder).map(Arc::new);
    LISTINGS.write().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(folder.to_string(), made.clone());
    made
}

fn list(folder: &str) -> Option<HashMap<String, Kind>> {
    let mut names = HashMap::new();
    for entry in std::fs::read_dir(folder).ok()? {
        let entry = entry.ok()?;
        let kind = match entry.file_type().ok()? {
            t if t.is_symlink() => Kind::Other,
            t if t.is_dir() => Kind::Folder,
            t if t.is_file() => Kind::File,
            _ => Kind::Other,
        };
        let name = entry.file_name().into_string().ok()?;
        if !name.is_ascii() {
            // A name the folder's listing cannot be matched against for
            // certain: the whole folder goes to the game's stat.
            return None;
        }
        names.insert(name.to_ascii_lowercase(), kind);
    }
    Some(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_answered_from_the_listings_of_its_folders() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-lookups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("gfx").join("Interface")).unwrap();
        std::fs::write(dir.join("gfx").join("Interface").join("Icon.dds"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("eu4-harmony")).unwrap();
        set_roots(&dir);
        let root = dir.to_string_lossy().into_owned();
        let ask = |rest: &str| missing(format!("{root}\\{rest}").as_bytes());
        assert_eq!(ask("gfx\\interface\\icon.dds"), Some(false));
        assert_eq!(ask("GFX/Interface/ICON.DDS"), Some(false));
        assert_eq!(ask("gfx\\interface\\other.dds"), Some(true));
        assert_eq!(ask("gfx\\missing\\icon.dds"), Some(true));
        // A file where a folder would be: Windows answers "path not found".
        assert_eq!(ask("gfx\\interface\\icon.dds\\more"), Some(true));
        assert_eq!(ask("gfx\\interface"), Some(false));
        // Not the listings' to say.
        assert_eq!(ask("eu4-harmony\\logs\\core.log"), None);
        assert_eq!(ask("gfx\\..\\gfx\\interface\\icon.dds"), None);
        assert_eq!(ask("gfx\\interface\\ICON~1.DDS"), None);
        assert_eq!(missing(b"C:\\elsewhere\\file.txt"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
