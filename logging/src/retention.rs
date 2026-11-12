//! Cleanup is restricted to registered, marked files. Opening a source and
//! reclaiming its registration share the directory lease, so deleting an idle
//! source lease cannot split two writers between different lock-file objects.
use super::{owned, segment, valid_stem, Limits, HEADER, SEGMENTS, SUMMARY};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Read as _},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && ordinary(&m))
}
pub(super) fn plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && ordinary(&m))
}
fn ordinary(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 == 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        !metadata.file_type().is_symlink()
    }
}
/// Older framework builds do not hold leases. On Windows, an exclusive
/// probe also keeps a new writer from rotating an older process's open log.
pub(super) fn available(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    if !plain_file(path) {
        return false;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(path).is_ok()
}

pub(super) fn receipt_matches(path: &Path, source: &str) -> bool {
    if !plain_file(path) {
        return false;
    }
    let mut text = String::new();
    File::open(path).and_then(|f| f.take(256).read_to_string(&mut text)).is_ok() && text == format!("{HEADER}source={source}\n")
}

/// Nonblocking for both registration and cleanup, including the proxy's
/// loader path. A busy registry is a recoverable open/maintenance failure.
pub(super) fn registry_guard(dir: &Path) -> io::Result<File> {
    let path = dir.join(".harmony-logs/cleanup.lease");
    if path.exists() && !plain_file(&path) {
        return Err(io::Error::other("the cleanup lease is not an ordinary file"));
    }
    let guard = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)?;
    guard.try_lock().map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "the log registry is busy"))?;
    Ok(guard)
}

/// Every journal in the same directory shares one maintenance deadline and
/// remaining pressure. Weak entries disappear with the last journal; alternate
/// spellings of a directory resolve to the same key.
pub(super) struct Directory {
    dir: PathBuf,
    pub(super) next: AtomicU64,
    pub(super) excess: AtomicU64,
    pub(super) failures: AtomicU64,
    pub(super) skipped: AtomicU64,
    #[cfg(test)]
    pub(super) passes: AtomicU64,
}
impl Directory {
    pub(super) fn of(dir: &Path) -> io::Result<Arc<Self>> {
        static DIRECTORIES: OnceLock<Mutex<HashMap<PathBuf, Weak<Directory>>>> = OnceLock::new();
        let dir = dir.canonicalize()?;
        let mut directories = DIRECTORIES.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner());
        directories.retain(|_, directory| directory.strong_count() != 0);
        if let Some(directory) = directories.get(&dir).and_then(Weak::upgrade) {
            return Ok(directory);
        }
        let directory = Arc::new(Self {
            dir: dir.clone(),
            next: AtomicU64::new(0),
            excess: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            #[cfg(test)]
            passes: AtomicU64::new(0),
        });
        directories.insert(dir, Arc::downgrade(&directory));
        Ok(directory)
    }

    pub(super) fn maintain(&self, limits: Limits, tick: u64) {
        let next = self.next.load(Ordering::Relaxed);
        if tick < next || self.next.compare_exchange(next, tick.saturating_add(SUMMARY.as_millis() as u64), Ordering::Relaxed, Ordering::Relaxed).is_err() {
            return;
        }
        #[cfg(test)]
        self.passes.fetch_add(1, Ordering::Relaxed);
        match clean(&self.dir, limits, SystemTime::now()) {
            Ok(result) => {
                self.excess.store(result.total.saturating_sub(limits.total), Ordering::Relaxed);
                self.failures.fetch_add(result.failures, Ordering::Relaxed);
                self.skipped.fetch_add(result.skipped, Ordering::Relaxed);
            }
            Err(_) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn reclaimed(&self, bytes: u64) {
        let _ = self.excess.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| Some(n.saturating_sub(bytes)));
    }
}

#[derive(Default)]
pub(super) struct Clean {
    total: u64,
    failures: u64,
    pub(super) skipped: u64,
}
struct Registration {
    source: String,
    lease: Option<File>,
    missing_lease: bool,
    paths: Vec<PathBuf>,
}
struct Candidate {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    inactive: bool,
}

