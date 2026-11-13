//! The troubleshooting files: what someone looking into a player's problem
//! needs, packed into one zip file on the player's desktop, from the
//! settings window's About page. The framework's logs and startup report,
//! the game's own logs, its last crash folders, the settings and the
//! playset, and a summary of where they came from; no saves.
//!
//! It packs on a thread of its own: the game's logs can be tens of
//! megabytes, and the game goes on drawing meanwhile.
use crate::zip::{self, Entry};
use eu4_harmony_folders::UserFolder;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{File, OpenOptions};
#[cfg(test)]
use std::io::Write as _;
use std::io::{Read as _, Seek as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// How many of the last crash folders are packed.
const CRASHES: usize = 3;
/// Uncompressed payload budget. Compression holds at most one bounded source
/// and its compressed form, rather than the entire troubleshooting bundle.
const TOTAL: u64 = 128 << 20;
const FRAMEWORK_FILE: u64 = 8 << 20;
const GAME_FILE: u64 = 2 << 20;
const DUMP_FILE: u64 = 32 << 20;
const MAX_FILES: usize = 1024;

/// Where the packing is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Packing,
    /// The file it wrote, on the desktop.
    Saved(String),
    Failed(String),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);

pub fn state() -> State {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Begins packing, unless it is already.
pub fn start() {
    {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if *state == State::Packing {
            return;
        }
        *state = State::Packing;
    }
    let spawned = std::thread::Builder::new().name("harmony troubleshooting".into()).spawn(|| {
        let done = match pack() {
            Ok(file) => {
                log!("troubleshooting files saved to {}", file.display());
                State::Saved(file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
            Err(e) => {
                log!("troubleshooting files not saved: {e}");
                State::Failed(e)
            }
        };
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = done;
    });
    if let Err(e) = spawned {
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = State::Failed(e.to_string());
    }
}

/// Evidence order is cached once, rather than reading metadata during every
/// heap comparison. Current segments precede archives; newer crash folders
/// precede older ones, even when their files have identical modification times.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
struct Source {
    order: (u8, u8, Reverse<String>, Reverse<SystemTime>),
    name: String,
    path: PathBuf,
}

impl Source {
    fn new(name: String, path: PathBuf) -> Self {
        let segment = match name.rsplit('/').next().map(eu4_harmony_logging::classify_file) {
            Some(eu4_harmony_logging::Artifact::Segment { rank, .. }) => rank,
            _ => 0,
        };
        let crash = if name.starts_with("crashes/") { name.split('/').nth(1).unwrap_or_default() } else { "" };
        let modified = path.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
        Self { order: (priority(&name), segment, Reverse(crash.into()), Reverse(modified)), name, path }
    }
}

/// Keep the best candidates across every folder, independent of enumeration
/// order, with one ZIP entry reserved for about.txt. Count every rejected or
/// displaced candidate without retaining an unbounded list of names.
#[derive(Default)]
struct Sources {
    files: BinaryHeap<Source>,
    omitted: usize,
}

impl Sources {
    fn push(&mut self, source: Source) {
        if self.files.len() < MAX_FILES - 1 {
            self.files.push(source);
        } else {
            if self.files.peek().is_some_and(|worst| source < *worst) {
                self.files.pop();
                self.files.push(source);
            }
            self.omitted = self.omitted.saturating_add(1);
        }
    }
}

impl FromIterator<Source> for Sources {
    fn from_iter<T: IntoIterator<Item = Source>>(iter: T) -> Self {
        let mut sources = Self::default();
        for source in iter {
            sources.push(source);
        }
        sources
    }
}

fn files_in(dir: &Path, under: &str, out: &mut Sources) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|t| t.is_file()) && !matches!(eu4_harmony_logging::classify_file(&name), eu4_harmony_logging::Artifact::Staging) {
            out.push(Source::new(format!("{under}/{name}"), entry.path()));
        }
    }
}

