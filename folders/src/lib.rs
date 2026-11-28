//! The game's folders as every program of EU4 Harmony finds them: the game
//! folder, the user folder (GLOSSARY), and the names of what the user folder
//! holds that the framework writes and its tools read. The core, the command
//! line tool, the client and `cargo xtask` share it, so that the game folder
//! is found in one order and a file of the user folder is named in one place.
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

mod item_id;
pub mod windows_crash_callback;

pub use item_id::parse_published_file_id;

/// The game's Steam app id, under which the Workshop keeps its items.
pub const APP_ID: &str = "236850";

/// Prefix for a replaced `version.dll` moved aside while loaded, waiting for a
/// later startup or install to remove it once no process maps it.
pub const OLD_PROXY_PREFIX: &str = "version.dll.old-";

/// Prefix for a staged `version.dll` written beside the live proxy before
/// switching it into place.
pub const NEW_PROXY_PREFIX: &str = "version.dll.new-";

/// Whether `bytes` belongs to Harmony's own `version.dll` proxy: it names the
/// core's bootstrap export, which a foreign DLL does not.
pub fn is_harmony_proxy(bytes: &[u8]) -> bool {
    const MARK: &[u8] = b"eu4_harmony_bootstrap";
    bytes.windows(MARK.len()).any(|w| w == MARK)
}

/// Harmony's published file id on the Steam Workshop, from `workshop/item.toml`;
/// `0` before the item is created.
pub const PUBLISHED_FILE_ID: u64 = include!(concat!(env!("OUT_DIR"), "/published_file_id.rs"));

pub type Result<T> = std::result::Result<T, String>;

/// File names written by the out-of-process Windows crash callback.
pub fn captured_dump_id(name: &str) -> Option<(u32, u64)> {
    let (pid, created) = name.strip_prefix("harmony-")?.strip_suffix(".dmp")?.split_once('-')?;
    if created.len() != 16 || !created.bytes().all(|b| b.is_ascii_hexdigit()) || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((pid.parse().ok()?, u64::from_str_radix(created, 16).ok()?))
}

/// Windows Error Reporting's default folder for the current Windows account.
pub fn windows_dump_root() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").filter(|path| !path.is_empty()).map(PathBuf::from).filter(|path| path.is_absolute()).map(|path| path.join("CrashDumps"))
}

/// Default total-size and age limits for Harmony-owned Windows dump folders.
pub const DUMP_MAX_BYTES: u64 = 256 * 1024 * 1024;
pub const DUMP_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// File names written by Windows Error Reporting LocalDumps for `eu4.exe`.
pub fn is_wer_dump(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.strip_prefix("eu4.exe.").and_then(|rest| rest.strip_suffix(".dmp")).is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
}

/// Prunes Harmony-owned dump files in `dir` matching `is_ours`: removes oldest
/// files first until count (when `max_files` is set), total bytes (`max_bytes`)
/// and age (`max_age`) are within limits, while always keeping at least the
/// newest matching dump. Never follows symlinks or touches unrelated files.
pub fn prune_dumps(dir: &Path, max_files: Option<usize>, max_bytes: u64, max_age: Duration, is_ours: impl Fn(&str) -> bool) -> usize {
    prune_dumps_at(dir, SystemTime::now(), max_files, max_bytes, max_age, is_ours)
}

fn prune_dumps_at(dir: &Path, now: SystemTime, max_files: Option<usize>, max_bytes: u64, max_age: Duration, is_ours: impl Fn(&str) -> bool) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut dumps: Vec<(SystemTime, String, u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_ours(&name) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = meta.file_type().is_symlink();
        if !meta.is_file() || reparse {
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        dumps.push((modified, name, meta.len(), path));
    }
    if dumps.len() <= 1 {
        return 0;
    }
    // Sort oldest first; tie-break by name (timestamp-bearing names sort chronologically).
    dumps.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut remaining_files = dumps.len();
    let mut remaining_bytes: u64 = dumps.iter().map(|d| d.2).fold(0u64, u64::saturating_add);
    let mut removed = 0;
    // Never remove the newest matching file (last element).
    for (modified, _, size, path) in &dumps[..dumps.len() - 1] {
        let over_count = max_files.is_some_and(|limit| remaining_files > limit);
        let over_bytes = remaining_bytes > max_bytes;
        let expired = now.duration_since(*modified).unwrap_or_default() > max_age;
        if (over_count || over_bytes || expired) && std::fs::remove_file(path).is_ok() {
            remaining_files -= 1;
            remaining_bytes = remaining_bytes.saturating_sub(*size);
            removed += 1;
        }
    }
    removed
}

