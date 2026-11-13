//! The files that make a Paradox mod an EU4 Harmony mod, as the core reads
//! them at startup and the command line tool writes and checks them:
//! `eu4-harmony/manifest.toml`, where the layout puts the rest of a mod's
//! files (ADR manifest-in-toml-layout-by-convention), the `key="value"`
//! lines of a `.mod` descriptor and the game's other plain files.
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The manifest's file, in a mod's `eu4-harmony/` folder.
pub const FILE: &str = "manifest.toml";

/// The extension's entry, relative to `eu4-harmony/`.
pub const EXTENSION: &str = "lua/main.lua";

/// `eu4-harmony/manifest.toml`: what cannot be told from where a mod's files
/// are.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub api_version: String,
    /// A key in the mod's localisation, for the mods window.
    #[serde(default)]
    pub description: Option<String>,
    /// The sensitive capabilities the extension declares
    /// (ADR sensitive-capabilities-are-declared).
    #[serde(default)]
    pub permissions: Permissions,
}

/// The sensitive capabilities a manifest declares.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// The real date and time.
    #[serde(default)]
    pub clock: bool,
    /// The extension's data folder.
    #[serde(default)]
    pub files: bool,
    /// The hosts HTTP and HTTPS requests may go to, by name or address, with
    /// a port where it is not the default.
    #[serde(default)]
    pub network: Vec<String>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let manifest: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        for entry in &manifest.permissions.network {
            host(entry).map_err(|e| format!("permissions.network: {e}"))?;
        }
        Ok(manifest)
    }

    /// Whether the mod was written for the catalogue version `provided`: in
    /// 0.x only that version itself (ADR one-capability-catalogue).
    pub fn built_for(&self, provided: &str) -> bool {
        self.api_version == provided
    }

    /// Why a framework that provides the catalogue version `provided` cannot
    /// run the mod's extension, when it cannot.
    pub fn unsupported(&self, provided: &str) -> Option<String> {
        (!self.built_for(provided)).then(|| format!("written for api_version {}, this Harmony provides {provided}", self.api_version))
    }
}

/// A host as `network` lists it and a URL names it: a name or an address
/// (an IPv6 one in brackets), lowercased, and its port where one is given.
pub fn host(text: &str) -> Result<(String, Option<u16>), String> {
    let wrong = || format!("{text:?} is not a host name or address, with :port where it is not the default");
    let (name, port) = match text.strip_prefix('[') {
        Some(rest) => {
            let (address, after) = rest.split_once(']').ok_or_else(wrong)?;
            let ok = !address.is_empty() && address.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.');
            if !ok {
                return Err(wrong());
            }
            match after {
                "" => (format!("[{address}]"), None),
                _ => (format!("[{address}]"), Some(after.strip_prefix(':').ok_or_else(wrong)?)),
            }
        }
        None => match text.split_once(':') {
            Some((name, port)) => (name.to_string(), Some(port)),
            None => (text.to_string(), None),
        },
    };
    let named = !name.starts_with('[');
    if named && (name.is_empty() || name.starts_with(['.', '-']) || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')) {
        return Err(wrong());
    }
    let port = match port {
        None => None,
        Some(p) => match p.parse::<u16>() {
            Ok(n) if n > 0 && p.chars().all(|c| c.is_ascii_digit()) => Some(n),
            _ => return Err(wrong()),
        },
    };
    Ok((name.to_ascii_lowercase(), port))
}

/// An interface fragment, found where the layout puts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiFragment {
    /// Fragment file, relative to the `eu4-harmony/` directory, with `/`.
    pub file: String,
    /// Vanilla window the fragment is spliced into: its folder's name.
    pub into: String,
    /// `"buttons"` for a fragment in the window's `buttons/` folder, laid out
    /// in the framework's button area.
    pub area: Option<String>,
}

/// A Harmony mod's files where the layout puts them
/// (ADR manifest-in-toml-layout-by-convention).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The extension's entry, [`EXTENSION`], when the mod has one.
    pub extension: Option<String>,
    /// `ui/<window>/*.gui`, then `ui/<window>/buttons/*.gui`, window by window
    /// and file by file in name order.
    pub ui: Vec<UiFragment>,
    /// Files under `ui/` where the layout has no place for them, relative to
    /// `eu4-harmony/`.
    pub misplaced: Vec<String>,
}