/// Every file the troubleshooting files hold of the user folder `user`.
fn sources(user: &UserFolder) -> Sources {
    let mut out = Sources::default();
    files_in(&user.logs(), "harmony-logs", &mut out);
    files_in(&user.game_logs(), "game-logs", &mut out);
    let crashes = user.crash_folders().unwrap_or_default();
    for name in crashes.iter().rev().take(CRASHES).rev() {
        files_in(&user.crashes().join(name), &format!("crashes/{name}"), &mut out);
    }
    for (name, path) in [
        ("settings/harmony-settings.txt", user.harmony().join("settings.txt")),
        ("settings/settings.txt", user.path().join("settings.txt")),
        ("settings/dlc_load.json", user.path().join("dlc_load.json")),
    ] {
        if path.is_file() {
            out.push(Source::new(name.into(), path));
        }
    }
    out
}

/// The summary's first lines: this core, when, and where the game and its
/// user folder are, with the command line and the playset.
fn about(user: &UserFolder, now: &str) -> Vec<String> {
    let mut lines = vec![
        format!("Harmony {} (api {})", env!("CARGO_PKG_VERSION"), eu4_harmony_remote::API_VERSION),
        format!("packed {now}"),
        format!("game: {}", crate::game_dir().map_or("not known".into(), |d| d.display().to_string())),
        format!("user folder: {}", user.path().display()),
        format!("command line: {}", eu4_harmony_logging::redact(crate::paths::command_line())),
        format!("run: {}", eu4_harmony_logging::run_id()),
        format!("collection: total payload at most {TOTAL} bytes; at most {MAX_FILES} ZIP entries including about.txt; large text files keep their tail; binary dumps are never truncated"),
        "playset:".into(),
    ];
    lines.extend(crate::mods::playset().iter().map(|m| format!("  {m}")));
    lines
}

/// MS-DOS's time and date of `t`, as a zip entry holds them.
fn dos_time(t: &windows_sys::Win32::Foundation::SYSTEMTIME) -> (u16, u16) {
    let time = (t.wHour << 11) | (t.wMinute << 5) | (t.wSecond / 2);
    let date = (t.wYear.saturating_sub(1980) << 9) | (t.wMonth << 5) | t.wDay;
    (time, date)
}

/// A zip file of `files`, each deflated, all written at `when` (MS-DOS's
/// time and date).
#[cfg(test)]
fn zip_of(files: &[(String, Vec<u8>)], when: (u16, u16)) -> Vec<u8> {
    let packed: Vec<(Entry, Vec<u8>)> = files
        .iter()
        .map(|(name, data)| {
            let mut entry = Entry::stored(name.as_bytes(), data);
            let deflated = miniz_oxide::deflate::compress_to_vec(data, 6);
            entry.method = zip::DEFLATED;
            entry.packed = deflated.len();
            (entry.time, entry.date) = when;
            (entry, deflated)
        })
        .collect();
    zip::write(packed.iter().map(|(e, d)| (e, d.as_slice())))
}

/// The desktop, where the player finds the file.
fn desktop() -> crate::Result<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath};
    let mut path = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), &mut path) };
    if hr < 0 || path.is_null() {
        return Err(format!("no desktop folder: {hr:#x}"));
    }
    let len = (0..).take_while(|&i| unsafe { *path.add(i) } != 0).count();
    let dir = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(path, len) });
    unsafe { CoTaskMemFree(path as _) };
    Ok(PathBuf::from(dir))
}

struct Portion {
    bytes: Vec<u8>,
    original: u64,
    offset: u64,
    truncated: bool,
}

