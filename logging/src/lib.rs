//! Bounded logs owned by Harmony. The proxy and the core use the same file
//! policy without linking the proxy to game, shell or extension code.
//!
//! A source holds a separate lease, not a lock on its readable log. Another
//! process uses a different stem; cleanup skips every leased source. Only
//! files registered here AND carrying our header are eligible for cleanup.
mod reader;
mod retention;
mod text;

pub use reader::{
    classify_file, extract_startup_reason, is_header, is_trouble_line, record_message, source_segments, split_record, startup_failure_reason, troubles_since,
    Artifact, SourceMark, CORE, CORE_EMERGENCY, PROXY,
};
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, BufRead as _, Read as _, Seek as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub use text::redact;
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::{
        SystemInformation::{GetLocalTime, GetTickCount64},
        Threading::{GetCurrentProcess, GetCurrentThreadId, GetProcessTimes},
    },
};

/// Limits apply to framework logs, never to game logs, dumps or saved zip files.
pub const SEGMENT_BYTES: u64 = 8 << 20;
pub const SEGMENTS: usize = 4;
pub const TOTAL_BYTES: u64 = 256 << 20;
pub const RETAIN: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const SUMMARY: Duration = Duration::from_secs(60);
const SNAPSHOT: Duration = Duration::from_secs(30);
const EVENTS: usize = 256;
const ERRORS: usize = 256;
const MESSAGE_BYTES: usize = 64 << 10;
const EVENT_BYTES: usize = 2048;
const HEADER: &str = "# Harmony owned log v1 ";
static SERIAL: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
struct Limits {
    segment: u64,
    total: u64,
    retain: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self { segment: SEGMENT_BYTES, total: TOTAL_BYTES, retain: RETAIN }
    }
}

/// A filesystem-safe extension source, distinct from framework sources even
/// for a mod named `core`. Escape unsafe UTF-8 bytes instead of accepting a
/// path from a manifest. Long IDs retain a stable suffix.
pub fn extension_source(id: &str) -> String {
    let mut name = String::from("extension-");
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in id.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        if byte.is_ascii_alphanumeric() || byte == b'-' {
            name.push(char::from(byte));
        } else {
            name.push_str(&format!("_{byte:02x}"));
        }
    }
    if name.len() > 120 {
        name.truncate(96);
        name.push_str(&format!("-{hash:016x}"));
    }
    name
}

/// The process identity is the same in the proxy and core, including a core
/// reloaded in this process. PID alone would confuse two uses of a reused PID.
pub fn run_id() -> &'static str {
    static RUN: OnceLock<String> = OnceLock::new();
    RUN.get_or_init(|| {
        let zero = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero(), zero(), zero(), zero());
        let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) };
        let created = if ok != 0 { (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime) } else { unix_ms() };
        format!("{}-{created:x}", std::process::id())
    })
}

/// Local wall clock, including its date. `up_ms` on each record is the
/// monotonic system clock, so records remain orderable across a clock change.
pub fn now() -> String {
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds)
}

pub fn monotonic_ms() -> u64 {
    unsafe { GetTickCount64() }
}
fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Note,
    /// A low-frequency change worth keeping in the diagnostic snapshot.
    Event,
    /// Exact repetitions are counted and summarized. Distinct errors are kept.
    Error,
    /// Never deduplicated; bypasses normal maintenance and its directory scan.
    Critical,
}

/// Classifies existing log messages without making callers put game state in
/// every record. Explicit lifecycle records use `Event` at their source.
pub fn kind(message: &str) -> Kind {
    if message.starts_with("crash:") || message.starts_with("panic ") || message.contains("fatal error") {
        Kind::Critical
    } else if ["error ", "error:", "failed ", "stopped:", "cannot ", "could not "].iter().any(|p| message.starts_with(p))
        || [": error in ", ": failed ", " not loaded:"].iter().any(|p| message.contains(p))
    {
        Kind::Error
    } else {
        Kind::Note
    }
}

struct Repeated {
    count: u64,
    first: String,
    last: String,
    emit_single: bool,
}

#[derive(Default)]
struct Operation {
    begun: u64,
    succeeded: u64,
    failed: u64,
    active: u64,
    last_ms: u64,
}

/// One source, with readable current/previous files and two older segments.
/// The current file is never deleted by another source's cleanup. Four active
/// segments per source can exceed the total budget if many sources are live;
/// in that case safety takes precedence and the next cleanup retries.
pub struct Journal {
    dir: PathBuf,
    directory: Arc<retention::Directory>,
    stem: String,
    source: String,
    _lease: File,
    file: Option<File>,
    bytes: u64,
    limits: Limits,
    errors: HashMap<String, Repeated>,
    events: VecDeque<String>,
    last_summary: u64,
    last_snapshot: u64,
    error_snapshot: bool,
    last_cleanup: u64,
    failures: u64,
    cleanup_failures: u64,
    truncated: u64,
    sequence: u64,
    operations: HashMap<String, Operation>,
    omitted_operations: u64,
}

impl Journal {
    pub fn open(dir: &Path, source: &str) -> io::Result<Self> {
        Self::open_with(dir, source, Limits::default())
    }