/// Prunes both Harmony-owned Windows dump folders (`eu4-harmony` and
/// `eu4-harmony-wer`) under `%LOCALAPPDATA%\CrashDumps`.
pub fn prune_harmony_dumps() -> usize {
    let Some(root) = windows_dump_root() else { return 0 };
    let mut removed = 0;
    removed += prune_dumps(&root.join("eu4-harmony"), Some(5), DUMP_MAX_BYTES, DUMP_MAX_AGE, |name| captured_dump_id(name).is_some());
    removed += prune_dumps(&root.join("eu4-harmony-wer"), None, DUMP_MAX_BYTES, DUMP_MAX_AGE, is_wer_dump);
    removed
}

/// The game folder: `given`, a command's `--game-dir`; else `EU4_GAME_DIR`,
/// unless it is empty, as a variable set to nothing is meant to be unset;
/// else the game folder this program is installed in
/// (`<game>\eu4-harmony\eu4-harmony.exe`); else the Steam library that holds
/// the game.
pub fn game_dir(given: Option<&Path>) -> Result<PathBuf> {
    let named = given.map(Path::to_path_buf).or_else(|| std::env::var_os("EU4_GAME_DIR").filter(|d| !d.is_empty()).map(PathBuf::from));
    named
        .or_else(|| std::env::current_exe().ok().and_then(|exe| installed_in(&exe)))
        .or_else(steam_game_dir)
        .ok_or_else(|| "cannot find the game: pass --game-dir, or set EU4_GAME_DIR".into())
}

/// The game folder holding the program at `exe`, when it is installed in
/// `<game>\eu4-harmony.exe` or `<game>\eu4-harmony\eu4-harmony.exe`.
fn installed_in(exe: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?;
    if parent.join("eu4.exe").is_file() {
        return Some(parent.to_path_buf());
    }
    let game = parent.parent()?;
    game.join("eu4.exe").is_file().then(|| game.to_path_buf())
}

/// The game's user folder, where it keeps saves, settings and the `.mod`
/// descriptors in `mod\`, as the game itself decides it: the folder
/// `userdir.txt` in the game folder names, and otherwise
/// `Documents\Paradox Interactive\Europa Universalis IV`.
pub fn user_dir(game_dir: Option<&Path>) -> Result<PathBuf> {
    match game_dir.and_then(named_user_dir) {
        Some(dir) => Ok(dir),
        None => Ok(documents()?.join("Paradox Interactive").join("Europa Universalis IV")),
    }
}

/// The user directory of the game started with `command_line`: the folder
/// its `-userdir=` names, as the game takes it before `userdir.txt`, relative
/// to the game folder when it is not absolute; else [`user_dir`].
pub fn user_dir_of(command_line: &str, game_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(named) = switch_value(command_line, "-userdir").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(named);
        return Ok(match game_dir {
            Some(game) if !path.is_absolute() => game.join(path),
            _ => path,
        });
    }
    user_dir(game_dir)
}

/// The folder Steam keeps the Workshop item `published_file_id` in, for the
/// game in `game_dir`: `<library>\steamapps\workshop\content\236850\<id>`
/// when the game is `<library>\steamapps\common\<game>`; none while the id is
/// `0` (item not yet created).
pub fn workshop_item(game_dir: &Path, published_file_id: u64) -> Option<PathBuf> {
    if published_file_id == 0 {
        return None;
    }
    let common = game_dir.parent()?;
    let steamapps = common.parent()?;
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(name));
    (named(common, "common") && named(steamapps, "steamapps"))
        .then(|| steamapps.join("workshop").join("content").join(APP_ID).join(published_file_id.to_string()))
}