/// Reads an inode as it stands now, beside its writer. `take` bounds even a
/// file that grows while it is read. A partial leading text line is dropped
/// so a credential field is not separated from its value by the tail cut.
fn portion(path: &Path, limit: u64, binary: bool) -> std::io::Result<Option<Portion>> {
    let mut file = File::open(path)?;
    let original = file.metadata()?.len();
    if binary && original > limit {
        return Ok(None);
    }
    let mut offset = original.saturating_sub(limit);
    let mut truncated = offset != 0;
    file.seek(std::io::SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity(original.min(limit) as usize);
    file.take(limit).read_to_end(&mut bytes)?;
    if !binary {
        if offset != 0 {
            let cut = bytes.iter().position(|&b| b == b'\n').map_or(bytes.len(), |n| n + 1);
            bytes.drain(..cut);
            offset += cut as u64;
        }
        bytes = eu4_harmony_logging::redact(&String::from_utf8_lossy(&bytes)).into_bytes();
        if bytes.len() as u64 > limit {
            truncated = true;
            let mut end = limit as usize;
            while std::str::from_utf8(&bytes[..end]).is_err() {
                end -= 1;
            }
            bytes.truncate(end);
        }
    }
    Ok(Some(Portion { bytes, original, offset, truncated }))
}

fn priority(name: &str) -> u8 {
    if let Some(leaf) = name.strip_prefix("harmony-logs/") {
        match eu4_harmony_logging::classify_file(leaf) {
            eu4_harmony_logging::Artifact::Segment { stem, .. } if stem == eu4_harmony_logging::CORE_EMERGENCY || stem.starts_with("core-emergency-") => 0,
            eu4_harmony_logging::Artifact::Segment { stem, .. } if stem == eu4_harmony_logging::CORE || stem.starts_with("core-") => 1,
            eu4_harmony_logging::Artifact::Snapshot { .. } => 1,
            _ if leaf.starts_with("report") => 1,
            _ => 3,
        }
    } else if name.starts_with("crashes/") {
        2
    } else if name.starts_with("settings/") {
        4
    } else {
        5
    }
}

fn add<W: std::io::Write>(stream: &mut zip::Stream<W>, name: &str, bytes: &[u8], when: (u16, u16)) -> std::io::Result<()> {
    let compressed = miniz_oxide::deflate::compress_to_vec(bytes, 6);
    let mut entry = Entry::stored(name.as_bytes(), bytes);
    entry.method = zip::DEFLATED;
    entry.packed = compressed.len();
    (entry.time, entry.date) = when;
    stream.entry(&entry, &compressed)
}

fn collect<W: std::io::Write>(output: W, sources: Sources, mut summary: Vec<String>, when: (u16, u16), budget: u64) -> std::io::Result<W> {
    summary.push(format!("files ({} omitted by count limit across all folders):", sources.omitted));
    let mut stream = zip::Stream::new(output);
    let mut left = budget;
    for source in sources.files.into_sorted_vec() {
        let size = std::fs::metadata(&source.path).map(|m| m.len()).unwrap_or(0);
        if left == 0 {
            summary.push(format!("  {}: omitted by total budget, original_bytes={size}", source.name));
            continue;
        }
        let binary = source.path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["dmp", "mdmp", "bin"].iter().any(|x| e.eq_ignore_ascii_case(x)));
        let maximum = if binary {
            DUMP_FILE
        } else if source.name.starts_with("game-logs/") {
            GAME_FILE
        } else {
            FRAMEWORK_FILE
        };
        match portion(&source.path, maximum.min(left), binary) {
            Ok(Some(data)) => {
                summary.push(format!(
                    "  {}: original_bytes={} offset={} collected_bytes={} truncated={} credential_redaction={}",
                    source.name,
                    data.original,
                    data.offset,
                    data.bytes.len(),
                    data.truncated,
                    !binary
                ));
                left = left.saturating_sub(data.bytes.len() as u64);
                add(&mut stream, &source.name, &data.bytes, when)?;
            }
            Ok(None) => summary.push(format!("  {}: binary omitted by size/budget, original_bytes={size}", source.name)),
            Err(e) => summary.push(format!("  {}: not read, {e}", source.name)),
        }
    }
    summary.push(format!("payload_bytes={} budget_bytes={budget}; about.txt is additional bounded metadata", budget - left));
    add(&mut stream, "about.txt", (summary.join("\r\n") + "\r\n").as_bytes(), when)?;
    stream.finish()
}

