//! Shared directory listing cache and Windows path-normalisation rules used
//! by both `super::lookups` (read-only roots) and `super::listed` (watched
//! PhysFS folder roots).
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// What a listing says a name is: a file, a folder, or something else (a
/// link).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    File,
    Folder,
    Other,
}

type Listing = HashMap<Box<str>, Kind>;

/// What the cache knows of a folder.
#[derive(Clone)]
enum Folder {
    Listed(Arc<Listing>),
    /// It is not there, or a file is where it would be.
    Missing,
    /// Not for the listings to say (unreadable folder, symlink, etc.).
    Unknown,
}

/// Normalizes a filesystem folder path to lower-case backslash form ending in `\`.
pub(super) fn normalize_dir(path: &Path) -> String {
    normalize_dir_str(&path.to_string_lossy())
}

/// Normalizes a folder path string to lower-case backslash form ending in `\`.
pub(super) fn normalize_dir_str(path: &str) -> String {
    format!("{}\\", path.trim_end_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase())
}

/// Normalizes a relative path into `out` in lower case, separated by `/`, if
/// every segment is one Windows reads as written. Accepts either `/` or `\` as
/// input separator when `accept_backslash` is true; otherwise `\` is rejected.
pub(super) fn lowered_relative<'a>(path: &[u8], accept_backslash: bool, out: &'a mut [u8; 512]) -> Option<&'a str> {
    if path.is_empty() || path.len() > out.len() {
        return None;
    }
    for (o, &c) in out.iter_mut().zip(path) {
        if !c.is_ascii() {
            return None;
        }
        *o = if accept_backslash && c == b'\\' { b'/' } else { c.to_ascii_lowercase() };
    }
    let lower = std::str::from_utf8(&out[..path.len()]).ok()?;
    lower.split('/').all(read_as_written).then_some(lower)
}