/// Where the layout puts a mod's files, read from its `eu4-harmony/` folder.
pub fn layout(harmony: &Path) -> Layout {
    let extension = harmony.join(EXTENSION).is_file().then(|| EXTENSION.to_string());
    let mut ui = Vec::new();
    let mut misplaced = Vec::new();
    for (window, path) in entries(&harmony.join("ui")) {
        if !path.is_dir() {
            misplaced.push(format!("ui/{window}"));
            continue;
        }
        let mut buttons = Vec::new();
        for (name, inner) in entries(&path) {
            if inner.is_dir() && name == "buttons" {
                for (button, at) in entries(&inner) {
                    if at.is_file() && is_gui(&button) {
                        buttons.push(UiFragment { file: format!("ui/{window}/buttons/{button}"), into: window.clone(), area: Some("buttons".into()) });
                    } else {
                        misplaced.push(format!("ui/{window}/buttons/{button}"));
                    }
                }
            } else if inner.is_file() && is_gui(&name) {
                ui.push(UiFragment { file: format!("ui/{window}/{name}"), into: window.clone(), area: None });
            } else {
                misplaced.push(format!("ui/{window}/{name}"));
            }
        }
        ui.extend(buttons);
    }
    Layout { extension, ui, misplaced }
}

fn is_gui(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".gui")
}

/// A folder's entries by name, in name order; none when it cannot be read.
fn entries(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> =
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect();
    found.sort();
    found
}

/// A Harmony mod as its folder holds it: its manifest, and where the layout
/// puts its files. Startup, an extension reload, `eu4-harmony check` and
/// `package` read a mod this one way.
#[derive(Debug)]
pub struct HarmonyMod {
    pub manifest: Manifest,
    pub layout: Layout,
}

/// Whether the mod folder `root` holds a Harmony mod: one with
/// `eu4-harmony/manifest.toml`.
pub fn is_harmony(root: &Path) -> bool {
    root.join("eu4-harmony").join(FILE).is_file()
}

impl HarmonyMod {
    /// The Harmony mod in the mod folder `root`; or why its manifest does not
    /// read, as `eu4-harmony/manifest.toml: <why>`.
    pub fn read(root: &Path) -> Result<Self, String> {
        let harmony = root.join("eu4-harmony");
        let manifest = std::fs::read_to_string(harmony.join(FILE)).map_err(|e| e.to_string()).and_then(|t| Manifest::parse(&t));
        let manifest = manifest.map_err(|e| format!("eu4-harmony/{FILE}: {e}"))?;
        Ok(Self { manifest, layout: layout(&harmony) })
    }

    /// What a framework that provides the catalogue version `provided` finds
    /// wrong with the mod, each a line that says where: an api_version it
    /// does not provide, files where the layout puts nothing.
    pub fn problems(&self, provided: &str) -> Vec<String> {
        let version = self.manifest.unsupported(provided).map(|why| format!("eu4-harmony/{FILE}: {why}"));
        let misplaced = self.layout.misplaced.iter().map(|f| format!("eu4-harmony/{f}: not where the layout puts anything, so the game never reads it"));
        version.into_iter().chain(misplaced).collect()
    }
}

/// The descriptor keys that say where the mod is: the launcher writes them,
/// and an upload leaves them out.
pub const LOCATION_KEYS: [&str; 2] = ["path", "archive"];

/// A Paradox `.mod` descriptor, resolved against the user folder it sits in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    pub name: String,
    /// The mod's own version, as its descriptor gives it.
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    pub archive: Option<String>,
}

/// `path` written with `/`, without Windows' `\\?\` prefix.
pub fn slash_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).replace('\\', "/")
}

impl Descriptor {
    /// Reads the `key="value"` lines of a `.mod` descriptor against `user_dir`.
    pub fn parse(text: &str, user_dir: &Path) -> Self {
        let mut name = String::new();
        let mut version = None;
        let mut path = None;
        let mut archive = None;
        for (key, value) in line_entries(text) {
            let value = value.replace('\\', "/");
            match key {
                "name" => name = value,
                "version" => version = Some(value),
                "path" => {
                    let p = PathBuf::from(&value);
                    path = Some(if p.is_absolute() { p } else { user_dir.join(p) });
                }
                "archive" => archive = Some(value),
                _ => {}
            }
        }
        Self { name, version, path, archive }
    }

    /// Descriptor text without the `path=` and `archive=` lines the launcher adds.
    pub fn without_location(text: &str) -> String {
        let mut out = String::new();
        for line in text.lines() {
            if !line_entry(line).is_some_and(|(key, _)| LOCATION_KEYS.contains(&key)) {
                out.push_str(line.strip_prefix('\u{feff}').unwrap_or(line));
                out.push('\n');
            }
        }
        out
    }