/// Packs the troubleshooting files onto the desktop; returns the file.
fn pack() -> crate::Result<PathBuf> {
    let game = crate::game_dir().ok_or("the game folder is not known")?;
    // The folder this game runs with, `-userdir` too, as startup takes it.
    let user = UserFolder::new(crate::paths::user_dir(game)?);
    let t = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    let now = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    crate::log::checkpoint();
    let summary = about(&user, &now);
    let name = format!("eu4-harmony-troubleshooting-{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    let desktop = desktop()?;
    // Never overwrite a troubleshooting file the player has already kept.
    let mut chosen = None;
    for n in 0..1000 {
        let path = desktop.join(if n == 0 { format!("{name}.zip") } else { format!("{name}-{n}.zip") });
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => {
                chosen = Some((path, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot write {}: {e}", path.display())),
        }
    }
    let (path, file) = chosen.ok_or("no unused troubleshooting filename")?;
    if let Err(e) = collect(file, sources(&user), summary, dos_time(&t), TOTAL) {
        let _ = std::fs::remove_file(&path);
        return Err(format!("cannot write {}: {e}", path.display()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu4h-troubleshooting-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn the_files_are_the_logs_the_last_crashes_and_the_settings_but_no_saves() {
        let dir = scratch("sources");
        let user = UserFolder::new(&dir);
        put(&user.core_log(), "core");
        put(&user.report(), "report");
        put(&user.game_logs().join("error.log"), "error");
        for name in ["eu4_2026_11_08", "eu4_2026_11_09", "eu4_2026_11_10", "eu4_2026_11_11"] {
            put(&user.crashes().join(name).join("exception.txt"), name);
        }
        put(&user.harmony().join("settings.txt"), "harmony");
        put(&dir.join("settings.txt"), "game");
        put(&dir.join("dlc_load.json"), "{}");
        put(&user.save_games().join("a.eu4"), "save");
        let mut names: Vec<String> = sources(&user).files.into_sorted_vec().into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "crashes/eu4_2026_11_09/exception.txt",
                "crashes/eu4_2026_11_10/exception.txt",
                "crashes/eu4_2026_11_11/exception.txt",
                "game-logs/error.log",
                "harmony-logs/core.log",
                "harmony-logs/report.txt",
                "settings/dlc_load.json",
                "settings/harmony-settings.txt",
                "settings/settings.txt",
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_zip_file_reads_back_what_was_packed() {
        let files = vec![("about.txt".to_string(), b"Harmony".to_vec()), ("game-logs/error.log".to_string(), b"error\n".repeat(1000))];
        let bytes = zip_of(&files, (0x5000, 0x5b6b));
        let mut cursor = std::io::Cursor::new(bytes);
        let entries = zip::directory(&mut cursor, 1 << 16).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].method, zip::DEFLATED);
        assert!(entries[1].packed < 6000, "deflated");
        for (entry, (name, data)) in entries.iter().zip(&files) {
            assert_eq!(entry.name, name.as_bytes());
            assert_eq!((entry.time, entry.date), (0x5000, 0x5b6b));
            assert_eq!(&zip::read(&mut cursor, entry, 1 << 16).unwrap(), data);
        }
    }

    #[test]
    fn a_large_sparse_game_log_is_tailed_redacted_and_not_read_as_one_allocation() {
        let dir = scratch("bounded-tail");
        let path = dir.join("error_old.log");
        let mut file = File::create(&path).unwrap();
        file.set_len(600 << 20).unwrap();
        file.seek(std::io::SeekFrom::End(0)).unwrap();
        writeln!(file, "\n-auth_token=private-value\nlast useful line").unwrap();
        drop(file);
        let got = portion(&path, 4096, false).unwrap().unwrap();
        assert!(got.original > 512 << 20);
        assert!(got.bytes.len() <= 4096);
        let text = String::from_utf8(got.bytes).unwrap();
        assert!(text.contains("last useful line"));
        assert!(!text.contains("private-value"));
        assert!(got.offset > 0);
        assert!(portion(&path, 4096, true).unwrap().is_none(), "binary dumps must not be cut into invalid files");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_total_budget_prefers_framework_evidence_and_records_omissions() {
        let dir = scratch("budget");
        put(&dir.join("core.log"), "essential core record");
        put(&dir.join("error.log"), "noise\n".repeat(100).as_str());
        let source = |name: &str, file: &str| Source::new(name.into(), dir.join(file));
        let bytes = collect(
            Vec::new(),
            [source("game-logs/error.log", "error.log"), source("harmony-logs/core.log", "core.log")].into_iter().collect(),
            vec!["test".into()],
            (0, 0x21),
            21,
        )
        .unwrap();
        let mut zip = std::io::Cursor::new(bytes);
        let entries = zip::directory(&mut zip, 4096).unwrap();
        let core = entries.iter().find(|e| e.name == b"harmony-logs/core.log").unwrap();
        assert_eq!(zip::read(&mut zip, core, 4096).unwrap(), b"essential core record");
        let summary = zip::find(&mut zip, b"about.txt", 4096).unwrap();
        assert!(String::from_utf8(summary).unwrap().contains("omitted by total budget"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn candidate_selection_keeps_critical_files_even_when_enumerated_after_the_count_limit() {
        let dir = scratch("candidate-limit");
        let mut candidates = Sources::default();
        for n in 0..MAX_FILES + 7 {
            let name = format!("a-noise-{n:04}.log");
            put(&dir.join(&name), "noise");
            candidates.push(Source::new(format!("harmony-logs/{name}"), dir.join(name)));
        }
        for name in ["core.log", "core-emergency.log", "core.snapshot.txt"] {
            put(&dir.join(name), "essential");
            candidates.push(Source::new(format!("harmony-logs/{name}"), dir.join(name)));
        }
        assert_eq!(candidates.files.len(), MAX_FILES - 1);
        assert_eq!(candidates.omitted, 11);
        assert!(candidates.files.iter().any(|s| s.name == "harmony-logs/core-emergency.log"));
        let mut scanned = Sources::default();
        files_in(&dir, "harmony-logs", &mut scanned);
        assert_eq!(scanned.omitted, 11);
        assert!(scanned.files.iter().any(|s| s.name == "harmony-logs/core.snapshot.txt"));
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), (0, 0x21), TOTAL).unwrap());
        assert!(zip::find(&mut archive, b"harmony-logs/core.log", 1 << 20).is_ok());
        let summary = zip::find(&mut archive, b"about.txt", 1 << 20).unwrap();
        assert!(String::from_utf8(summary).unwrap().contains("11 omitted by count limit across all folders"));
        assert_eq!(zip::directory(&mut archive, 1 << 20).unwrap().len(), MAX_FILES);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tight_budgets_keep_current_segments_and_the_newest_crash_first() {
        let dir = scratch("recent-evidence");
        put(&dir.join("current"), "current evidence");
        put(&dir.join("old"), "old evidence that must be omitted");
        for (wanted, names) in [
            ("harmony-logs/core.log", ["harmony-logs/core.2.log", "harmony-logs/core.3.log"]),
            ("crashes/eu4_2026_11_13/exception.txt", ["crashes/eu4_2026_11_11/exception.txt", "crashes/eu4_2026_11_12/exception.txt"]),
        ] {
            let mut candidates: Sources = names.into_iter().map(|name| Source::new(name.into(), dir.join("old"))).collect();
            candidates.push(Source::new(wanted.into(), dir.join("current")));
            let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), (0, 0x21), 16).unwrap());
            assert_eq!(zip::find(&mut archive, wanted.as_bytes(), 4096).unwrap(), b"current evidence");
            let summary = String::from_utf8(zip::find(&mut archive, b"about.txt", 4096).unwrap()).unwrap();
            assert_eq!(summary.matches("omitted by total budget").count(), 2);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_time_is_given_as_ms_dos_has_it() {
        let t = windows_sys::Win32::Foundation::SYSTEMTIME {
            wYear: 2026,
            wMonth: 11,
            wDayOfWeek: 3,
            wDay: 11,
            wHour: 20,
            wMinute: 31,
            wSecond: 7,
            wMilliseconds: 0,
        };
        assert_eq!(dos_time(&t), ((20 << 11) | (31 << 5) | 3, (46 << 9) | (11 << 5) | 11));
    }
}