    fn open_with(dir: &Path, source: &str, limits: Limits) -> io::Result<Self> {
        if !valid_stem(source) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid log source"));
        }
        fs::create_dir_all(dir)?;
        let registry = dir.join(".harmony-logs");
        fs::create_dir_all(&registry)?;
        if !retention::plain_directory(&registry) {
            return Err(io::Error::other("the log registry is not an ordinary directory"));
        }
        let directory = retention::Directory::of(dir)?;
        let registration = retention::registry_guard(dir)?;
        let mut selected = None;
        for attempt in 0..16 {
            let stem = if attempt == 0 { source.to_string() } else { format!("{source}-{}-{}", run_id(), SERIAL.fetch_add(1, Ordering::Relaxed)) };
            let receipt = registry.join(format!("{stem}.owned"));
            if receipt.exists() && !retention::receipt_matches(&receipt, source) {
                continue;
            }
            let path = registry.join(format!("{stem}.lease"));
            if path.exists() && !retention::plain_file(&path) {
                continue;
            }
            let lease = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&path)?;
            if lease.try_lock().is_ok()
                && (0..SEGMENTS).all(|n| {
                    let file = segment(dir, &stem, n);
                    retention::available(&file) && (n < 2 || !file.exists() || owned(&file, source))
                })
            {
                selected = Some((stem, lease));
                break;
            }
        }
        let (stem, lease) = selected.ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "all log names are in use"))?;
        let receipt = registry.join(format!("{stem}.owned"));
        if receipt.exists() && !retention::plain_file(&receipt) {
            return Err(io::Error::other("the log receipt is not an ordinary file"));
        }
        fs::write(&receipt, format!("{HEADER}source={source}\n"))?;
        let tick = monotonic_ms();
        let mut journal = Self {
            dir: dir.into(),
            directory,
            stem,
            source: source.into(),
            _lease: lease,
            file: None,
            bytes: 0,
            limits,
            errors: HashMap::new(),
            events: VecDeque::new(),
            last_summary: tick,
            last_snapshot: 0,
            error_snapshot: false,
            last_cleanup: tick,
            failures: 0,
            cleanup_failures: 0,
            truncated: 0,
            sequence: 0,
            operations: HashMap::new(),
            omitted_operations: 0,
        };
        journal.roll()?;
        drop(registration);
        // A failed cleanup never prevents the current source from logging.
        journal.cleanup();
        Ok(journal)
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(format!("{}.log", self.stem))
    }
    pub fn failures(&self) -> u64 {
        self.failures
    }

    /// Logs text after redacting credential arguments. Large individual records
    /// are explicitly truncated; no caller can evade the file bounds with one
    /// extension log call. Critical records are not counted away as duplicates.
    pub fn write(&mut self, kind: Kind, message: &str) -> io::Result<()> {
        let oversized = message.len() > MESSAGE_BYTES;
        let mut message = redact(&bounded(message, MESSAGE_BYTES));
        if oversized || message.len() > MESSAGE_BYTES {
            cut(&mut message, MESSAGE_BYTES);
            message.push_str(" [record truncated]");
            self.truncated += 1;
        }
        let stamp = now();
        if kind == Kind::Error {
            if let Some(error) = self.errors.get_mut(&message) {
                error.count = error.count.saturating_add(1);
                error.last = stamp;
                return self.maintain();
            }
            if self.errors.len() >= ERRORS {
                // Bound error identity memory too; flush counts before eviction.
                self.summaries()?;
            }
            self.errors.insert(message.clone(), Repeated { count: 1, first: stamp.clone(), last: stamp.clone(), emit_single: false });
        }
        let line = self.line(&stamp, &message);
        if matches!(kind, Kind::Event | Kind::Error | Kind::Critical) {
            self.remember(&line);
        }
        if let Err(error) = self.append(&line) {
            // A failed first write must not cause the next occurrence to be
            // counted away as though its text had reached the file.
            if kind == Kind::Error {
                self.errors.remove(&message);
            }
            return Err(error);
        }
        if kind == Kind::Error {
            self.error_snapshot = true;
        }
        if kind == Kind::Critical {
            self.snapshot()?;
        }
        if kind != Kind::Critical {
            self.maintain()?;
        }
        Ok(())
    }

    /// Counts a suppressed repetition of `message` whose initial occurrences
    /// were already written in full, so periodic and checkpoint summaries emit
    /// the exact suppressed count even when only one additional occurrence came.
    pub fn count_repeated(&mut self, message: &str) -> io::Result<()> {
        let oversized = message.len() > MESSAGE_BYTES;
        let mut message = redact(&bounded(message, MESSAGE_BYTES));
        if oversized || message.len() > MESSAGE_BYTES {
            cut(&mut message, MESSAGE_BYTES);
            message.push_str(" [record truncated]");
            self.truncated += 1;
        }
        let stamp = now();
        if let Some(error) = self.errors.get_mut(&message) {
            error.count = error.count.saturating_add(1);
            error.last = stamp;
            error.emit_single = true;
            return self.maintain();
        }
        if self.errors.len() >= ERRORS {
            self.summaries()?;
        }
        self.errors.insert(message, Repeated { count: 1, first: stamp.clone(), last: stamp, emit_single: true });
        self.error_snapshot = true;
        self.maintain()
    }

    /// Aggregate command kinds only, not arguments or a record per unit
    /// action. These counters enter the next bounded snapshot. `None` begins
    /// an operation; an outcome ends it. No filesystem I/O happens here.
    pub fn operation(&mut self, name: &str, outcome: Option<bool>) {
        let name = bounded(&redact(name), 96);
        if !self.operations.contains_key(&name) && self.operations.len() >= 64 {
            self.omitted_operations = self.omitted_operations.saturating_add(1);
            return;
        }
        let operation = self.operations.entry(name).or_default();
        operation.last_ms = monotonic_ms();
        match outcome {
            None => {
                operation.begun = operation.begun.saturating_add(1);
                operation.active = operation.active.saturating_add(1);
            }
            Some(ok) => {
                operation.active = operation.active.saturating_sub(1);
                if ok {
                    operation.succeeded = operation.succeeded.saturating_add(1);
                } else {
                    operation.failed = operation.failed.saturating_add(1);
                }
            }
        }
    }

    fn line(&self, stamp: &str, message: &str) -> String {
        format!(
            "{stamp} run={} pid={} tid={} up_ms={} source={} {message}\n",
            run_id(),
            std::process::id(),
            unsafe { GetCurrentThreadId() },
            monotonic_ms(),
            self.source
        )
    }

    fn remember(&mut self, line: &str) {
        let mut line = line.to_string();
        if line.len() > EVENT_BYTES {
            cut(&mut line, EVENT_BYTES);
            line.push_str(" [snapshot record truncated]\n");
        }
        if self.events.len() == EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(line);
    }

    fn append(&mut self, text: &str) -> io::Result<()> {
        self.append_with(text, |file, bytes| {
            file.write_all(bytes)?;
            file.flush()
        })
    }

    fn append_with(&mut self, text: &str, write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>) -> io::Result<()> {
        let result = (|| {
            if self.file.is_none() || (self.bytes.saturating_add(text.len() as u64) > self.limits.segment && self.bytes > 256) {
                self.roll()?;
            }
            let file = self.file.as_mut().ok_or_else(|| io::Error::other("the log file is closed"))?;
            write(file, text.as_bytes())?;
            self.bytes = self.bytes.saturating_add(text.len() as u64);
            Ok(())
        })();
        if result.is_err() {
            self.failures = self.failures.saturating_add(1);
            // write_all may have appended a prefix before failing. Never use
            // the old length for the next rotation decision. If the actual
            // length cannot be read, close the file so the next write rolls.
            if let Some(file) = &self.file {
                match file.metadata() {
                    Ok(metadata) => self.bytes = metadata.len(),
                    Err(_) => {
                        self.file.take();
                    }
                }
            }
        }
        result
    }

    fn roll(&mut self) -> io::Result<()> {
        self.file.take();
        for index in (1..SEGMENTS).rev() {
            let from = segment(&self.dir, &self.stem, index - 1);
            let to = segment(&self.dir, &self.stem, index);
            if !from.exists() {
                continue;
            }
            if !retention::plain_file(&from) {
                return Err(io::Error::other("a log segment is not an ordinary file"));
            }
            if to.exists() {
                if !owned(&to, &self.source) {
                    return Err(io::Error::other("an older log segment is not owned by Harmony"));
                }
                fs::remove_file(&to)?;
            }
            // Legacy current logs are explicitly adopted only by their own
            // source, kept as a bounded previous segment, never by cleanup.
            if !owned(&from, &self.source) {
                let mut file = File::open(&from)?;
                let len = file.metadata()?.len();
                let header = format!("{HEADER}source={} legacy-tail original_bytes={len}\n", self.source);
                let take = self.limits.segment.saturating_sub(header.len() as u64);
                file.seek(io::SeekFrom::Start(len.saturating_sub(take)))?;
                let mut tail = io::BufReader::new(file.take(take));
                if len > take {
                    // Keep credential names together with their values when
                    // a later collector redacts the retained legacy tail.
                    loop {
                        let bytes = tail.fill_buf()?;
                        if bytes.is_empty() {
                            break;
                        }
                        let newline = bytes.iter().position(|&b| b == b'\n');
                        let used = newline.map_or(bytes.len(), |at| at + 1);
                        tail.consume(used);
                        if newline.is_some() {
                            break;
                        }
                    }
                }
                let mut output = File::create(&to)?;
                output.write_all(header.as_bytes())?;
                io::copy(&mut tail, &mut output)?;
                output.flush()?;
                fs::remove_file(&from)?;
            } else {
                fs::rename(&from, &to)?;
            }
        }
        let path = self.path();
        if path.exists() && !retention::plain_file(&path) {
            return Err(io::Error::other("the current log is not an ordinary file"));
        }
        let mut file = File::create(path)?;
        self.sequence += 1;
        let header = format!("{HEADER}source={} run={} segment={}-{}\n", self.source, run_id(), unix_ms(), self.sequence);
        file.write_all(header.as_bytes())?;
        file.flush()?;
        self.bytes = header.len() as u64;
        self.file = Some(file);
        Ok(())
    }

    fn summaries(&mut self) -> io::Result<()> {
        let mut errors = std::mem::take(&mut self.errors).into_iter();
        self.last_summary = monotonic_ms();
        while let Some((message, error)) = errors.next() {
            if error.count > 1 || error.emit_single {
                let line = self.line(&now(), &format!("repeated error: count={} first={} last={} {message}", error.count, error.first, error.last));
                self.remember(&line);
                if let Err(failed) = self.append(&line) {
                    self.errors.insert(message, error);
                    self.errors.extend(errors);
                    return Err(failed);
                }
            }
        }
        Ok(())
    }

    fn cleanup(&mut self) {
        let now = SystemTime::now();
        self.directory.maintain(self.limits, monotonic_ms());
        // Other writers never touch a leased source. This writer alone can
        // safely expire its own archived segments while keeping its current
        // segment and snapshot, even during a long game with no restart.
        for index in (1..SEGMENTS).rev() {
            let path = segment(&self.dir, &self.stem, index);
            if !owned(&path, &self.source) {
                continue;
            }
            let Ok(metadata) = path.metadata() else { continue };
            let expired = now.duration_since(metadata.modified().unwrap_or(now)).unwrap_or_default() > self.limits.retain;
            if expired || self.directory.excess.load(Ordering::Relaxed) > 0 {
                if fs::remove_file(path).is_ok() {
                    self.directory.reclaimed(metadata.len());
                } else {
                    self.cleanup_failures = self.cleanup_failures.saturating_add(1);
                }
            }
        }
    }

    /// Called from safe frames, as well as from ordinary writes. No timer
    /// thread or file write per frame is needed.
    pub fn maintain(&mut self) -> io::Result<()> {
        let tick = monotonic_ms();
        if tick.saturating_sub(self.last_summary) >= SUMMARY.as_millis() as u64 {
            self.summaries()?;
        }
        if tick.saturating_sub(self.last_snapshot) >= SNAPSHOT.as_millis() as u64 || (self.error_snapshot && tick.saturating_sub(self.last_snapshot) >= 1000) {
            self.snapshot()?;
        }
        if tick.saturating_sub(self.last_cleanup) >= SUMMARY.as_millis() as u64 {
            self.last_cleanup = tick;
            self.cleanup();
        }
        Ok(())
    }

    /// Flush repeated counts at a session change, rather than waiting for an
    /// orderly process exit that the game need not provide.
    pub fn checkpoint(&mut self) -> io::Result<()> {
        self.summaries()?;
        self.snapshot()
    }

    fn snapshot(&mut self) -> io::Result<()> {
        self.last_snapshot = monotonic_ms();
        self.error_snapshot = false;
        let result = (|| {
            let path = self.dir.join(format!("{}.snapshot.txt", self.stem));
            if path.exists() && !owned(&path, &self.source) {
                return Err(io::Error::other("the snapshot is not owned by Harmony"));
            }
            // Never truncate a preexisting staging file, even a regular
            // file. Each attempt creates its own name exclusively.
            let temporary = self.dir.join(format!("{}.snapshot-{}-{}.new", self.stem, run_id(), SERIAL.fetch_add(1, Ordering::Relaxed)));
            let mut file = OpenOptions::new().create_new(true).write(true).open(&temporary)?;
            let written = (|| {
                writeln!(file, "{HEADER}source={} run={}", self.source, run_id())?;
                writeln!(
                file,
                "snapshot={} up_ms={} write_failures={} cleanup_failures={} cleanup_skipped_receipts={} truncated_records={} events={} pending_error_identities={}",
                now(),
                monotonic_ms(),
                self.failures,
                self.cleanup_failures.saturating_add(self.directory.failures.load(Ordering::Relaxed)),
                self.directory.skipped.load(Ordering::Relaxed),
                self.truncated,
                self.events.len(),
                self.errors.len()
            )?;
                writeln!(file, "operation_kinds_omitted={}", self.omitted_operations)?;
                for (name, operation) in &self.operations {
                    writeln!(
                        file,
                        "operation={name} begun={} succeeded={} failed={} unmatched_begins={} last_up_ms={}",
                        operation.begun, operation.succeeded, operation.failed, operation.active, operation.last_ms
                    )?;
                }
                for line in &self.events {
                    file.write_all(line.as_bytes())?;
                }
                for (message, error) in &self.errors {
                    writeln!(file, "pending error: count={} first={} last={} {}", error.count, error.first, error.last, bounded(message, EVENT_BYTES))?;
                }
                file.flush()?;
                // Keep the handle open through rename: an exclusive probe must
                // not mistake the close/rename gap for abandoned staging data.
                fs::rename(&temporary, path)?;
                drop(file);
                Ok(())
            })();
            if written.is_err() {
                // Only this attempt's newly created file can be removed.
                let _ = fs::remove_file(&temporary);
            }
            written
        })();
        if result.is_err() {
            self.failures = self.failures.saturating_add(1);
        }
        result
    }
}

