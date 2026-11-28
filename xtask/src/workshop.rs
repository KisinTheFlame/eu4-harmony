//! Harmony's item on the Steam Workshop (docs/releases.md): the
//! folder `cargo xtask release` packs for it, and `cargo xtask workshop-upload`,
//! which uploads the folder with SteamCMD.
use crate::{repo, run, Args, Result};
use eu4_harmony_folders::APP_ID;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// The repository's `workshop/`: the sources of what the Workshop shows.
fn sources() -> PathBuf {
    repo().join("workshop")
}

/// Where the release packs for the Workshop: the item's folder `harmony\`,
/// and the files SteamCMD is given.
fn packed() -> PathBuf {
    repo().join("target").join("workshop")
}

/// Harmony's version, the workspace's.
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The published file id `workshop/item.toml` records, 0 before the item
/// exists.
fn published_file_id() -> Result<u64> {
    let path = sources().join("item.toml");
    let text = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    eu4_harmony_folders::parse_published_file_id(&text).ok_or_else(|| format!("{} has no `published_file_id = <number>`", path.display()))
}

/// `item.toml`'s text with `id` recorded.
fn with_id(text: &str, id: u64) -> String {
    let mut out: Vec<String> = text
        .lines()
        .map(|line| match line.split('#').next().and_then(|s| s.split_once('=')) {
            Some((key, _)) if key.trim() == "published_file_id" => format!("published_file_id = {id}"),
            _ => line.to_string(),
        })
        .collect();
    if eu4_harmony_folders::parse_published_file_id(text).is_none() {
        out.push(format!("published_file_id = {id}"));
    }
    out.join("\n") + "\n"
}

/// The item's `descriptor.mod`: the template's, its version filled in, and
/// the item's id once it has one.
fn descriptor(template: &str, version: &str, id: u64) -> String {
    let mut text = template.replace("{version}", version);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    if id != 0 {
        text.push_str(&format!("remote_file_id=\"{id}\"\n"));
    }
    text
}

/// Packs Harmony's item from the player build in `built` into
/// `target\workshop\harmony\`, made anew.
/// Records which commit it was packed from, for the upload to check.
pub fn pack(built: &Path) -> Result<()> {
    let out = packed();
    let item = out.join("harmony");
    if item.exists() {
        fs::remove_dir_all(&item).map_err(|e| format!("cannot remove {}: {e}", item.display()))?;
    }
    fs::create_dir_all(item.join("eu4-harmony").join("fonts")).map_err(|e| format!("cannot make {}: {e}", item.display()))?;
    for (name, to) in crate::files::GAME_FILES {
        copy(&built.join(name), &item.join(to))?;
    }
    let fonts = crate::files::fonts_dir();
    for (name, to) in crate::files::BUNDLED_FONTS {
        copy(&fonts.join(name), &item.join(to))?;
    }
    let template = sources().join("descriptor.mod");
    let template = fs::read_to_string(&template).map_err(|e| format!("cannot read {}: {e}", template.display()))?;
    write(&item.join("descriptor.mod"), &descriptor(&template, version(), published_file_id()?))?;
    let thumbnail = sources().join("thumbnail.png");
    if !thumbnail.is_file() {
        return Err(format!("{} is missing: the item's cover, a square PNG under 1 MB", thumbnail.display()));
    }
    copy(&thumbnail, &item.join("thumbnail.png"))?;
    write(&out.join("harmony.commit"), &commit_stamp()?)?;
    println!("packed for the Workshop:\n  {}", item.display());
    Ok(())
}

/// The commit HEAD names, and whether the checkout has changes besides.
fn commit_stamp() -> Result<String> {
    let head = git(&["rev-parse", "HEAD"])?;
    let clean = git(&["status", "--porcelain"])?.is_empty();
    Ok(format!("{head}{}\n", if clean { "" } else { " dirty" }))
}

