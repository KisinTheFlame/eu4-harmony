//! Discovers enabled Paradox mods and the EU4 Harmony manifests inside them.
//!
//! The launcher writes the active playset to `<user_dir>/dlc_load.json`; each
//! entry names a `.mod` descriptor whose `path=` points at the mod directory.
//! A mod is a Harmony mod when it has `eu4-harmony/manifest.toml`; where its
//! other files are says the rest (ADR manifest-in-toml-layout-by-convention).
use crate::{base::notice::Message, base::Result};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub use eu4_harmony_manifest::{same_descriptor, Descriptor, HarmonyMod, Layout, Manifest, Permissions, Playset};

#[derive(Debug)]
pub struct DiscoveredMod {
    pub descriptor: Descriptor,
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// Where its files are: the extension, its interface fragments.
    pub layout: Layout,
}

/// Minimal reader for the `key="value"` lines of a `.mod` descriptor.
pub fn parse_descriptor(text: &str, user_dir: &Path) -> Descriptor {
    Descriptor::parse(text, user_dir)
}

/// An enabled mod that discovery skipped, and why.
pub struct Skipped {
    pub text: String,
    /// What to tell the player when the mod carries an EU4 Harmony manifest,
    /// since they expect it to run.
    pub notice: Option<Message>,
}

impl Skipped {
    pub(crate) fn other(text: String) -> Self {
        Self { text, notice: None }
    }
}

/// The playset the game started with, as [`discover`] read it.
static PLAYSET: OnceLock<Vec<String>> = OnceLock::new();

/// The mods the game started with: the playset's descriptors as the launcher
/// names them in `dlc_load.json` (`mod/ugc_2976470733.mod`), which the game
/// reads as it starts, read by [`discover`] at startup; none when it could
/// not be read, as the game then has none.
pub fn playset() -> &'static [String] {
    PLAYSET.get().map_or(&[], Vec::as_slice)
}

/// Returns the enabled mods that carry a manifest, in playset order, plus
/// every entry that was skipped and why. Keeps the playset for [`playset`].
pub fn discover(user_dir: &Path) -> Result<(Vec<DiscoveredMod>, Vec<Skipped>)> {
    let dlc_load = user_dir.join("dlc_load.json");
    let enabled = if dlc_load.exists() { Playset::read(&dlc_load)?.enabled() } else { Vec::new() };
    let _ = PLAYSET.set(enabled.clone());
    let mut mods = Vec::new();
    let mut notes = Vec::new();
    for entry in enabled {
        let descriptor_file = user_dir.join(entry.replace('/', "\\"));
        let text = match std::fs::read(&descriptor_file) {
            Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
            Err(e) => {
                notes.push(Skipped::other(format!("{}: cannot read descriptor: {e}", descriptor_file.display())));
                continue;
            }
        };
        let descriptor = parse_descriptor(&text, user_dir);
        let Some(dir) = descriptor.path.clone() else {
            match &descriptor.archive {
                Some(a) => notes.push(Skipped::other(format!("{}: archive mods are not supported ({a})", descriptor.name))),
                None => notes.push(Skipped::other(format!("{}: descriptor has no path", descriptor_file.display()))),
            }
            continue;
        };
        if !eu4_harmony_manifest::is_harmony(&dir) {
            continue;
        }
        let HarmonyMod { manifest, layout } = match HarmonyMod::read(&dir) {
            Ok(read) => read,
            Err(e) => {
                let notice = Message::new("EU4HARMONY_SKIPPED_MANIFEST").arg("MOD", &descriptor.name).arg("DETAIL", &e);
                notes.push(Skipped { text: format!("{}: bad manifest: {e}", descriptor.name), notice: Some(notice) });
                continue;
            }
        };
        if mods.iter().any(|m: &DiscoveredMod| m.manifest.id == manifest.id) {
            let notice = Message::new("EU4HARMONY_SKIPPED_DUPLICATE_ID").arg("MOD", &descriptor.name).arg("ID", &manifest.id);
            notes.push(Skipped {
                text: format!("{}: id {:?} is already used by an earlier mod in the playset", descriptor.name, manifest.id),
                notice: Some(notice),
            });
            continue;
        }
        mods.push(DiscoveredMod { descriptor, dir, manifest, layout });
    }
    Ok((mods, notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_resolves_relative_and_absolute_paths() {
        let user = Path::new("C:\\u");
        let d = parse_descriptor("name=\"X\"\nversion=\"1.2\"\npath=\"mod/x\"\nremote_file_id=\"1\"\n", user);
        assert_eq!((d.name.as_str(), d.version.as_deref()), ("X", Some("1.2")));
        assert_eq!(d.path.unwrap(), Path::new("C:\\u").join("mod/x"));
        let d = parse_descriptor("path=\"E:/Steam/workshop/1\"\n", user);
        assert!(d.version.is_none());
        assert_eq!(d.path.unwrap(), PathBuf::from("E:/Steam/workshop/1"));
        let d = parse_descriptor("archive=\"mod/x.zip\"\n", user);
        assert!(d.path.is_none());
        assert_eq!(d.archive.as_deref(), Some("mod/x.zip"));
    }

    #[test]
    fn discover_reads_non_utf8_mod_descriptors_without_skipping_harmony_mods() {
        let dir = std::env::temp_dir().join(format!("eu4h-mods-nonutf8-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mod_dir = dir.join("mod").join("latin1_mod");
        std::fs::create_dir_all(mod_dir.join("eu4-harmony")).unwrap();
        std::fs::write(mod_dir.join("eu4-harmony").join("manifest.toml"), "id = \"latin1\"\napi_version = \"0.3\"\n").unwrap();
        // Windows-1252 accented character in name= alongside an ASCII path=.
        std::fs::write(dir.join("mod").join("latin1.mod"), b"name=\"Caf\xe9 Mod\"\npath=\"mod/latin1_mod\"\n").unwrap();
        std::fs::write(dir.join("dlc_load.json"), r#"{"enabled_mods":["mod/latin1.mod"]}"#).unwrap();

        let (mods, skipped) = discover(&dir).unwrap();
        assert_eq!(mods.len(), 1, "skipped: {:?}", skipped.iter().map(|s| &s.text).collect::<Vec<_>>());
        assert_eq!(mods[0].manifest.id, "latin1");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_descriptor_is_the_same_path_whatever_its_case_and_slashes() {
        assert!(same_descriptor("mod/ugc_2976470733.mod", "mod/ugc_2976470733.mod"));
        assert!(same_descriptor("mod\\Harmony.mod", " MOD/harmony.mod"));
        assert!(!same_descriptor("mod/ugc_2976470733.mod", "mod/ugc_3047072888.mod"));
    }

    #[test]
    fn discover_treats_missing_dlc_load_as_empty_playset() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-mods-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (mods, notes) = discover(&dir).unwrap();
        assert!(mods.is_empty());
        assert!(notes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
