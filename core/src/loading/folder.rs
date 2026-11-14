//! A folder's listing and the names Windows reads as they are written, shared
//! by the lookups (`super::lookups`) and the listed folders (`super::listed`).
//!
//! Both answer ASCII paths from a folder's listing instead of asking Windows
//! each time, and both leave a name Windows would read otherwise than as
//! written to the game's stat. The classification and the rule are written
//! once here; each speed-up keeps its own seam (PhysFS's stat, the folder
//! archive's stat) and its own switch.
use std::collections::HashMap;

/// What a listing says a name is: a file, a folder, or something else (a
/// link).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Kind {
    File,
    Folder,
    Other,
}

/// A folder's listing: its ASCII names in lower case, each a file, a folder,
/// or something else; None where it could not be listed. Non-ASCII names are
/// skipped: Windows's ordinal case folding maps none of them to ASCII, and
/// only ASCII paths reach the listings.
pub(super) fn list(folder: &str) -> Option<HashMap<Box<str>, Kind>> {
    let mut names = HashMap::new();
    for entry in std::fs::read_dir(folder).ok()? {
        let entry = entry.ok()?;
        let kind = match entry.file_type().ok()? {
            t if t.is_symlink() => Kind::Other,
            t if t.is_dir() => Kind::Folder,
            t if t.is_file() => Kind::File,
            _ => Kind::Other,
        };
        if let Ok(name) = entry.file_name().into_string() {
            if name.is_ascii() {
                names.insert(name.to_ascii_lowercase().into(), kind);
            }
        }
    }
    Some(names)
}

/// Whether Windows reads a name as it is written: ASCII, with no control
/// character nor any of `\ / : < > " | ? * ~`, not empty nor `.` nor `..`, and
/// not ending in `.` or a space. A path is answered name by name, split on its
/// separator, so the folder answered for may itself be a short name
/// (`RUNNER~1`).
pub(super) fn read_as_written(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && name.is_ascii()
        && !name.bytes().any(|c| c.is_ascii_control() || b"\\/:<>\"|?*~".contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_windows_reads_as_they_are_written() {
        for name in ["icon.dds", "gfx", "interface", "a b", "1.310", "file-name", "x", "a.b.c"] {
            assert!(read_as_written(name), "{name:?}");
        }
        for name in
            ["", ".", "..", "icon.", "icon ", "a<b", "a>b", "a\"b", "a|b", "a?b", "a*b", "icon~1.dds", "a:b", "a\\b", "a/b", "a\u{1}b", "\u{66f4}\u{65b0}"]
        {
            assert!(!read_as_written(name), "{name:?}");
        }
    }

    #[test]
    fn a_folders_listing_classifies_its_ascii_names_in_lower_case() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Interface")).unwrap();
        std::fs::write(dir.join("Icon.dds"), b"x").unwrap();
        std::fs::write(dir.join("\u{66f4}\u{65b0}.txt"), b"x").unwrap();
        let listing = list(dir.to_str().unwrap()).unwrap();
        assert_eq!(listing.get("interface"), Some(&Kind::Folder));
        assert_eq!(listing.get("icon.dds"), Some(&Kind::File));
        // Non-ASCII names are skipped, and the keys are lower case.
        assert_eq!(listing.len(), 2);
        assert!(!listing.contains_key("Icon.dds"));
        assert!(list(dir.join("missing").to_str().unwrap()).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