/// `cargo xtask workshop-upload --user <login> [--create]`.
pub fn upload(args: &Args) -> Result<()> {
    let user = args.value("user").ok_or("--user <steam login> is needed: SteamCMD logs in as that account and asks for its password itself")?;
    let steamcmd = steamcmd()?;
    let out = packed();
    let item = out.join("harmony");
    let thumbnail = item.join("thumbnail.png");
    if !item.join("descriptor.mod").is_file() || !thumbnail.is_file() {
        return Err(format!("nothing packed in {}: run cargo xtask release", item.display()));
    }
    let id = published_file_id()?;
    let creating = args.flag("create");
    let vdf = out.join("harmony.vdf");
    let fields = if creating {
        if id != 0 {
            return Err(format!("the item exists already ({id} in workshop/item.toml); upload without --create"));
        }
        Item { id: 0, folder: &item, preview: &thumbnail, change_note: "Created.".into(), private: true }
    } else {
        if id == 0 {
            return Err("workshop/item.toml has no published file id: create the item first with --create".into());
        }
        let note = ready_to_update(&item, id)?;
        Item { id, folder: &item, preview: &thumbnail, change_note: note, private: false }
    };
    write(&vdf, &fields.vdf())?;

    // SteamCMD asks for the account's password, and a Steam Guard code,
    // itself, in this console.
    let mut steam = Command::new(steamcmd);
    steam.arg("+login").arg(user).arg("+workshop_build_item").arg(&vdf).arg("+quit");
    run(steam, "SteamCMD")?;

    if creating {
        // SteamCMD writes the new item's id back into the build file.
        let written = fs::read_to_string(&vdf).map_err(|e| format!("cannot read {}: {e}", vdf.display()))?;
        let new = vdf_id(&written).filter(|&id| id != 0).ok_or_else(|| {
            format!("SteamCMD wrote no published file id into {}; take it from the item's page and write it into workshop/item.toml", vdf.display())
        })?;
        let path = sources().join("item.toml");
        let text = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        write(&path, &with_id(&text, new))?;
        println!(
            "created Harmony's item {new}, private: https://steamcommunity.com/sharedfiles/filedetails/?id={new}\n\
             recorded in workshop/item.toml. Next: commit it, merge it into master, create and push\n\
             that version's tag, then run\n  \
             cargo xtask release\n  cargo xtask workshop-upload --user {user}\n\
             so that the proxy is built with the id and the item holds it."
        );
        return Ok(());
    }

    println!("uploaded Harmony {} to https://steamcommunity.com/sharedfiles/filedetails/?id={id}", version());
    Ok(())
}

/// Checks that the packed item may be uploaded as this version; its change
/// note, from `CHANGELOG.md`.
fn ready_to_update(item: &Path, id: u64) -> Result<String> {
    let version = version();
    let release = crate::release::check(&repo(), Some(&format!("v{version}")))?;
    let packed_from = fs::read_to_string(packed().join("harmony.commit")).unwrap_or_default();
    validate_packed(item, &packed_from, version, &release.commit, id)?;
    Ok(change_note(&release.notes))
}

/// The packed item still belongs to this clean tagged source, version and
/// item. Kept separate from SteamCMD so refusals can be checked offline.
fn validate_packed(item: &Path, packed_from: &str, version: &str, commit: &str, id: u64) -> Result<()> {
    if packed_from.trim() != commit {
        return Err("the packed item is not from this commit, or was packed with changes: run cargo xtask release again".into());
    }
    let descriptor = fs::read_to_string(item.join("descriptor.mod")).map_err(|e| format!("cannot read the packed descriptor: {e}"))?;
    let has = |field: &str| descriptor.lines().any(|line| line.trim() == field);
    if !has(&format!("version=\"{version}\"")) || !has(&format!("remote_file_id=\"{id}\"")) {
        return Err("the packed descriptor is not this version's or this item's: run cargo xtask release again".into());
    }
    Ok(())
}

/// A changelog section as Steam's change note: Markdown lists as BBCode
/// lists, the rest as it is.
fn change_note(section: &str) -> String {
    let mut out = Vec::new();
    let mut listing = false;
    for line in section.lines() {
        let item = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
        match (item, listing) {
            (Some(item), false) => {
                out.push("[list]".to_string());
                out.push(format!("[*]{item}"));
                listing = true;
            }
            (Some(item), true) => out.push(format!("[*]{item}")),
            (None, true) if line.starts_with("  ") => {
                let last = out.last_mut().expect("a list item");
                last.push(' ');
                last.push_str(line.trim());
            }
            (None, true) => {
                out.push("[/list]".to_string());
                out.push(line.to_string());
                listing = false;
            }
            (None, false) => out.push(line.to_string()),
        }
    }
    if listing {
        out.push("[/list]".to_string());
    }
    out.join("\n")
}

/// What SteamCMD's `workshop_build_item` is given.
struct Item<'a> {
    id: u64,
    folder: &'a Path,
    preview: &'a Path,
    change_note: String,
    /// Made private: only when creating it, so that an upload never changes
    /// the visibility set on its page.
    private: bool,
}