    /// Descriptor text pointing at `mod_dir`, with any earlier location lines removed.
    pub fn with_path(text: &str, mod_dir: &Path) -> String {
        format!("{}path=\"{}\"\n", Self::without_location(text), slash_path(mod_dir))
    }

    /// Descriptor text relocated from `original_user` into another user folder:
    /// relative `path=` and `archive=` entries become absolute paths under
    /// `original_user`, while already-absolute paths and trailing comments stay.
    pub fn relocate(text: &str, original_user: &Path) -> Result<String, String> {
        let mut out = String::new();
        for raw_line in text.lines() {
            let line = raw_line.strip_prefix('\u{feff}').unwrap_or(raw_line);
            let mut quoted = false;
            let comment_at = line
                .char_indices()
                .find(|&(_, c)| {
                    quoted ^= c == '"';
                    c == '#' && !quoted
                })
                .map_or(line.len(), |(at, _)| at);
            let field = line[..comment_at].split_once('=').filter(|(key, _)| LOCATION_KEYS.contains(&key.trim()));
            match field {
                Some((key, rest)) => {
                    let trimmed = rest.trim_start();
                    let (value, trailing) = if let Some(after_open) = trimmed.strip_prefix('"') {
                        let Some((inside, after_close)) = after_open.split_once('"') else {
                            return Err("a copied mod descriptor has an incomplete path string".into());
                        };
                        (inside, format!("{}{}", after_close, &line[comment_at..]))
                    } else {
                        (trimmed.trim_end(), line[comment_at..].to_string())
                    };
                    if value.is_empty() {
                        return Err("a copied mod descriptor has an empty path".into());
                    }
                    let candidate = PathBuf::from(value.replace('/', "\\"));
                    let resolved = if candidate.is_absolute() { candidate } else { original_user.join(candidate) };
                    out.push_str(&format!("{}=\"{}\"{}\n", key.trim(), slash_path(&resolved), trailing));
                }
                None => {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        Ok(out)
    }
}

/// Whether `descriptor`, as a save or the playset names a mod's descriptor,
/// is the one `other` names: the same path in either case, with either slash.
pub fn same_descriptor(descriptor: &str, other: &str) -> bool {
    let normal = |path: &str| path.trim().replace('\\', "/").to_lowercase();
    normal(descriptor) == normal(other)
}

/// The active playset in `<user_dir>/dlc_load.json`, preserving non-mod fields
/// such as `disabled_dlcs` when written back.
#[derive(Debug, Clone)]
pub struct Playset(serde_json::Value);

impl Default for Playset {
    fn default() -> Self {
        Self(serde_json::json!({ "enabled_mods": [], "disabled_dlcs": [] }))
    }
}

impl Playset {
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        let Some(object) = value.as_object() else {
            return Err("playset is not a JSON object".into());
        };
        if let Some(enabled) = object.get("enabled_mods") {
            let Some(list) = enabled.as_array() else {
                return Err("enabled_mods is not a list".into());
            };
            if !list.iter().all(serde_json::Value::is_string) {
                return Err("enabled_mods holds a non-string entry".into());
            }
        }
        Ok(Self(value))
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("parse {}: {e}", path.display()))
    }

    pub fn read_or_default(path: &Path) -> Self {
        Self::read(path).unwrap_or_default()
    }

    pub fn enabled(&self) -> Vec<String> {
        self.0
            .get("enabled_mods")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.iter().filter_map(serde_json::Value::as_str).map(String::from).collect())
            .unwrap_or_default()
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        let kept: Vec<serde_json::Value> = self.enabled().into_iter().filter(|e| keep(e)).map(serde_json::Value::String).collect();
        self.0["enabled_mods"] = serde_json::Value::Array(kept);
    }

    pub fn enable(&mut self, descriptor: impl Into<String>) {
        let descriptor = descriptor.into();
        let mut list = self.enabled();
        if !list.iter().any(|existing| same_descriptor(existing, &descriptor)) {
            list.push(descriptor);
        }
        self.0["enabled_mods"] = serde_json::Value::Array(list.into_iter().map(serde_json::Value::String).collect());
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.0).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }
}

/// The key and the value of one line of the game's plain `key="value"`
/// files, as a `.mod` descriptor, the game's `settings.txt`, the framework
/// settings and a plain save's header have them: both trimmed, the value's
/// quotes taken off. A comment, from a `#` outside quotes, is no part of the
/// line, nor a byte order mark before the key, which Notepad writes. None for
/// a line with no `=` before its comment, such as a block's `}`.
pub fn line_entry(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_prefix('\u{feff}').unwrap_or(line);
    let mut quoted = false;
    let end = line
        .char_indices()
        .find(|&(_, c)| {
            quoted ^= c == '"';
            c == '#' && !quoted
        })
        .map_or(line.len(), |(at, _)| at);
    let (key, value) = line[..end].split_once('=')?;
    Some((key.trim(), value.trim().trim_matches('"')))
}

