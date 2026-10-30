//! The files that make a Paradox mod an EU4 Harmony mod, as the core reads
//! them at startup and the command line tool writes and checks them:
//! `eu4-harmony/manifest.json`, the lines of a `.mod` descriptor, and the
//! user directory whose `mod\` folder holds the descriptors.
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `eu4-harmony/manifest.json`.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub api_version: String,
    /// DLL path relative to the `eu4-harmony/` directory.
    #[serde(default)]
    pub extension: Option<String>,
    #[serde(default)]
    pub ui: Vec<UiFragment>,
}

#[derive(Debug, Deserialize)]
pub struct UiFragment {
    /// Fragment file, relative to the `eu4-harmony/` directory.
    pub file: String,
    /// Vanilla window the fragment is spliced into.
    pub into: String,
    /// `"buttons"` places the fragment's elements in the framework's button area.
    #[serde(default)]
    pub area: Option<String>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    /// Whether the mod was built for the interface version `provided`: in
    /// 0.x only that version itself (docs/v1.md, "Interface version").
    pub fn built_for(&self, provided: &str) -> bool {
        self.api_version == provided
    }
}

/// The descriptor keys that say where the mod is: the launcher writes them,
/// and an upload leaves them out.
pub const LOCATION_KEYS: [&str; 2] = ["path", "archive"];

/// The key and the unquoted value of a descriptor line `key="value"`.
pub fn descriptor_entry(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim().trim_matches('"')))
}

/// The game's user directory, where it keeps saves, settings and the `.mod`
/// descriptors in `mod\`, as the game itself decides it: the folder
/// `userdir.txt` in the game folder names, and otherwise
/// `Documents\Paradox Interactive\Europa Universalis IV`.
pub fn user_dir(game_dir: Option<&Path>) -> Result<PathBuf, String> {
    match game_dir.and_then(named_user_dir) {
        Some(dir) => Ok(dir),
        None => Ok(documents()?.join("Paradox Interactive").join("Europa Universalis IV")),
    }
}

/// The folder `userdir.txt` in the game folder names, absolute or relative to
/// the game folder; none when there is no such file or it is empty.
pub fn named_user_dir(game_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(game_dir.join("userdir.txt")).ok()?;
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return None;
    }
    let path = PathBuf::from(text);
    Some(if path.is_absolute() { path } else { game_dir.join(path) })
}

/// The Windows user's Documents folder.
fn documents() -> Result<PathBuf, String> {
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

    #[test]
    fn userdir_txt_names_the_user_directory() {
        let game = std::env::temp_dir().join(format!("eu4h-userdir-{}", std::process::id()));
        std::fs::create_dir_all(&game).unwrap();
        assert_eq!(named_user_dir(&game), None);
        for (text, expected) in [("\u{feff}  D:\\EU4 user\r\n", Some(PathBuf::from(r"D:\EU4 user"))), ("user", Some(game.join("user"))), (" \r\n", None)] {
            std::fs::write(game.join("userdir.txt"), text).unwrap();
            assert_eq!(named_user_dir(&game), expected, "{text:?}");
            if let Some(dir) = expected {
                assert_eq!(user_dir(Some(&game)).unwrap(), dir);
            }
        }
        let _ = std::fs::remove_dir_all(game);
    }

    #[test]
    fn a_manifest_needs_where_its_fragments_go() {
        let manifest =
            Manifest::parse(r#"{ "id": "m", "api_version": "0.1", "ui": [{ "file": "ui/b.gui", "into": "single_unitpanel", "area": "buttons" }] }"#).unwrap();
        assert_eq!((manifest.id.as_str(), manifest.ui[0].into.as_str(), manifest.ui[0].area.as_deref()), ("m", "single_unitpanel", Some("buttons")));
        assert!(manifest.extension.is_none());
        assert!(Manifest::parse(r#"{ "id": "m", "api_version": "0.1", "ui": [{ "file": "ui/b.gui" }] }"#).is_err());
    }

    #[test]
    fn descriptor_lines_give_their_key_and_value() {
        assert_eq!(descriptor_entry(r#" path = "D:/m" "#), Some(("path", "D:/m")));
        assert_eq!(descriptor_entry("tags={"), Some(("tags", "{")));
        assert_eq!(descriptor_entry("}"), None);
    }
}