impl Item<'_> {
    /// The build file, with no description: the page's, by language, stays
    /// as written there.
    fn vdf(&self) -> String {
        let mut fields = vec![
            ("appid", APP_ID.to_string()),
            ("publishedfileid", self.id.to_string()),
            ("contentfolder", self.folder.display().to_string()),
            ("previewfile", self.preview.display().to_string()),
        ];
        if self.private {
            fields.push(("visibility", "2".into()));
        }
        fields.push(("title", "Harmony".into()));
        fields.push(("changenote", self.change_note.clone()));
        let mut text = String::from("\"workshopitem\"\n{\n");
        for (key, value) in fields {
            text.push_str(&format!("\t\"{key}\"\t\t\"{}\"\n", vdf_escape(&value)));
        }
        text.push_str("}\n");
        text
    }
}

/// A value as a quoted VDF string holds it.
fn vdf_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t")
}

/// The `publishedfileid` of a build file.
fn vdf_id(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let mut parts = line.split('"').filter(|part| !part.trim().is_empty());
        (parts.next()? == "publishedfileid").then(|| parts.next()?.parse().ok())?
    })
}

/// SteamCMD: where `STEAMCMD` names it, else on `PATH`, else in the
/// development folder's `steamcmd\` (`%LOCALAPPDATA%\eu4-harmony-dev`, or
/// the one `EU4_HARMONY_DEV_DIR` names), where it is kept on this computer.
fn steamcmd() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("STEAMCMD").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path.clone()).ok_or_else(|| format!("STEAMCMD names {}, which does not exist", path.display()));
    }
    let dev = crate::dev_dir_path().ok();
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .chain(dev.map(|d| d.join("steamcmd")))
        .map(|dir| dir.join("steamcmd.exe"))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "SteamCMD not found: install it (https://developer.valvesoftware.com/wiki/SteamCMD) into %LOCALAPPDATA%\\eu4-harmony-dev\\steamcmd\\ or onto PATH, or name steamcmd.exe in STEAMCMD"
                .into()
        })
}

/// What git prints for `arguments`, trimmed; an error unless it succeeds.
fn git(arguments: &[&str]) -> Result<String> {
    crate::release::git(&repo(), arguments)
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    fs::copy(from, to).map(drop).map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))
}