/// Every entry of a file of such lines, in order; see [`line_entry`].
pub fn line_entries(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(line_entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_holds_what_the_files_cannot_say() {
        let manifest = Manifest::parse(
            r#"
            id = "m"
            api_version = "0.2"
            description = "M_DESCRIPTION"
            permissions = { clock = true, network = ["api.example.com", "192.168.1.10:8080"] }
            "#,
        )
        .unwrap();
        assert_eq!((manifest.id.as_str(), manifest.api_version.as_str(), manifest.description.as_deref()), ("m", "0.2", Some("M_DESCRIPTION")));
        assert_eq!(manifest.permissions, Permissions { clock: true, files: false, network: vec!["api.example.com".into(), "192.168.1.10:8080".into()] });
        let bare = Manifest::parse("id = \"m\"\napi_version = \"0.2\"").unwrap();
        assert_eq!((bare.description, bare.permissions), (None, Permissions::default()));
        // What the layout says, or a word misspelled, is refused.
        assert!(Manifest::parse("id = \"m\"\napi_version = \"0.2\"\nextension = \"x.dll\"").is_err());
        assert!(Manifest::parse("id = \"m\"\napi_version = \"0.2\"\npermissions = { clocks = true }").is_err());
        assert!(Manifest::parse("id = \"m\"").is_err());
        assert!(Manifest::parse(
            "id = \"m\"
api_version = \"0.2\"
permissions = { network = [\"https://x.com\"] }"
        )
        .unwrap_err()
        .contains("permissions.network"));
    }

    #[test]
    fn a_host_is_a_name_or_an_address_with_a_port_where_one_is_given() {
        assert_eq!(host("API.Example.com").unwrap(), ("api.example.com".to_string(), None));
        assert_eq!(host("192.168.1.10:8080").unwrap(), ("192.168.1.10".to_string(), Some(8080)));
        assert_eq!(host("[::1]:3000").unwrap(), ("[::1]".to_string(), Some(3000)));
        assert_eq!(host("[::1]").unwrap(), ("[::1]".to_string(), None));
        for bad in ["", "x.com:", "x.com:0", "x.com:65536", "x.com:+80", "a b", "user@x.com", "https://x.com", "x.com/", "[::1", "[::1]x", "-x.com", "[]"] {
            assert!(host(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_layout_says_where_each_file_goes() {
        let dir = std::env::temp_dir().join(format!("eu4h-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in [
            "lua/main.lua",
            "ui/single_unitpanel/b.gui",
            "ui/single_unitpanel/a.gui",
            "ui/single_unitpanel/buttons/zone.gui",
            "ui/topbar/x.gui",
            "ui/stray.gui",
            "ui/topbar/notes.txt",
        ] {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let found = layout(&dir);
        assert_eq!(found.extension.as_deref(), Some(EXTENSION));
        let ui: Vec<(&str, &str, Option<&str>)> = found.ui.iter().map(|f| (f.file.as_str(), f.into.as_str(), f.area.as_deref())).collect();
        assert_eq!(
            ui,
            [
                ("ui/single_unitpanel/a.gui", "single_unitpanel", None),
                ("ui/single_unitpanel/b.gui", "single_unitpanel", None),
                ("ui/single_unitpanel/buttons/zone.gui", "single_unitpanel", Some("buttons")),
                ("ui/topbar/x.gui", "topbar", None),
            ]
        );
        assert_eq!(found.misplaced, ["ui/stray.gui", "ui/topbar/notes.txt"]);
        assert_eq!(layout(&dir.join("nowhere")), Layout::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_mod_is_read_with_its_problems_worded_once() {
        let root = std::env::temp_dir().join(format!("eu4h-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(!is_harmony(&root));
        let write = |file: &str, text: &str| {
            let path = root.join("eu4-harmony").join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(FILE, "id = \"m\"");
        assert!(is_harmony(&root));
        let refused = HarmonyMod::read(&root).unwrap_err();
        assert!(refused.starts_with("eu4-harmony/manifest.toml: ") && refused.contains("api_version"), "{refused}");
        write(FILE, "id = \"m\"\napi_version = \"0.1\"");
        write("lua/main.lua", "return {}");
        write("ui/stray.gui", "");
        let read = HarmonyMod::read(&root).unwrap();
        assert_eq!(read.layout.extension.as_deref(), Some(EXTENSION));
        assert_eq!(
            read.problems("0.2"),
            [
                "eu4-harmony/manifest.toml: written for api_version 0.1, this Harmony provides 0.2",
                "eu4-harmony/ui/stray.gui: not where the layout puts anything, so the game never reads it",
            ]
        );
        assert_eq!(read.manifest.unsupported("0.1"), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn lines_give_their_key_and_value() {
        assert_eq!(line_entry(r#" path = "D:/m" "#), Some(("path", "D:/m")));
        assert_eq!(line_entry("tags={"), Some(("tags", "{")));
        assert_eq!(line_entry("}"), None);
        assert_eq!(line_entry("language=l_english"), Some(("language", "l_english")));
        // A comment is no part of the line, but a `#` in quotes is.
        assert_eq!(line_entry(r#"name="Mod #1" # the first"#), Some(("name", "Mod #1")));
        assert_eq!(line_entry("# version=\"1\""), None);
        assert_eq!(line_entry("version=\"1\"#=2"), Some(("version", "1")));
        // A byte order mark, as Notepad saves a file with.
        assert_eq!(line_entries("\u{feff}body_font=\"Microsoft YaHei\"\r\n}\r\nx = 2").collect::<Vec<_>>(), [("body_font", "Microsoft YaHei"), ("x", "2")]);
        assert_eq!(line_entry(""), None);
    }

    #[test]
    fn descriptors_parse_strip_location_and_relocate_with_bom_and_comments() {
        let user = Path::new(r"C:\Users\p\Documents\EU4");
        let raw = "\u{feff}name=\"Mod #1\" # display\nversion=\"1.2\"\npath=\"mod/local #1\" # kept\narchive=\"mod/pack.zip\"\npath_hint=\"kept\"\n";
        let parsed = Descriptor::parse(raw, user);
        assert_eq!((parsed.name.as_str(), parsed.version.as_deref()), ("Mod #1", Some("1.2")));
        assert_eq!(parsed.path.unwrap(), user.join("mod/local #1"));
        assert_eq!(parsed.archive.as_deref(), Some("mod/pack.zip"));

        let stripped = Descriptor::without_location(raw);
        assert_eq!(stripped, "name=\"Mod #1\" # display\nversion=\"1.2\"\npath_hint=\"kept\"\n");

        let linked = Descriptor::with_path(raw, Path::new(r"\\?\D:\mods\m"));
        assert_eq!(linked, "name=\"Mod #1\" # display\nversion=\"1.2\"\npath_hint=\"kept\"\npath=\"D:/mods/m\"\n");

        let relocated = Descriptor::relocate(raw, user).unwrap();
        assert_eq!(
            relocated,
            "name=\"Mod #1\" # display\nversion=\"1.2\"\npath=\"C:/Users/p/Documents/EU4/mod/local #1\" # kept\narchive=\"C:/Users/p/Documents/EU4/mod/pack.zip\"\npath_hint=\"kept\"\n"
        );
        let workshop = Descriptor::relocate("name=\"W\"\npath=\"E:/Steam/workshop/content/236850/1\"\n", user).unwrap();
        assert_eq!(workshop, "name=\"W\"\npath=\"E:/Steam/workshop/content/236850/1\"\n");
        assert!(Descriptor::relocate("path=\"unclosed\n", user).is_err());
        assert!(Descriptor::relocate("path=\"\"\n", user).is_err());
        assert!(same_descriptor("mod\\Harmony.mod", " MOD/harmony.mod"));
    }

    #[test]
    fn playset_preserves_other_fields_and_deduplicates_descriptors() {
        let dir = std::env::temp_dir().join(format!("eu4h-playset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("dlc_load.json");
        let mut playset = Playset::parse("\u{feff}{\"enabled_mods\":[\"mod/ugc_1.mod\"],\"disabled_dlcs\":[\"dlc/a.dlc\"],\"extra\":1}").unwrap();
        playset.enable("MOD\\ugc_1.mod");
        playset.enable("mod/eu4harmony-test-qa.mod");
        assert_eq!(playset.enabled(), ["mod/ugc_1.mod", "mod/eu4harmony-test-qa.mod"]);
        playset.retain(|e| !e.starts_with("mod/eu4harmony-test-"));
        playset.write(&file).unwrap();
        let reread: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(reread["enabled_mods"], serde_json::json!(["mod/ugc_1.mod"]));
        assert_eq!(reread["disabled_dlcs"], serde_json::json!(["dlc/a.dlc"]));
        assert_eq!(reread["extra"], serde_json::json!(1));
        let _ = std::fs::remove_dir_all(dir);
    }
}
