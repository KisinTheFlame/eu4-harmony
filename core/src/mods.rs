//! Discovers enabled Paradox mods and the EU4 Harmony manifests inside them.
//!
//! The launcher writes the active playset to `<user_dir>/dlc_load.json`; each
//! entry names a `.mod` descriptor whose `path=` points at the mod directory.
//! A mod carries an extension when it has `eu4-harmony/manifest.json`.
use crate::{notice::Message, Result};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Deserialize)]
struct DlcLoad {
    #[serde(default)]
    enabled_mods: Vec<String>,
}

pub use eu4_harmony_manifest::{Manifest, UiFragment};

#[derive(Debug)]
pub struct Descriptor {
    pub name: String,
    pub path: Option<PathBuf>,
    pub archive: Option<String>,
}

#[derive(Debug)]
pub struct DiscoveredMod {
    pub descriptor_file: PathBuf,
    pub descriptor: Descriptor,
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// Minimal reader for the `key="value"` lines of a `.mod` descriptor.
pub fn parse_descriptor(text: &str, user_dir: &Path) -> Descriptor {
    let mut name = String::new();
    let mut path = None;
    let mut archive = None;
    for line in text.lines() {
        let Some((key, value)) = eu4_harmony_manifest::descriptor_entry(line) else { continue };
        let value = value.replace('\\', "/");
        match key {
            "name" => name = value,
            "path" => {
                let p = PathBuf::from(&value);
                path = Some(if p.is_absolute() { p } else { user_dir.join(p) });
            }
            "archive" => archive = Some(value),
            _ => {}
        }
    }
    Descriptor { name, path, archive }
}

/// An enabled mod that discovery skipped, and why.
pub struct Skipped {
    pub text: String,
    /// What to tell the player when the mod carries an EU4 Harmony manifest,
    /// since they expect it to run.
    pub notice: Option<Message>,
}

impl Skipped {
    fn other(text: String) -> Self {
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

/// Whether `descriptor`, as a save or the playset names a mod's descriptor,
/// is the one `other` names: the same path in either case, with either
/// slash.
pub fn same_descriptor(descriptor: &str, other: &str) -> bool {
    let normal = |path: &str| path.trim().replace('\\', "/").to_lowercase();
    normal(descriptor) == normal(other)
}

/// Returns the enabled mods that carry a manifest, in playset order, plus
/// every entry that was skipped and why. Keeps the playset for [`playset`].
pub fn discover(user_dir: &Path) -> Result<(Vec<DiscoveredMod>, Vec<Skipped>)> {
    let dlc_load = user_dir.join("dlc_load.json");
    let text = std::fs::read_to_string(&dlc_load).map_err(|e| format!("read {}: {e}", dlc_load.display()))?;
    let load: DlcLoad = serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", dlc_load.display()))?;
    let _ = PLAYSET.set(load.enabled_mods.clone());
    let mut mods = Vec::new();
    let mut notes = Vec::new();
    for entry in load.enabled_mods {
        let descriptor_file = user_dir.join(entry.replace('/', "\\"));
        let text = match std::fs::read_to_string(&descriptor_file) {
            Ok(t) => t,
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
        let manifest_file = dir.join("eu4-harmony").join("manifest.json");
        if !manifest_file.is_file() {
            continue;
        }
        let manifest: Manifest = match std::fs::read_to_string(&manifest_file).map_err(|e| e.to_string()).and_then(|t| Manifest::parse(&t)) {
            Ok(m) => m,
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
        mods.push(DiscoveredMod { descriptor_file, descriptor, dir, manifest });
    }
    Ok((mods, notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_resolves_relative_and_absolute_paths() {
        let user = Path::new("C:\\u");
        let d = parse_descriptor("name=\"X\"\npath=\"mod/x\"\nremote_file_id=\"1\"\n", user);
        assert_eq!(d.name, "X");
        assert_eq!(d.path.unwrap(), Path::new("C:\\u").join("mod/x"));
        let d = parse_descriptor("path=\"E:/Steam/workshop/1\"\n", user);
        assert_eq!(d.path.unwrap(), PathBuf::from("E:/Steam/workshop/1"));
        let d = parse_descriptor("archive=\"mod/x.zip\"\n", user);
        assert!(d.path.is_none());
        assert_eq!(d.archive.as_deref(), Some("mod/x.zip"));
    }

    #[test]
    fn a_descriptor_is_the_same_path_whatever_its_case_and_slashes() {
        assert!(same_descriptor("mod/ugc_2976470733.mod", "mod/ugc_2976470733.mod"));
        assert!(same_descriptor("mod\\Harmony.mod", " MOD/harmony.mod"));
        assert!(!same_descriptor("mod/ugc_2976470733.mod", "mod/ugc_3047072888.mod"));
    }
}
