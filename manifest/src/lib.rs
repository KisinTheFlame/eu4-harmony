//! The files that make a Paradox mod an EU4 Harmony mod, as the core reads
//! them at startup and the command line tool writes and checks them:
//! `eu4-harmony/manifest.json`, and the lines of a `.mod` descriptor.
use serde::Deserialize;

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
}

/// The descriptor keys that say where the mod is: the launcher writes them,
/// and an upload leaves them out.
pub const LOCATION_KEYS: [&str; 2] = ["path", "archive"];

/// The key and the unquoted value of a descriptor line `key="value"`.
pub fn descriptor_entry(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim().trim_matches('"')))
}

#[cfg(test)]
mod tests {
    use super::*;

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