/// Where the framework looks for `eu4_harmony_core.dll` when no explicit
/// `-harmony_core` is given: `<game>\eu4-harmony\` first, then Harmony's own
/// Workshop item in the same Steam library.
pub fn core_places(game_dir: &Path, published_file_id: u64) -> Vec<PathBuf> {
    let mut places = vec![game_dir.join(HARMONY).join("eu4_harmony_core.dll")];
    if let Some(item) = workshop_item(game_dir, published_file_id) {
        places.push(item.join(HARMONY).join("eu4_harmony_core.dll"));
    }
    places
}

/// The first existing core DLL in [`core_places`] for [`PUBLISHED_FILE_ID`].
pub fn active_core(game_dir: &Path) -> Option<PathBuf> {
    core_places(game_dir, PUBLISHED_FILE_ID).into_iter().find(|p| p.is_file())
}

/// The words of a Windows command line, split at spaces and tabs outside
/// double quotes, with the quotes left out. Backslashes are taken as they
/// are, which is all a path needs.
pub fn command_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in line.chars() {
        match c {
            '"' => (quoted, any) = (!quoted, true),
            ' ' | '\t' if !quoted => {
                if any {
                    words.push(std::mem::take(&mut word));
                }
                any = false;
            }
            _ => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(word);
    }
    words
}

/// The value of the switch `name=value` among a command line's words, the
/// name in any case and with one dash or two, the last one when it is there
/// more than once. The game restarting, as a return to the main menu with
/// the in-process return off does, writes every switch again as
/// `--name="value"`.
pub fn switch_value(line: &str, name: &str) -> Option<String> {
    command_words(line).into_iter().rev().find_map(|word| {
        let (switch, value) = word.split_once('=')?;
        same_switch(switch, name).then(|| value.to_string())
    })
}

/// Whether the word `switch` is the switch `name`: in any case, with one
/// dash or two.
pub fn same_switch(switch: &str, name: &str) -> bool {
    let bare = |s: &str| s.strip_prefix("--").or_else(|| s.strip_prefix('-')).map(str::to_ascii_lowercase);
    bare(switch).is_some_and(|s| Some(s) == bare(name))
}

/// The folder `userdir.txt` in the game folder names, absolute or relative to
/// the game folder; none when there is no such file or it is empty.
fn named_user_dir(game_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(game_dir.join("userdir.txt")).ok()?;
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return None;
    }
    let path = PathBuf::from(text);
    Some(if path.is_absolute() { path } else { game_dir.join(path) })
}

/// The game's saves, in the user folder.
pub const SAVE_GAMES: &str = "save games";
/// The framework's own folder in the user folder.
pub const HARMONY: &str = "eu4-harmony";

/// A user folder, and the names of what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserFolder(PathBuf);