pub(super) fn clean(dir: &Path, limits: Limits, now: SystemTime) -> io::Result<Clean> {
    let _guard = registry_guard(dir)?;
    let registry = dir.join(".harmony-logs");
    let mut result = Clean::default();
    let mut registrations = HashMap::new();
    for entry in fs::read_dir(&registry)?.flatten() {
        let receipt = entry.path();
        if receipt.extension().is_none_or(|e| e != "owned") {
            continue;
        }
        let mut text = String::new();
        let stem = receipt.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if !plain_file(&receipt)
            || stem.is_empty()
            || stem.len() > 220
            || !stem.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            || File::open(&receipt).and_then(|f| f.take(256).read_to_string(&mut text)).is_err()
        {
            result.skipped += 1;
            continue;
        }
        let Some(source) = text.strip_prefix(&format!("{HEADER}source=")).and_then(|s| s.strip_suffix('\n')).filter(|s| valid_stem(s)) else {
            result.skipped += 1;
            continue;
        };
        let lease_path = registry.join(format!("{stem}.lease"));
        let missing_lease = fs::symlink_metadata(&lease_path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound);
        let lease = if missing_lease {
            None
        } else if plain_file(&lease_path) {
            match OpenOptions::new().read(true).write(true).open(&lease_path) {
                Ok(lease) => lease.try_lock().is_ok().then_some(lease),
                Err(_) => {
                    result.skipped += 1;
                    continue;
                }
            }
        } else {
            result.skipped += 1;
            continue;
        };
        let paths =
            (0..SEGMENTS).map(|i| segment(dir, stem, i)).chain([dir.join(format!("{stem}.snapshot.txt")), dir.join(format!("{stem}.snapshot.new"))]).collect();
        registrations.insert(stem.to_string(), Registration { source: source.into(), lease, missing_lease, paths });
    }
    // Interrupted exclusive snapshot writes are found in one directory pass,
    // rather than rescanning the directory for every source.
    for entry in fs::read_dir(dir)?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|s| s.ends_with(".new")) else { continue };
        if let Some(registration) = name.split_once(".snapshot-").and_then(|(stem, _)| registrations.get_mut(stem)) {
            registration.paths.push(entry.path());
        }
    }
    let mut candidates = Vec::new();
    for registration in registrations.values() {
        for path in &registration.paths {
            if !owned(path, &registration.source) {
                continue;
            }
            let Ok(metadata) = fs::metadata(path) else {
                result.failures += 1;
                continue;
            };
            // Unique staging names are never reopened. A closed staging
            // file can therefore be reclaimed even beside a live source.
            // On Windows the exclusive probe fails until its atomic rename.
            let closed_staging =
                cfg!(windows) && path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains(".snapshot-") && n.ends_with(".new")) && available(path);
            candidates.push(Candidate {
                path: path.clone(),
                size: metadata.len(),
                modified: metadata.modified().unwrap_or(UNIX_EPOCH),
                inactive: registration.lease.is_some() || closed_staging,
            });
        }
    }
    candidates.sort_by_key(|c| c.modified);
    result.total = candidates.iter().map(|c| c.size).fold(0u64, u64::saturating_add);
    for c in candidates {
        let expired = now.duration_since(c.modified).unwrap_or_default() > limits.retain;
        if c.inactive && (expired || result.total > limits.total) {
            if fs::remove_file(&c.path).is_ok() {
                result.total = result.total.saturating_sub(c.size);
            } else {
                result.failures += 1;
            }
        }
    }
    for (stem, mut registration) in registrations {
        if (registration.lease.is_none() && !registration.missing_lease) || registration.paths.iter().any(|p| p.try_exists().unwrap_or(true)) {
            continue;
        }
        // No writer can register/reopen this name until _guard is released.
        // Close before unlinking on Windows; the directory lease prevents a
        // new lock-file object from being opened in the gap.
        registration.lease.take();
        if !registration.missing_lease && fs::remove_file(registry.join(format!("{stem}.lease"))).is_err() {
            result.failures += 1;
            continue;
        }
        if fs::remove_file(registry.join(format!("{stem}.owned"))).is_err() {
            result.failures += 1;
        }
    }
    Ok(result)
}
