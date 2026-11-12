//! Cleanup is restricted to registered, marked files. A separate lease makes
//! readable logs safe from another process's rotation or retention pass.
use super::{owned, segment, valid_stem, Limits, HEADER, SEGMENTS};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read as _},
    path::{Path, PathBuf},
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

struct Candidate {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    inactive: bool,
}

pub(super) fn clean(dir: &Path, limits: Limits, now: SystemTime) -> io::Result<u64> {
    let registry = dir.join(".harmony-logs");
    if !plain_directory(&registry) {
        return Ok(0);
    }
    let cleanup_path = registry.join("cleanup.lease");
    if cleanup_path.exists() && !plain_file(&cleanup_path) {
        return Ok(0);
    }
    let cleanup = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(cleanup_path)?;
    if cleanup.try_lock().is_err() {
        return Ok(0);
    }
    // Hold every lease acquired for this pass until the corresponding files
    // have been considered; a new writer must choose another name meanwhile.
    let mut leases = Vec::new();
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&registry)?.flatten() {
        let receipt = entry.path();
        if receipt.extension().is_none_or(|e| e != "owned") || !plain_file(&receipt) {
            continue;
        }
        let Some(stem) = receipt.file_stem().and_then(|s| s.to_str()) else { continue };
        if stem.len() > 220 || !stem.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) {
            continue;
        }
        let mut text = String::new();
        File::open(&receipt)?.take(256).read_to_string(&mut text)?;
        let Some(source) = text.strip_prefix(&format!("{HEADER}source=")).and_then(|s| s.strip_suffix('\n')) else { continue };
        if !valid_stem(source) {
            continue;
        }
        let lease_path = registry.join(format!("{stem}.lease"));
        if !plain_file(&lease_path) {
            continue;
        }
        let lease = OpenOptions::new().read(true).write(true).open(lease_path)?;
        let inactive = lease.try_lock().is_ok();
        if inactive {
            leases.push(lease);
        }
        let paths = (0..SEGMENTS).map(|i| segment(dir, stem, i)).chain([dir.join(format!("{stem}.snapshot.txt")), dir.join(format!("{stem}.snapshot.new"))]);
        for path in paths {
            if !owned(&path, source) {
                continue;
            }
            let metadata = fs::metadata(&path)?;
            candidates.push(Candidate { path, size: metadata.len(), modified: metadata.modified().unwrap_or(UNIX_EPOCH), inactive });
        }
    }
    candidates.sort_by_key(|c| c.modified);
    let mut total = candidates.iter().map(|c| c.size).fold(0u64, u64::saturating_add);
    for c in candidates {
        let expired = now.duration_since(c.modified).unwrap_or_default() > limits.retain;
        if c.inactive && (expired || total > limits.total) && fs::remove_file(&c.path).is_ok() {
            total = total.saturating_sub(c.size);
        }
    }
    Ok(total)
}
