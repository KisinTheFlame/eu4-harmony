//! Discovers enabled Paradox mods and the EU4 Harmony manifests inside them.
//!
//! The launcher writes the active playset to `<user_dir>/dlc_load.json`; each
//! entry names a `.mod` descriptor whose `path=` points at the mod directory.
//! A mod carries an extension when it has `eu4-harmony/manifest.json`.
use crate::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct DlcLoad {
    #[serde(default)]
    enabled_mods: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub api_version: String,
    /// DLL path relative to the `eu4-harmony/` directory. Not loaded by the prototype yet.
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
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        let value = value.trim().trim_matches('"').replace('\\', "/");
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

/// Returns the enabled mods that carry a manifest, in playset order, plus a
/// report line for every entry that was skipped and why.
pub fn discover(user_dir: &Path) -> Result<(Vec<DiscoveredMod>, Vec<String>)> {
    let dlc_load = user_dir.join("dlc_load.json");
    let text = std::fs::read_to_string(&dlc_load).map_err(|e| format!("read {}: {e}", dlc_load.display()))?;
    let load: DlcLoad = serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", dlc_load.display()))?;
    let mut mods = Vec::new();
    let mut notes = Vec::new();
    for entry in load.enabled_mods {
        let descriptor_file = user_dir.join(entry.replace('/', "\\"));
        let text = match std::fs::read_to_string(&descriptor_file) {
            Ok(t) => t,
            Err(e) => {
                notes.push(format!("{}: cannot read descriptor: {e}", descriptor_file.display()));
                continue;
            }
        };
        let descriptor = parse_descriptor(&text, user_dir);
        let Some(dir) = descriptor.path.clone() else {
            match &descriptor.archive {
                Some(a) => notes.push(format!("{}: archive mods are not supported ({a})", descriptor.name)),
                None => notes.push(format!("{}: descriptor has no path", descriptor_file.display())),
            }
            continue;
        };
        let manifest_file = dir.join("eu4-harmony").join("manifest.json");
        if !manifest_file.is_file() {
            continue;
        }
        let manifest: Manifest = match std::fs::read_to_string(&manifest_file)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        {
            Ok(m) => m,
            Err(e) => {
                notes.push(format!("{}: bad manifest: {e}", descriptor.name));
                continue;
            }
        };
        if mods.iter().any(|m: &DiscoveredMod| m.manifest.id == manifest.id) {
            notes.push(format!("{}: duplicate id {:?}, skipped", descriptor.name, manifest.id));
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
}