fn cut(text: &mut String, maximum: usize) {
    let mut end = maximum.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}
fn bounded(text: &str, maximum: usize) -> String {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}
fn valid_stem(text: &str) -> bool {
    !text.is_empty() && text.len() <= 160 && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
fn segment(dir: &Path, stem: &str, index: usize) -> PathBuf {
    dir.join(match index {
        0 => format!("{stem}.log"),
        1 => format!("{stem}.prev.log"),
        n => format!("{stem}.{n}.log"),
    })
}
fn owned(path: &Path, source: &str) -> bool {
    if !retention::plain_file(path) {
        return false;
    }
    let mut bytes = [0u8; 256];
    let Ok(mut file) = File::open(path) else { return false };
    let Ok(n) = file.read(&mut bytes) else { return false };
    let prefix = format!("{HEADER}source={source}");
    bytes[..n].starts_with(prefix.as_bytes()) && bytes.get(prefix.len()).is_some_and(|c| matches!(c, b' ' | b'\n'))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("harmony-owned-logs-{}-{name}-{}", std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn limits() -> Limits {
        Limits { segment: 1024, total: 4096, retain: Duration::from_secs(60) }
    }

    #[test]
    fn rotation_keeps_four_bounded_readable_segments_and_adopts_only_its_own_legacy_log() {
        let s = Scratch::new("rotation");
        fs::write(s.0.join("core.log"), format!("{}\nretained-line\n", "old".repeat(1000))).unwrap();
        let mut log = Journal::open_with(&s.0, "core", limits()).unwrap();
        assert_eq!(fs::read_to_string(s.0.join("core.prev.log")).unwrap().lines().nth(1), Some("retained-line"));
        for n in 0..20 {
            log.write(Kind::Note, &format!("{n} {}", "x".repeat(300))).unwrap();
        }
        for n in 0..SEGMENTS {
            let file = segment(&s.0, "core", n);
            assert!(file.exists());
            assert!(file.metadata().unwrap().len() <= 1024);
        }
        assert!(!s.0.join("core.4.log").exists());
        let current = fs::read_to_string(log.path()).unwrap();
        assert!(current.contains("19 "));
        assert!(current.contains(&format!("run={}", run_id())));
    }

    #[test]
    fn a_second_writer_never_rotates_or_cleans_the_first_writer_s_files() {
        let s = Scratch::new("concurrent");
        let mut first = Journal::open_with(&s.0, "core", limits()).unwrap();
        first.write(Kind::Note, "first process still here").unwrap();
        let second = Journal::open_with(&s.0, "core", limits()).unwrap();
        assert_ne!(first.path(), second.path());
        retention::clean(&s.0, Limits { total: 1, ..limits() }, SystemTime::now() + Duration::from_secs(100)).unwrap();
        assert!(fs::read_to_string(first.path()).unwrap().contains("first process still here"));
        assert!(second.path().exists());
    }

    #[cfg(windows)]
    #[test]
    fn an_open_legacy_log_without_a_lease_is_not_rotated() {
        let s = Scratch::new("legacy-writer");
        fs::write(s.0.join("core.log"), "legacy writer still running").unwrap();
        let held = OpenOptions::new().read(true).write(true).open(s.0.join("core.log")).unwrap();
        let new = Journal::open_with(&s.0, "core", limits()).unwrap();
        assert_ne!(new.path(), s.0.join("core.log"));
        assert_eq!(fs::read_to_string(s.0.join("core.log")).unwrap(), "legacy writer still running");
        drop(held);
    }

    #[test]
    fn a_live_writer_expires_its_archives_but_keeps_current_files_under_space_pressure() {
        let s = Scratch::new("live-archives");
        let mut log = Journal::open_with(&s.0, "core", limits()).unwrap();
        log.write(Kind::Note, "current").unwrap();
        let older = segment(&s.0, "core", 1);
        fs::copy(log.path(), &older).unwrap();
        let file = OpenOptions::new().write(true).open(&older).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(100))).unwrap();
        drop(file);
        log.cleanup();
        assert!(!older.exists());
        assert!(log.path().exists());
        for n in 0..15 {
            log.write(Kind::Note, &format!("{n} {}", "x".repeat(300))).unwrap();
        }
        log.limits.total = 1;
        log.directory.next.store(0, Ordering::Relaxed);
        log.cleanup();
        assert!(log.path().exists());
        assert!((1..SEGMENTS).all(|n| !segment(&s.0, "core", n).exists()));
    }

    #[test]
    fn an_unrecognized_registry_receipt_is_preserved_and_uses_another_source_stem() {
        let s = Scratch::new("unrecognized");
        fs::create_dir_all(s.0.join(".harmony-logs")).unwrap();
        fs::write(s.0.join(".harmony-logs/core.owned"), "not a Harmony receipt").unwrap();
        fs::write(s.0.join("core.log"), "keep this file").unwrap();
        let log = Journal::open_with(&s.0, "core", limits()).unwrap();
        assert_ne!(log.path(), s.0.join("core.log"));
        assert_eq!(fs::read_to_string(s.0.join("core.log")).unwrap(), "keep this file");
        assert_eq!(fs::read_to_string(s.0.join(".harmony-logs/core.owned")).unwrap(), "not a Harmony receipt");
    }

    #[test]
    fn cleanup_only_removes_registered_owned_inactive_files_by_age_and_total_size() {
        let s = Scratch::new("cleanup");
        let mut log = Journal::open_with(&s.0, "core", limits()).unwrap();
        log.write(Kind::Event, "a save began").unwrap();
        log.checkpoint().unwrap();
        let current = log.path();
        drop(log);
        fs::write(s.0.join("error.log"), "game error").unwrap();
        fs::write(s.0.join("minidump.dmp"), "dump").unwrap();
        fs::write(s.0.join("core.prev.log"), "unmarked file").unwrap();
        retention::clean(&s.0, Limits { total: 1, ..limits() }, SystemTime::now()).unwrap();
        assert!(!current.exists());
        assert_eq!(fs::read_to_string(s.0.join("error.log")).unwrap(), "game error");
        assert!(s.0.join("minidump.dmp").exists());
        assert_eq!(fs::read_to_string(s.0.join("core.prev.log")).unwrap(), "unmarked file");
        let log = Journal::open_with(&s.0, "other", limits()).unwrap();
        let path = log.path();
        drop(log);
        retention::clean(&s.0, limits(), SystemTime::now() + Duration::from_secs(100)).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn malformed_receipts_do_not_block_valid_cleanup_and_are_counted_and_preserved() {
        let s = Scratch::new("malformed-receipts");
        let log = Journal::open_with(&s.0, "expired", limits()).unwrap();
        let path = log.path();
        drop(log);
        let malformed = s.0.join(".harmony-logs/malformed.owned");
        fs::write(&malformed, [0xff, 0xfe]).unwrap();
        let unreadable = s.0.join(".harmony-logs/not-a-file.owned");
        fs::create_dir(&unreadable).unwrap();
        let outcome = retention::clean(&s.0, limits(), SystemTime::now() + Duration::from_secs(100)).unwrap();
        assert!(!path.exists());
        assert_eq!(outcome.skipped, 2);
        assert_eq!(fs::read(malformed).unwrap(), [0xff, 0xfe]);
        assert!(unreadable.is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn an_unreadable_regular_receipt_is_skipped_without_stopping_other_sources() {
        use std::os::windows::fs::OpenOptionsExt;
        let s = Scratch::new("unreadable-receipt");
        let log = Journal::open_with(&s.0, "expired", limits()).unwrap();
        let path = log.path();
        drop(log);
        let receipt = s.0.join(".harmony-logs/locked.owned");
        fs::write(&receipt, format!("{HEADER}source=locked\n")).unwrap();
        let held = OpenOptions::new().read(true).share_mode(0).open(&receipt).unwrap();
        let outcome = retention::clean(&s.0, limits(), SystemTime::now() + Duration::from_secs(100)).unwrap();
        assert_eq!(outcome.skipped, 1);
        assert!(!path.exists());
        drop(held);
        assert_eq!(fs::read_to_string(receipt).unwrap(), format!("{HEADER}source=locked\n"));
    }

    #[test]
    fn idle_registration_metadata_is_reclaimed_and_new_writers_share_the_registry_lock() {
        let s = Scratch::new("metadata-reclamation");
        for n in 0..40 {
            let log = Journal::open_with(&s.0, &format!("old-{n}"), limits()).unwrap();
            drop(log);
        }
        let active = Journal::open_with(&s.0, "active", limits()).unwrap();
        let guard = retention::registry_guard(&s.0).unwrap();
        assert!(Journal::open_with(&s.0, "fresh", limits()).is_err());
        drop(guard);
        retention::clean(&s.0, limits(), SystemTime::now() + Duration::from_secs(100)).unwrap();
        let names: Vec<_> = fs::read_dir(s.0.join(".harmony-logs")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 3, "only the directory lease and active source's two files remain");
        assert!(active.path().exists());
        drop(active);
        retention::clean(&s.0, limits(), SystemTime::now() + Duration::from_secs(100)).unwrap();
        assert_eq!(fs::read_dir(s.0.join(".harmony-logs")).unwrap().count(), 1);
        let reopened = Journal::open_with(&s.0, "active", limits()).unwrap();
        let another = Journal::open_with(&s.0, "active", limits()).unwrap();
        assert_ne!(reopened.path(), another.path());
    }

    #[test]
    fn directory_maintenance_runs_once_for_many_sources_and_independently_for_other_directories() {
        let s = Scratch::new("shared-schedule");
        let mut logs: Vec<_> = (0..40).map(|n| Journal::open_with(&s.0, &format!("source-{n}"), limits()).unwrap()).collect();
        let directory = logs[0].directory.clone();
        assert!(logs.iter().all(|log| Arc::ptr_eq(&log.directory, &directory)));
        assert_eq!(directory.passes.load(Ordering::Relaxed), 1);
        for log in &mut logs {
            log.cleanup();
        }
        assert_eq!(directory.passes.load(Ordering::Relaxed), 1);
        let next = directory.next.load(Ordering::Relaxed);
        for log in &logs {
            log.directory.maintain(limits(), next);
        }
        assert_eq!(directory.passes.load(Ordering::Relaxed), 2);
        let other = Scratch::new("independent-schedule");
        let log = Journal::open_with(&other.0, "core", limits()).unwrap();
        assert!(!Arc::ptr_eq(&directory, &log.directory));
        assert_eq!(log.directory.passes.load(Ordering::Relaxed), 1);
    }

    #[cfg(windows)]
    #[test]
    fn abandoned_unique_snapshots_are_reclaimed_but_open_snapshot_writes_are_protected() {
        let s = Scratch::new("snapshot-staging-cleanup");
        let log = Journal::open_with(&s.0, "core", limits()).unwrap();
        let path = s.0.join("core.snapshot-synthetic-run-0.new");
        let mut held = OpenOptions::new().create_new(true).write(true).open(&path).unwrap();
        writeln!(held, "{HEADER}source=core run=synthetic-run").unwrap();
        held.flush().unwrap();
        retention::clean(&s.0, Limits { total: 1, ..limits() }, SystemTime::now()).unwrap();
        assert!(path.exists());
        drop(held);
        retention::clean(&s.0, Limits { total: 1, ..limits() }, SystemTime::now()).unwrap();
        assert!(!path.exists());
        assert!(log.path().exists());
    }

    #[test]
    fn errors_are_counted_with_first_and_last_times_but_crashes_are_never_deduplicated() {
        let s = Scratch::new("repeat");
        let mut log = Journal::open_with(&s.0, "core", Limits::default()).unwrap();
        for _ in 0..10 {
            log.write(Kind::Error, "error in on_frame: bad").unwrap();
        }
        for _ in 0..2 {
            log.write(Kind::Critical, "crash: exact fault").unwrap();
        }
        log.checkpoint().unwrap();
        let text = fs::read_to_string(log.path()).unwrap();
        assert_eq!(text.matches("crash: exact fault").count(), 2);
        assert_eq!(text.matches("error in on_frame: bad").count(), 2);
        assert!(text.contains("count=10 first="));
        assert!(text.contains(" last="));
    }

    #[test]
    fn repeated_errors_stay_small_and_operation_counters_do_not_write_per_command() {
        let s = Scratch::new("volume");
        let mut log = Journal::open_with(&s.0, "core", Limits::default()).unwrap();
        log.last_summary = u64::MAX;
        log.last_snapshot = u64::MAX;
        log.last_cleanup = u64::MAX;
        for _ in 0..10_000 {
            log.write(Kind::Error, "error in on_frame: repeated").unwrap();
        }
        assert!(fs::metadata(log.path()).unwrap().len() < 4096);
        let before = fs::metadata(log.path()).unwrap().len();
        for _ in 0..10_000 {
            log.operation("moveunit", None);
            log.operation("moveunit", Some(true));
        }
        assert_eq!(fs::metadata(log.path()).unwrap().len(), before);
        assert_eq!(log.operations.len(), 1);
        log.checkpoint().unwrap();
        let text = fs::read_to_string(log.path()).unwrap();
        assert!(text.contains("count=10000"));
        assert!(text.len() < 4096);
        let snapshot = fs::read_to_string(s.0.join("core.snapshot.txt")).unwrap();
        assert!(snapshot.contains("operation=moveunit begun=10000 succeeded=10000 failed=0 unmatched_begins=0"));
        for n in 0..1000 {
            log.operation(&format!("kind-{n}"), None);
        }
        assert_eq!(log.operations.len(), 64);
        assert!(log.omitted_operations > 0);
    }

    #[test]
    fn failed_first_errors_are_retryable_and_failed_summaries_keep_their_counts() {
        let s = Scratch::new("failed-records");
        let mut log = Journal::open_with(&s.0, "core", Limits::default()).unwrap();
        log.file = Some(File::open(log.path()).unwrap());
        assert!(log.write(Kind::Error, "error: first write failed").is_err());
        assert!(log.errors.is_empty());
        log.roll().unwrap();
        log.write(Kind::Error, "error: first write failed").unwrap();
        assert!(fs::read_to_string(log.path()).unwrap().contains("error: first write failed"));
        log.write(Kind::Error, "error: first write failed").unwrap();
        log.file = Some(File::open(log.path()).unwrap());
        assert!(log.checkpoint().is_err());
        assert_eq!(log.errors.get("error: first write failed").unwrap().count, 2);
        log.roll().unwrap();
        log.checkpoint().unwrap();
        assert!(fs::read_to_string(log.path()).unwrap().contains("count=2"));
    }

    #[test]
    fn a_partial_write_failure_counts_the_bytes_that_reached_disk_before_recovery() {
        let s = Scratch::new("partial-write");
        let mut log = Journal::open_with(&s.0, "core", limits()).unwrap();
        assert!(log
            .append_with(&"p".repeat(700), |file, bytes| {
                file.write_all(&bytes[..600])?;
                Err(io::Error::other("synthetic failure after a partial write"))
            })
            .is_err());
        assert_eq!(log.bytes, log.path().metadata().unwrap().len());
        log.append(&"r".repeat(400)).unwrap();
        assert!(segment(&s.0, "core", 1).exists());
        for n in 0..2 {
            assert!(segment(&s.0, "core", n).metadata().unwrap().len() <= limits().segment);
        }
        assert_eq!(log.bytes, log.path().metadata().unwrap().len());
    }

    #[test]
    fn snapshots_never_overwrite_unknown_or_another_runs_staging_file() {
        let s = Scratch::new("snapshot-ownership");
        let mut log = Journal::open_with(&s.0, "core", limits()).unwrap();
        let unknown = s.0.join("core.snapshot.new");
        let other = s.0.join("core.snapshot-other-run-0.new");
        fs::write(&unknown, "unrecognized staging contents").unwrap();
        fs::write(&other, format!("{HEADER}source=core run=other-run\nother context")).unwrap();
        let before = fs::read(&other).unwrap();
        log.checkpoint().unwrap();
        assert_eq!(fs::read_to_string(unknown).unwrap(), "unrecognized staging contents");
        assert_eq!(fs::read(other).unwrap(), before);
        assert!(owned(&s.0.join("core.snapshot.txt"), "core"));
    }

    #[test]
    fn successful_results_are_not_mistaken_for_repeated_errors() {
        assert_eq!(kind("batch make_state: 8 succeeded, 1 skipped, 0 failed"), Kind::Note);
        assert_eq!(kind("error in on_frame: bad"), Kind::Error);
        assert_eq!(kind("extension example: error in on_frame: bad"), Kind::Error);
        assert_eq!(kind("crash: bad"), Kind::Critical);
    }

    #[test]
    fn extension_names_are_safe_even_when_a_manifest_id_is_not_a_filename() {
        assert_eq!(extension_source("regional-combat"), "extension-regional-combat");
        let unusual = extension_source("../core\\unicode\n");
        assert!(valid_stem(&unusual));
        assert!(!unusual.contains(".."));
        assert_ne!(extension_source("a_b"), extension_source("a/b"));
        assert!(valid_stem(&extension_source(&"x".repeat(1000))));
    }

    #[test]
    fn snapshots_and_error_memory_are_bounded_and_failures_are_reported() {
        let s = Scratch::new("snapshot");
        let mut log = Journal::open_with(&s.0, "core", Limits::default()).unwrap();
        for n in 0..1000 {
            log.write(Kind::Event, &format!("event {n} {}", "x".repeat(4000))).unwrap();
        }
        log.checkpoint().unwrap();
        let text = fs::read_to_string(s.0.join("core.snapshot.txt")).unwrap();
        assert!(text.len() < (EVENTS + 1) * (EVENT_BYTES + 100));
        assert!(!text.contains(" event 0 "));
        assert!(text.contains("event 999 "));
        log.file = Some(File::open(log.path()).unwrap());
        assert!(log.write(Kind::Critical, "crash: write must fail").is_err());
        assert!(log.failures() > 0);
        assert!(!valid_stem("../error"));
    }

    #[test]
    fn source_marks_ignore_preexisting_archives_follow_fallback_stems_and_adopt_legacy_tails() {
        let s = Scratch::new("reader-marks");
        let ext = extension_source("qa_mod");
        // Write an earlier run's archived segment while current `.log` is absent.
        {
            let mut old = Journal::open_with(&s.0, &ext, limits()).unwrap();
            old.write(Kind::Event, "stale from earlier run").unwrap();
        }
        fs::rename(s.0.join(format!("{ext}.log")), s.0.join(format!("{ext}.prev.log"))).unwrap();
        let lazy_mark = SourceMark::of(&s.0, &ext);
        assert!(lazy_mark.messages_since(&s.0, &ext).is_empty(), "an uncreated current log must not replay preexisting archives");
        {
            let mut fresh = Journal::open_with(&s.0, &ext, limits()).unwrap();
            fresh.write(Kind::Event, "fresh after lazy open").unwrap();
            fresh.count_repeated("error in on_day: boom").unwrap();
            fresh.checkpoint().unwrap();
        }
        let messages = lazy_mark.messages_since(&s.0, &ext);
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert_eq!(messages[0], "fresh after lazy open");
        assert!(messages[1].starts_with("repeated error: count=1 ") && messages[1].ends_with("error in on_day: boom"), "{messages:?}");

        // Legacy tail adoption and fallback stem when primary `.log` is held open.
        fs::write(s.0.join("core.log"), "legacy before mark\n").unwrap();
        let core_mark = SourceMark::of(&s.0, CORE);
        let emergency_mark = SourceMark::of(&s.0, CORE_EMERGENCY);
        {
            let mut adopted = Journal::open_with(&s.0, CORE, limits()).unwrap();
            adopted.write(Kind::Event, "after legacy adoption").unwrap();
            let _held = OpenOptions::new().read(true).write(true).open(s.0.join("core.log")).unwrap();
            // Simulate a second writer forced onto a fallback stem.
            let mut fallback = Journal::open_with(&s.0, CORE, limits()).unwrap();
            fallback.write(Kind::Critical, "crash: 0xc0000005 at eu4+0x1234").unwrap();
        }
        let core_msgs = core_mark.messages_since(&s.0, CORE);
        assert_eq!(core_msgs, ["after legacy adoption", "crash: 0xc0000005 at eu4+0x1234"]);
        let troubles = troubles_since(&s.0, &core_mark, &emergency_mark);
        assert_eq!(troubles.len(), 1);
        assert!(troubles[0].contains("crash: 0xc0000005"));

        // Startup failure reason finds proxy and emergency logs too.
        let before_proxy = SystemTime::now() - Duration::from_secs(2);
        let mut proxy = Journal::open_with(&s.0, PROXY, limits()).unwrap();
        proxy.write(Kind::Critical, "LoadLibraryExW of the named core failed").unwrap();
        drop(proxy);
        let s_proxy = Scratch::new("proxy-only");
        let mut only_proxy = Journal::open_with(&s_proxy.0, PROXY, limits()).unwrap();
        only_proxy.write(Kind::Critical, "cannot load named core").unwrap();
        drop(only_proxy);
        assert_eq!(startup_failure_reason(&s_proxy.0, before_proxy).as_deref(), Some("proxy: cannot load named core"));
    }
}
