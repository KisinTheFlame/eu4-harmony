//! The game's folders as every program of EU4 Harmony finds them: the game
//! folder, the user folder (GLOSSARY), and the names of what the user folder
//! holds that the framework writes and its tools read. The core, the command
//! line tool, the client and `cargo xtask` share it, so that the game folder
//! is found in one order and a file of the user folder is named in one place.
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

pub type Result<T> = std::result::Result<T, String>;

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

/// The game folder holding the program at `exe`, when it is installed there.
fn installed_in(exe: &Path) -> Option<PathBuf> {
    let game = exe.parent()?.parent()?;
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
    match switch_value(command_line, "-userdir").filter(|v| !v.is_empty()) {
        Some(named) => {
            let path = PathBuf::from(named);
            Ok(match game_dir {
                Some(game) if !path.is_absolute() => game.join(path),
                _ => path,
            })
        }
        None => user_dir(game_dir),
    }
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

/// The Windows user's Documents folder.
fn documents() -> Result<PathBuf> {
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    };
    let mut out: *mut u16 = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut out) };
    if hr < 0 || out.is_null() {
        return Err(format!("cannot find the Documents folder: {hr:#x}"));
    }
    let len = (0..).take_while(|&i| unsafe { *out.add(i) } != 0).count();
    let path = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, len) });
    unsafe { CoTaskMemFree(out as _) };
    Ok(PathBuf::from(path))
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
        let tool = game.join("eu4-harmony").join("eu4-harmony.exe");
        assert_eq!(installed_in(&tool), None, "no eu4.exe beside it");
        fs::write(game.join("eu4.exe"), "").unwrap();
        assert_eq!(installed_in(&tool), Some(game.clone()));
        let _ = fs::remove_dir_all(game);
    }

    #[test]
    fn a_game_folder_given_comes_first() {
        let given = Path::new(r"X:\a game");
        assert_eq!(game_dir(Some(given)).unwrap(), given);
    }

    #[test]
    fn the_new_crash_folders_are_those_not_known_the_last_written_last() {
        let user = UserFolder::new(scratch("crashes"));
        assert_eq!(user.crash_folders().unwrap(), Vec::<String>::new(), "no crashes folder");
        for name in ["old", "b", "a"] {
            fs::create_dir_all(user.crashes().join(name)).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
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
        let game = Path::new(r"E:\game");
        assert_eq!(user_dir_of(r"eu4.exe -userdir=D:\u", Some(game)).unwrap(), Path::new(r"D:\u"));
        assert_eq!(user_dir_of("eu4.exe -userdir=u", Some(game)).unwrap(), Path::new(r"E:\game\u"));
        assert_eq!(user_dir_of("eu4.exe -userdir=", Some(game)).unwrap(), user_dir(Some(game)).unwrap());
        assert_eq!(user_dir_of("eu4.exe", Some(game)).unwrap(), user_dir(Some(game)).unwrap());
    }
}