fn write(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::changelog_section;

    #[test]
    fn reads_and_records_the_published_file_id() {
        assert_eq!(eu4_harmony_folders::parse_published_file_id("published_file_id = 0\n"), Some(0));
        assert_eq!(eu4_harmony_folders::parse_published_file_id("# a note\npublished_file_id=3141592653\n"), Some(3141592653));
        assert_eq!(eu4_harmony_folders::parse_published_file_id("published_file_id = soon\n"), None);
        assert_eq!(eu4_harmony_folders::parse_published_file_id(""), None);
        assert_eq!(with_id("published_file_id = 0\n", 42), "published_file_id = 42\n");
        assert_eq!(with_id("# kept\npublished_file_id = 0", 42), "# kept\npublished_file_id = 42\n");
        assert_eq!(with_id("", 7), "published_file_id = 7\n");
    }

    #[test]
    fn the_descriptor_names_the_item_once_it_exists() {
        let template = "name=\"Harmony\"\nversion=\"{version}\"\n";
        assert_eq!(descriptor(template, "0.1.0", 0), "name=\"Harmony\"\nversion=\"0.1.0\"\n");
        assert_eq!(descriptor(template, "0.1.0", 99), "name=\"Harmony\"\nversion=\"0.1.0\"\nremote_file_id=\"99\"\n");
    }

    #[test]
    fn the_repository_template_is_filled_in() {
        let template = fs::read_to_string(sources().join("descriptor.mod")).unwrap();
        let text = descriptor(&template, "1.2.3", 0);
        assert!(text.contains("version=\"1.2.3\"") && text.contains("name=\"Harmony\"") && text.contains("supported_version=\"v1.37.5\""));
        assert!(!text.contains("{version}") && !text.contains("remote_file_id"));
        assert!(eu4_harmony_folders::parse_published_file_id(&fs::read_to_string(sources().join("item.toml")).unwrap()).is_some());
    }

    #[test]
    fn packed_item_must_match_the_tagged_commit_version_and_item() {
        let item = std::env::temp_dir().join(format!("harmony-workshop-validation-{}", std::process::id()));
        fs::create_dir_all(&item).unwrap();
        fs::write(item.join("descriptor.mod"), "version=\"0.2.0\"\nremote_file_id=\"123\"\n").unwrap();
        assert!(validate_packed(&item, "commit\n", "0.2.0", "commit", 123).is_ok());
        assert!(validate_packed(&item, "commit dirty\n", "0.2.0", "commit", 123).unwrap_err().contains("packed item"));
        assert!(validate_packed(&item, "older\n", "0.2.0", "commit", 123).is_err());
        assert!(validate_packed(&item, "commit\n", "0.2.1", "commit", 123).unwrap_err().contains("packed descriptor"));
        assert!(validate_packed(&item, "commit\n", "0.2.0", "commit", 456).is_err());
        fs::remove_dir_all(item).unwrap();
    }

    #[test]
    fn takes_a_version_section_of_the_changelog() {
        let text = "# Changelog\n\nIntro.\n\n## 0.2.0 (2026-12-01)\n\n- Faster.\n\n## [0.1.0]\n\nFirst release.\n\n- One\n  two.\n- Three\n\n## 0.1.0-rc\n\nNot this.\n";
        assert_eq!(changelog_section(text, "0.2.0").as_deref(), Some("- Faster."));
        assert_eq!(changelog_section(text, "0.1.0").as_deref(), Some("First release.\n\n- One\n  two.\n- Three"));
        assert_eq!(changelog_section(text, "0.1"), None);
        assert_eq!(changelog_section("## 0.3.0\n\n## 0.2.0\n- x\n", "0.3.0"), None, "an empty section is none");
        assert_eq!(change_note("First release.\n\n- One\n  two.\n- Three"), "First release.\n\n[list]\n[*]One two.\n[*]Three\n[/list]");
        assert_eq!(change_note("- a\nAfter."), "[list]\n[*]a\n[/list]\nAfter.");
        let changelog = fs::read_to_string(repo().join("CHANGELOG.md")).unwrap();
        assert!(changelog_section(&changelog, version()).is_some(), "CHANGELOG.md has this version's section");
    }

    #[test]
    fn writes_the_build_file_and_reads_back_the_id() {
        let folder = PathBuf::from("D:\\a \"b\"\\harmony");
        let preview = folder.join("thumbnail.png");
        let item = Item { id: 0, folder: &folder, preview: &preview, change_note: "Line one\n[list]\n[*]x\n[/list]".into(), private: true };
        let vdf = item.vdf();
        assert_eq!(
            vdf,
            "\"workshopitem\"\n{\n\
             \t\"appid\"\t\t\"236850\"\n\
             \t\"publishedfileid\"\t\t\"0\"\n\
             \t\"contentfolder\"\t\t\"D:\\\\a \\\"b\\\"\\\\harmony\"\n\
             \t\"previewfile\"\t\t\"D:\\\\a \\\"b\\\"\\\\harmony\\\\thumbnail.png\"\n\
             \t\"visibility\"\t\t\"2\"\n\
             \t\"title\"\t\t\"Harmony\"\n\
             \t\"changenote\"\t\t\"Line one\\n[list]\\n[*]x\\n[/list]\"\n\
             }\n"
        );
        assert!(!Item { private: false, ..item }.vdf().contains("visibility"), "an update leaves the visibility alone");
        assert_eq!(vdf_id(&vdf), Some(0));
        assert_eq!(vdf_id("\"workshopitem\"\n{\n\t\"appid\"\t\t\"236850\"\n\t\"publishedfileid\"\t\t\"3300000001\"\n}\n"), Some(3300000001));
    }

    #[test]
    fn workshop_descriptions_fit_steams_8000_byte_limit() {
        // Steam's Workshop editor enforces both an 8,000-character textarea cap
        // and an 8,000-byte UTF-8 server limit (k_cchPublishedDocumentDescriptionMax).
        const STEAM_DESCRIPTION_MAX_BYTES: usize = 8000;
        let mut count = 0;
        for entry in fs::read_dir(sources()).unwrap() {
            let path = entry.unwrap().path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !(name.starts_with("description.") && name.ends_with(".bbcode")) {
                continue;
            }
            count += 1;
            let raw = fs::read_to_string(&path).unwrap();
            let normalized = raw.trim_end().replace("\r\n", "\n");
            let crlf_bytes = normalized.len() + normalized.bytes().filter(|&b| b == b'\n').count();
            assert!(crlf_bytes <= STEAM_DESCRIPTION_MAX_BYTES, "{name} is {crlf_bytes} UTF-8 bytes with CRLF (limit {STEAM_DESCRIPTION_MAX_BYTES})");
        }
        assert!(count >= 5, "expected at least 5 workshop description files, found {count}");
    }
}