impl UserFolder {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// The user folder of the game in `game_dir`.
    pub fn of(game_dir: &Path) -> Result<Self> {
        user_dir(Some(game_dir)).map(Self)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The game's saves.
    pub fn save_games(&self) -> PathBuf {
        self.0.join(SAVE_GAMES)
    }

    /// The framework's own folder.
    pub fn harmony(&self) -> PathBuf {
        self.0.join(HARMONY)
    }

    /// The framework's logs: the log, the startup report and what the
    /// development tools write.
    pub fn logs(&self) -> PathBuf {
        self.harmony().join("logs")
    }

    /// The framework's log, every line in order.
    pub fn core_log(&self) -> PathBuf {
        self.logs().join("core.log")
    }

    /// The startup report.
    pub fn report(&self) -> PathBuf {
        self.logs().join("report.txt")
    }

    /// The game's own logs.
    pub fn game_logs(&self) -> PathBuf {
        self.0.join("logs")
    }

    /// The game's crash folders, one for each crash its crash handler caught.
    pub fn crashes(&self) -> PathBuf {
        self.0.join("crashes")
    }

    /// The names of the crash folders there are now, in order; none when
    /// there is no `crashes\`.
    pub fn crash_folders(&self) -> Result<Vec<String>> {
        let dir = self.crashes();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    /// The crash folders that came since `known` were all there were, the
    /// last written last; none when they cannot be read.
    pub fn new_crash_folders(&self, known: &[String]) -> Vec<PathBuf> {
        let mut new: Vec<(SystemTime, PathBuf)> = self
            .crash_folders()
            .unwrap_or_default()
            .into_iter()
            .filter(|name| !known.contains(name))
            .map(|name| self.crashes().join(name))
            .map(|path| (std::fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH), path))
            .collect();
        new.sort();
        new.into_iter().map(|(_, path)| path).collect()
    }
}

/// The game folder Steam installed the game in: under Steam's own folder or
/// one of the libraries its `steamapps\libraryfolders.vdf` lists, the first
/// that holds `eu4.exe`.
fn steam_game_dir() -> Option<PathBuf> {
    let steam = steam_path()?;
    let mut libraries = vec![steam.clone()];
    if let Ok(text) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        libraries.extend(library_paths(&text));
    }
    libraries.into_iter().map(|l| l.join("steamapps").join("common").join("Europa Universalis IV")).find(|game| game.join("eu4.exe").is_file())
}

/// The `"path"` values of a `libraryfolders.vdf`, with its doubled
/// backslashes made single.
fn library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let value = line.trim().strip_prefix("\"path\"")?.trim().strip_prefix('"')?.strip_suffix('"')?;
            Some(PathBuf::from(value.replace("\\\\", "\\")))
        })
        .collect()
}

/// Steam's folder, `SteamPath` under `HKCU\Software\Valve\Steam`.
fn steam_path() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let key: Vec<u16> = "Software\\Valve\\Steam\0".encode_utf16().collect();
    let name: Vec<u16> = "SteamPath\0".encode_utf16().collect();
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buffer.as_mut_ptr().cast(), &mut size) };
    if status != 0 {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
}

