//! Whether a file is in a folder of the game's, answered from the folder's
//! listing (docs/game/program/loading.md).
//!
//! Every lookup of a file asks each folder of PhysFS's search path in turn,
//! and for a folder PhysFS asks Windows (`0x1a061d0`, its platform's stat:
//! `GetFileAttributesExW`, then the file's times through the time zone): the
//! game folder, the workshop's mods and the user folder, for each picture,
//! texture and script the game looks for, most of them in none of those
//! folders (docs/game/program/loading.md).
//!
//! Inside the game folder and the workshop's folder for the game, which
//! nothing changes while the game runs, the framework lists each folder once,
//! the first time a path in it is asked for via [`DirectoryCache`]. A path
//! whose folders or file the listings do not have is answered as Windows
//! answers a missing file: PhysFS's error "not found", and no stat. A path
//! they have, and any path the framework cannot answer for certain (another
//! folder, the framework's own folder, a name Windows would read otherwise),
//! goes to the game's stat as before.
use super::folder::{lowered_relative, normalize_dir, DirectoryCache, Kind};
use crate::base::called_by_game;
use crate::native::physfs;
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::ffi::{c_char, CStr};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// `(native path in UTF-8, &stat, follow links) -> found`.
const STAT: usize = 0x1a061d0;
/// Where it fails: `GetLastError`, the path let go, the error made PhysFS's
/// (`0x1a06990`: `ERROR_FILE_NOT_FOUND` and `ERROR_PATH_NOT_FOUND` give
/// `PHYSFS_ERR_NOT_FOUND`) and set (`0x19f8510`).
const STAT_FAILS: usize = 0x1a062a3;
const NOT_FOUND_MADE: usize = 0x1a06a00;

/// Checked, the game's stat answers every path the listings answer missing
/// as well, and the game's answer is kept.
pub const SPEED_UP: SpeedUp = SpeedUp::new("lookups", "loading", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(STAT, "40 55 57 41 54 41 56 41 57 48 81 ec b0 02 00 00", "PhysFS stat", stat as Stat, &ORIGINAL_STAT),
        Point::keep(
            STAT_FAILS,
            "ff 15 ef 03 16 00 48 8b ce 8b d8 e8 fd 34 ff ff 8b cb e8 d6 06 00 00 85 c0 0f 84 02 01 00 00 8b c8 e9 f6 00",
            "PhysFS stat fails",
        ),
        Point::keep(NOT_FOUND_MADE, "b8 0b 00 00 00 c3", "PhysFS's not found from Windows'").uses(&[physfs::NOT_FOUND]),
    ]
}

type Stat = extern "C" fn(*const c_char, usize, i32) -> i32;
static ORIGINAL_STAT: Function<Stat> = Function::new();

/// The immutable roots answered from listings, plus the framework's own folder
/// (which it writes to and excludes from negative caching).
struct Lookups {
    roots: Vec<DirectoryCache>,
    own: String,
}

impl Lookups {
    fn new(game_dir: &Path) -> Self {
        let mut roots = vec![DirectoryCache::new(&game_dir.to_string_lossy())];
        if let Some(library) = game_dir.parent().and_then(Path::parent) {
            roots.push(DirectoryCache::new(&library.join("workshop").join("content").join("236850").to_string_lossy()));
        }
        Self { roots, own: normalize_dir(&game_dir.join("eu4-harmony")) }
    }

    /// Whether the path is certainly missing (`Some(true)`), certainly there
    /// (`Some(false)`), or not for the listings to say (`None`).
    fn missing(&self, path: &[u8]) -> Option<bool> {
        let text = std::str::from_utf8(path).ok()?;
        let lower = text.replace('/', "\\").to_ascii_lowercase();
        if lower.starts_with(self.own.as_str()) {
            return None;
        }
        let root = self.roots.iter().find(|r| lower.starts_with(r.lower()))?;
        let rest = &lower[root.lower().len()..];
        let mut buffer = [0u8; 512];
        let rel = lowered_relative(rest.as_bytes(), true, &mut buffer)?;
        match root.kind(rel) {
            Ok(None) => Some(true),
            Ok(Some(Kind::Other)) | Err(()) => None,
            Ok(Some(Kind::File | Kind::Folder)) => Some(false),
        }
    }
}

static ROOTS: OnceLock<Lookups> = OnceLock::new();
static ANSWERED: AtomicU64 = AtomicU64::new(0);

/// The game folder and, beside its library's `common`, the workshop's folder
/// for the game.
pub fn set_roots(game_dir: &Path) {
    let _ = ROOTS.set(Lookups::new(game_dir));
}

#[called_by_game]
extern "C" fn stat(path: *const c_char, st: usize, follow: i32) -> i32 {
    let missing = !path.is_null() && crate::base::guarded("PhysFS stat", || missing(unsafe { CStr::from_ptr(path) }.to_bytes()) == Some(true));
    if !missing {
        return ORIGINAL_STAT.get()(path, st, follow);
    }
    if speed::checked(&SPEED_UP) {
        let game = ORIGINAL_STAT.get()(path, st, follow);
        speed::compared(&SPEED_UP, game == 0, || format!("{:?} found by the game, missing from the listings", unsafe { CStr::from_ptr(path) }));
        return game;
    }
    let m = crate::native::hooked_memory();
    physfs::set_not_found(&m);
    let n = ANSWERED.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_power_of_two() && n >= 1024 {
        log!("lookups: {n} paths answered missing from the folders' listings");
    }
    0
}

fn missing(path: &[u8]) -> Option<bool> {
    ROOTS.get()?.missing(path)
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
        std::fs::write(dir.join("1.310\u{66f4}\u{65b0}\u{5185}\u{5bb9}.txt"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("eu4-harmony")).unwrap();
        let lookups = Lookups::new(&dir);
        let root = dir.to_string_lossy().into_owned();
        let ask = |rest: &str| lookups.missing(format!("{root}\\{rest}").as_bytes());
        assert_eq!(ask("gfx\\interface\\icon.dds"), Some(false));
        assert_eq!(ask("GFX/Interface/ICON.DDS"), Some(false));
        assert_eq!(ask("gfx\\interface\\other.dds"), Some(true));
        assert_eq!(ask("gfx\\missing\\icon.dds"), Some(true));
        // A file where a folder would be: Windows answers "path not found".
        assert_eq!(ask("gfx\\interface\\icon.dds\\more"), Some(true));
        assert_eq!(ask("gfx\\interface"), Some(false));
        // Not the listings' to say.
        assert_eq!(ask("1.310\u{66f4}\u{65b0}\u{5185}\u{5bb9}.txt"), None);
        assert_eq!(ask("eu4-harmony\\logs\\core.log"), None);
        assert_eq!(ask("gfx\\..\\gfx\\interface\\icon.dds"), None);
        assert_eq!(ask("gfx\\interface\\ICON~1.DDS"), None);
        assert_eq!(lookups.missing(b"C:\\elsewhere\\file.txt"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