/// A folder's listing: its ASCII names in lower case, each a file, a folder,
/// or something else; None where it could not be listed. Non-ASCII names are
/// skipped: Windows's ordinal case folding maps none of them to ASCII, and
/// only ASCII paths reach the listings.
pub(super) fn list(folder: &str) -> Option<Listing> {
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

/// Lazily populated hierarchical directory listing cache rooted at one folder,
/// with negative caching and race-free invalidation.
pub(super) struct DirectoryCache {
    /// Original root path ending in `\`, used when calling `read_dir`.
    path: Box<str>,
    /// Lower-case backslash-normalized root path ending in `\`, used for prefix matching.
    lower: Box<str>,
    /// Cached listings keyed by lower-case `/`-separated relative folder path (`""` for root).
    listings: RwLock<HashMap<Box<str>, Option<Arc<Listing>>>>,
    /// Monotonic invalidation counter: a listing read across an invalidation is not cached.
    changes: AtomicU64,
}

impl DirectoryCache {
    /// Creates a cache rooted at `path` (automatically ensures trailing `\`).
    pub(super) fn new(path: &str) -> Self {
        let trimmed = path.trim_end_matches(['\\', '/']);
        let with_sep = format!("{trimmed}\\");
        let lower = with_sep.replace('/', "\\").to_ascii_lowercase();
        Self { path: with_sep.into(), lower: lower.into(), listings: RwLock::new(HashMap::new()), changes: AtomicU64::new(0) }
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn lower(&self) -> &str {
        &self.lower
    }

    fn folder(&self, folder: &str) -> Folder {
        if let Some(found) = self.listings.read().unwrap_or_else(|e| e.into_inner()).get(folder) {
            return found.clone().map_or(Folder::Unknown, Folder::Listed);
        }
        if let Some((parent, name)) = folder.rsplit_once('/').or_else(|| (!folder.is_empty()).then_some(("", folder))) {
            match self.folder(parent) {
                Folder::Listed(listing) => match listing.get(name) {
                    Some(Kind::Folder) => {}
                    Some(Kind::Other) => return Folder::Unknown,
                    Some(Kind::File) | None => return Folder::Missing,
                },
                other => return other,
            }
        }
        let changes = self.changes.load(Ordering::Acquire);
        let made = list(&format!("{}{}", self.path, folder.replace('/', "\\"))).map(Arc::new);
        let mut listings = self.listings.write().unwrap_or_else(|e| e.into_inner());
        if self.changes.load(Ordering::Acquire) == changes {
            listings.insert(folder.into(), made.clone());
        }
        made.map_or(Folder::Unknown, Folder::Listed)
    }

    /// Resolves `path` (lower-case, `/`-separated, relative to this root).
    /// Returns `Ok(Some(kind))` when present, `Ok(None)` when certainly absent,
    /// or `Err(())` when the cache cannot answer with certainty.
    pub(super) fn kind(&self, path: &str) -> Result<Option<Kind>, ()> {
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        match self.folder(folder) {
            Folder::Listed(listing) => Ok(listing.get(name).copied()),
            Folder::Missing => Ok(None),
            Folder::Unknown => Err(()),
        }
    }

    /// Invalidates cached listings for every ancestor of `path`, `path` itself,
    /// and any cached folders beneath it.
    pub(super) fn invalidate_path(&self, path: &str) {
        let mut listings = self.listings.write().unwrap_or_else(|e| e.into_inner());
        self.changes.fetch_add(1, Ordering::AcqRel);
        let mut parent = path;
        while let Some((folder, _)) = parent.rsplit_once('/') {
            listings.remove(folder);
            parent = folder;
        }
        listings.remove("");
        let below = format!("{path}/");
        listings.retain(|k, _| **k != *path && !k.starts_with(&below));
    }

    /// Drops every cached listing under this root.
    pub(super) fn invalidate_all(&self) {
        let mut listings = self.listings.write().unwrap_or_else(|e| e.into_inner());
        self.changes.fetch_add(1, Ordering::AcqRel);
        listings.clear();
    }

    /// Invalidates every cache in `caches` whose root contains `dir/path`.
    pub(super) fn invalidate_written<'a>(caches: impl IntoIterator<Item = &'a Self>, dir: &str, path: &str) {
        let dir = normalize_dir_str(dir);
        let whole = format!("{dir}{}", path.replace('/', "\\").to_ascii_lowercase());
        for cache in caches {
            if let Some(rest) = whole.strip_prefix(cache.lower()) {
                let parts: Vec<&str> = rest.split('\\').filter(|part| !part.is_empty()).collect();
                if parts.is_empty() || parts.iter().any(|&part| part == "." || part == "..") {
                    cache.invalidate_all();
                } else {
                    cache.invalidate_path(&parts.join("/"));
                }
            }
        }
    }
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
        assert_eq!(listing.len(), 2);
        assert!(!listing.contains_key("Icon.dds"));
        assert!(list(dir.join("missing").to_str().unwrap()).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn directory_cache_resolves_hierarchies_caches_misses_and_invalidates_ancestors() {
        let dir = std::env::temp_dir().join(format!("eu4-harmony-dircache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("gfx").join("Interface")).unwrap();
        std::fs::write(dir.join("gfx").join("Interface").join("Icon.dds"), b"x").unwrap();

        let cache = DirectoryCache::new(&dir.to_string_lossy());
        assert_eq!(cache.kind("gfx/interface/icon.dds"), Ok(Some(Kind::File)));
        assert_eq!(cache.kind("gfx/interface"), Ok(Some(Kind::Folder)));
        assert_eq!(cache.kind("gfx/interface/missing.dds"), Ok(None));
        assert_eq!(cache.kind("gfx/interface/icon.dds/child"), Ok(None));
        assert_eq!(cache.kind("nonexistent/a/b"), Ok(None));

        // New nested folder stays invisible until invalidated.
        std::fs::create_dir_all(dir.join("nonexistent").join("a")).unwrap();
        std::fs::write(dir.join("nonexistent").join("a").join("b"), b"x").unwrap();
        assert_eq!(cache.kind("nonexistent/a/b"), Ok(None));

        DirectoryCache::invalidate_written([&cache], &dir.to_string_lossy(), "nonexistent//a/b");
        assert_eq!(cache.kind("nonexistent/a/b"), Ok(Some(Kind::File)));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