/// The Windows user's Documents folder, read from `HKCU\...\User Shell Folders`
/// (with `USERPROFILE\Documents` fallback) so calling from `DllMain` under
/// the loader lock never loads `shell32.dll` or `ole32.dll`.
fn documents() -> Result<PathBuf> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders\0".encode_utf16().collect();
    let value: Vec<u16> = "Personal\0".encode_utf16().collect();
    let mut buf = vec![0u16; 32768];
    let mut size = (buf.len() * 2) as u32;
    // A REG_EXPAND_SZ value comes back expanded, as a REG_SZ.
    let status =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size) };
    if status == 0 {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = String::from_utf16_lossy(&buf[..len]);
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    std::env::var_os("USERPROFILE")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join("Documents"))
        .ok_or_else(|| format!("cannot find the Documents folder: registry status {status}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu4h-folders-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn userdir_txt_names_the_user_folder() {
        let game = scratch("userdir");
        assert_eq!(named_user_dir(&game), None);
        for (text, expected) in [("\u{feff}  D:\\EU4 user\r\n", Some(PathBuf::from(r"D:\EU4 user"))), ("user", Some(game.join("user"))), (" \r\n", None)] {
            fs::write(game.join("userdir.txt"), text).unwrap();
            assert_eq!(named_user_dir(&game), expected, "{text:?}");
            if let Some(dir) = expected {
                assert_eq!(user_dir(Some(&game)).unwrap(), dir);
            }
        }
        let _ = fs::remove_dir_all(game);
    }

    #[test]
    fn library_paths_reads_every_path_with_single_backslashes() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"E:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(library_paths(vdf), [PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"E:\SteamLibrary")]);
    }

    #[test]
    fn a_program_is_installed_in_the_game_folder_that_holds_it_and_the_game() {
        let game = scratch("installed");
        fs::create_dir_all(game.join("eu4-harmony")).unwrap();
        let nested = game.join("eu4-harmony").join("eu4-harmony.exe");
        let top = game.join("eu4-harmony.exe");
        assert_eq!(installed_in(&nested), None, "no eu4.exe beside it");
        assert_eq!(installed_in(&top), None, "no eu4.exe beside it");
        fs::write(game.join("eu4.exe"), "").unwrap();
        assert_eq!(installed_in(&nested), Some(game.clone()));
        assert_eq!(installed_in(&top), Some(game.clone()));
        let _ = fs::remove_dir_all(game);
    }

    #[test]
    fn workshop_item_core_places_and_active_core_follow_steam_library_layout() {
        let root = scratch("workshop-core");
        let game = root.join("SteamLibrary").join("steamapps").join("common").join("Europa Universalis IV");
        fs::create_dir_all(&game).unwrap();
        assert_eq!(
            core_places(&game, 3_123_456_789),
            [
                game.join("eu4-harmony").join("eu4_harmony_core.dll"),
                root.join("SteamLibrary")
                    .join("steamapps")
                    .join("workshop")
                    .join("content")
                    .join("236850")
                    .join("3123456789")
                    .join("eu4-harmony")
                    .join("eu4_harmony_core.dll"),
            ]
        );
        assert_eq!(workshop_item(&game, 0), None);
        assert_eq!(core_places(&game, 0).len(), 1);
        assert_eq!(workshop_item(Path::new(r"D:\Games\eu4"), 7), None);

        let workshop_core = workshop_item(&game, PUBLISHED_FILE_ID).unwrap().join("eu4-harmony").join("eu4_harmony_core.dll");
        if PUBLISHED_FILE_ID != 0 {
            assert_eq!(active_core(&game), None);
            fs::create_dir_all(workshop_core.parent().unwrap()).unwrap();
            fs::write(&workshop_core, b"").unwrap();
            assert_eq!(active_core(&game).as_deref(), Some(workshop_core.as_path()));
            let local_core = game.join("eu4-harmony").join("eu4_harmony_core.dll");
            fs::create_dir_all(local_core.parent().unwrap()).unwrap();
            fs::write(&local_core, b"").unwrap();
            assert_eq!(active_core(&game).as_deref(), Some(local_core.as_path()));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn published_file_id_parser_skips_comments_and_rejects_non_numbers() {
        assert_eq!(parse_published_file_id("published_file_id = 0\n"), Some(0));
        assert_eq!(parse_published_file_id("published_file_id = 42 # inline\n"), Some(42));
        assert_eq!(parse_published_file_id("# a note\npublished_file_id=3141592653\n"), Some(3141592653));
        assert_eq!(parse_published_file_id("# published_file_id = 9\n"), None);
        assert_eq!(parse_published_file_id("published_file_id = soon\n"), None);
    }

    #[test]
    fn a_game_folder_given_comes_first() {
        let given = Path::new(r"X:\a game");
        assert_eq!(game_dir(Some(given)).unwrap(), given);
    }

    #[test]
    fn the_new_crash_folders_are_those_not_known_the_last_written_last() {
        use std::{
            fs::{FileTimes, OpenOptions},
            os::windows::fs::OpenOptionsExt,
            time::Duration,
        };
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

        let user = UserFolder::new(scratch("crashes"));
        assert_eq!(user.crash_folders().unwrap(), Vec::<String>::new(), "no crashes folder");
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for (index, name) in ["old", "b", "a"].into_iter().enumerate() {
            let dir = user.crashes().join(name);
            fs::create_dir_all(&dir).unwrap();
            let handle = OpenOptions::new().access_mode(FILE_WRITE_ATTRIBUTES).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(&dir).unwrap();
            handle.set_times(FileTimes::new().set_modified(base + Duration::from_secs((index as u64 + 1) * 10))).unwrap();
        }
        fs::write(user.crashes().join("not a folder.txt"), "").unwrap();
        assert_eq!(user.crash_folders().unwrap(), ["a", "b", "old"]);
        assert_eq!(user.new_crash_folders(&["old".into()]), [user.crashes().join("b"), user.crashes().join("a")]);
        let _ = fs::remove_dir_all(user.path());
    }

    #[test]
    fn a_command_line_s_words_and_switches() {
        let line = r#""E:\Steam\Europa Universalis IV\eu4.exe" -harmony_hidden "-harmony_core=C:\a b\core.dll"  -USERDIR=D:\u -seed=3"#;
        assert_eq!(
            command_words(line),
            [r"E:\Steam\Europa Universalis IV\eu4.exe", "-harmony_hidden", r"-harmony_core=C:\a b\core.dll", r"-USERDIR=D:\u", "-seed=3"]
        );
        assert_eq!(switch_value(line, "-harmony_core").as_deref(), Some(r"C:\a b\core.dll"));
        assert_eq!(switch_value(line, "-userdir").as_deref(), Some(r"D:\u"));
        assert_eq!(switch_value(line, "-harmony_hidden"), None);
        assert_eq!(switch_value("eu4.exe -seed=1 -seed=2", "-seed").as_deref(), Some("2"));
        assert_eq!(command_words(r#"a "" b"#), ["a", "", "b"]);
    }

    /// The command line the game makes as it restarts itself.
    #[test]
    fn a_switch_is_read_as_the_restarted_game_writes_it_again() {
        let line = r#"--E:\Steam\eu4.exe --harmony_core="E:\claims\a\core\eu4_harmony_core.dll" --harmony_hidden --harmony_test_game="a" --userdir="E:\claims\a\user" -skiplauncher"#;
        assert_eq!(switch_value(line, "-harmony_core").as_deref(), Some(r"E:\claims\a\core\eu4_harmony_core.dll"));
        assert_eq!(switch_value(line, "-harmony_test_game").as_deref(), Some("a"));
        assert_eq!(switch_value(line, "-userdir").as_deref(), Some(r"E:\claims\a\user"));
        assert!(same_switch("--harmony_hidden", "-harmony_hidden") && same_switch("-HARMONY_HIDDEN", "-harmony_hidden"));
        assert!(!same_switch("harmony_hidden", "-harmony_hidden") && !same_switch("---harmony_hidden", "-harmony_hidden"));
    }

    #[test]
    fn the_user_folder_is_the_one_userdir_names_first() {
        let game = scratch("userdir-first");
        fs::write(game.join("userdir.txt"), "D:\\named\n").unwrap();
        assert_eq!(user_dir_of(r"eu4.exe -userdir=D:\u", Some(&game)).unwrap(), Path::new(r"D:\u"));
        assert_eq!(user_dir_of(r#"eu4.exe "--userdir=rel dir""#, Some(&game)).unwrap(), game.join("rel dir"));
        assert_eq!(user_dir_of("eu4.exe -userdir=", Some(&game)).unwrap(), Path::new(r"D:\named"));
        assert_eq!(user_dir_of("eu4.exe", Some(&game)).unwrap(), Path::new(r"D:\named"));
        let _ = fs::remove_dir_all(game);
    }

    #[test]
    fn prune_dumps_enforces_byte_and_age_limits_while_keeping_newest_and_unrelated_files() {
        let dir = scratch("prune-dumps");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let old = now - Duration::from_secs(40 * 24 * 60 * 60);
        let recent = now - Duration::from_secs(10);
        let put = |name: &str, len: usize, time: SystemTime| {
            let p = dir.join(name);
            fs::write(&p, vec![0u8; len]).unwrap();
            let f = fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.set_modified(time).unwrap();
        };
        put("eu4.exe.100.dmp", 100, old);
        put("eu4.exe.101.dmp", 600, recent - Duration::from_secs(5));
        put("eu4.exe.102.dmp", 600, recent);
        put("unrelated.dmp", 5000, old);
        let removed = prune_dumps_at(&dir, now, None, 800, Duration::from_secs(30 * 24 * 60 * 60), is_wer_dump);
        assert_eq!(removed, 2);
        assert!(!dir.join("eu4.exe.100.dmp").exists());
        assert!(!dir.join("eu4.exe.101.dmp").exists());
        assert!(dir.join("eu4.exe.102.dmp").exists(), "newest dump is always kept");
        assert!(dir.join("unrelated.dmp").exists(), "unrelated files are never touched");
        let _ = fs::remove_dir_all(dir);
    }
}
