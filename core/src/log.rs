//! Core logging, including the bounded buffer before the user folder is
//! known. File policy, records and credential redaction are shared with the
//! proxy and extensions through `eu4_harmony_logging`.
use eu4_harmony_logging::{Journal, Kind};
use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

const PENDING_BYTES: usize = 1 << 20;
struct Sink {
    file: Option<Journal>,
    pending: VecDeque<(Kind, String)>,
    bytes: usize,
    dropped: u64,
}
static SINK: Mutex<Sink> = Mutex::new(Sink { file: None, pending: VecDeque::new(), bytes: 0, dropped: 0 });
// This independent, preopened writer is only used if the normal log cannot
// be locked/written. It has its own bounds, lease and error counters.
static EMERGENCY: Mutex<Option<Journal>> = Mutex::new(None);
static LOCK_TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static EMERGENCY_FAILURES: AtomicU64 = AtomicU64::new(0);
static NEXT: AtomicU64 = AtomicU64::new(0);
static MISSED_OPERATIONS: AtomicU64 = AtomicU64::new(0);

/// Bounded aggregate metadata for commands the framework posts. No arguments
/// or per-command file records are kept. A busy logger is never waited on.
pub(crate) struct Operation<'a> {
    name: &'a str,
    recorded: bool,
    succeeded: bool,
}
impl<'a> Operation<'a> {
    pub fn begin(name: &'a str) -> Self {
        let mut recorded = false;
        if let Ok(mut sink) = SINK.try_lock() {
            if let Some(file) = sink.file.as_mut() {
                file.operation(name, None);
                recorded = true;
            }
        }
        if !recorded {
            MISSED_OPERATIONS.fetch_add(1, Ordering::Relaxed);
        }
        Self { name, recorded, succeeded: false }
    }
    pub fn succeeded(&mut self) {
        self.succeeded = true;
    }
}
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        if !self.recorded {
            return;
        }
        if let Ok(mut sink) = SINK.try_lock() {
            if let Some(file) = sink.file.as_mut() {
                file.operation(self.name, Some(self.succeeded));
                return;
            }
        }
        MISSED_OPERATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

/// The startup report, which is overwritten rather than appended, still
/// keeps the preceding report. Streaming logs use the shared segment policy.
pub fn rotate(path: &Path) {
    if path.exists() {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("log");
        let _ = fs::rename(path, path.with_extension(format!("prev.{ext}")));
    }
}

pub fn open(path: &Path) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("the log has no folder"))?;
    let mut file = Journal::open(dir, "core")?;
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    for (kind, message) in sink.pending.drain(..) {
        let _ = file.write(kind, &message);
    }
    sink.bytes = 0;
    if sink.dropped != 0 {
        let _ = file.write(Kind::Event, &format!("startup log buffer: {} older records dropped", sink.dropped));
    }
    sink.file = Some(file);
    drop(sink);
    *EMERGENCY.lock().unwrap_or_else(|e| e.into_inner()) = Journal::open(dir, "core-emergency").ok();
    Ok(())
}

pub fn is_open() -> bool {
    SINK.lock().unwrap_or_else(|e| e.into_inner()).file.is_some()
}
pub fn now() -> String {
    eu4_harmony_logging::now()
}

pub fn write(args: std::fmt::Arguments) {
    let message = args.to_string();
    let mut kind = eu4_harmony_logging::kind(&message);
    if kind == Kind::Critical {
        let _ = write_within(format_args!("{message}"), Duration::from_secs(1));
        return;
    }
    if kind == Kind::Note && ["session ", "extension ", "startup:", "eu4-harmony core", "saves:"].iter().any(|p| message.starts_with(p)) {
        kind = Kind::Event;
    }
    put(&mut SINK.lock().unwrap_or_else(|e| e.into_inner()), kind, message);
}

pub fn event(args: std::fmt::Arguments) {
    put(&mut SINK.lock().unwrap_or_else(|e| e.into_inner()), Kind::Event, args.to_string());
}

/// A failed or timed-out normal write is retried through a separate bounded
/// writer. True means an actual file write and flush succeeded, never merely
/// that bytes were buffered. No crash record goes through error deduplication.
/// Filesystem I/O itself has no guaranteed deadline, as with the old logger.
pub fn write_within(args: std::fmt::Arguments, wait: Duration) -> bool {
    within(&SINK, &EMERGENCY, &args.to_string(), wait)
}

fn within(normal: &Mutex<Sink>, fallback: &Mutex<Option<Journal>>, message: &str, wait: Duration) -> bool {
    let until = std::time::Instant::now() + wait;
    loop {
        match normal.try_lock() {
            Ok(mut sink) => {
                if critical(&mut sink, message) {
                    return true;
                }
                break;
            }
            Err(std::sync::TryLockError::Poisoned(e)) => {
                if critical(&mut e.into_inner(), message) {
                    return true;
                }
                break;
            }
            Err(std::sync::TryLockError::WouldBlock) if std::time::Instant::now() < until => std::thread::sleep(Duration::from_millis(1)),
            Err(std::sync::TryLockError::WouldBlock) => {
                LOCK_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }
    let wrote = fallback.try_lock().ok().and_then(|mut log| log.as_mut().map(|log| log.write(Kind::Critical, message).is_ok())).unwrap_or(false);
    if !wrote {
        EMERGENCY_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
    wrote
}

fn critical(sink: &mut Sink, message: &str) -> bool {
    if let Some(log) = sink.file.as_mut() {
        log.write(Kind::Critical, message).is_ok()
    } else {
        // Startup may not know the user folder yet. Retain the attempt, but
        // never report buffered bytes as a persisted crash record.
        put(sink, Kind::Critical, message.to_string());
        false
    }
}

fn put(sink: &mut Sink, kind: Kind, mut message: String) -> bool {
    if let Some(file) = sink.file.as_mut() {
        return file.write(kind, &message).is_ok();
    }
    // The bounded startup buffer records its original time and thread, which
    // precede the eventual file's open time. Redact before retaining text.
    message = format!("buffered_at={} tid={} {}", now(), crate::thread_id(), eu4_harmony_logging::redact(&message));
    if message.len() > PENDING_BYTES {
        sink.dropped += 1;
        return false;
    }
    while sink.bytes + message.len() > PENDING_BYTES {
        if let Some((_, old)) = sink.pending.pop_front() {
            sink.bytes -= old.len();
            sink.dropped += 1;
        } else {
            break;
        }
    }
    sink.bytes += message.len();
    sink.pending.push_back((kind, message));
    true
}

/// Cheap gate at safe frames. Maintenance does not wait for the log lock and
/// writes snapshots only at their interval, including while the game is paused.
pub fn poll() {
    let now = eu4_harmony_logging::monotonic_ms();
    let next = NEXT.load(Ordering::Relaxed);
    if now < next || NEXT.compare_exchange(next, now.saturating_add(1000), Ordering::Relaxed, Ordering::Relaxed).is_err() {
        return;
    }
    if let Ok(mut sink) = SINK.try_lock() {
        if let Some(file) = sink.file.as_mut() {
            let _ = file.maintain();
        }
    }
    crate::lua::maintain_logs(false);
}

pub fn checkpoint() {
    if let Ok(mut sink) = SINK.try_lock() {
        if let Some(file) = sink.file.as_mut() {
            let _ = file.write(
                Kind::Event,
                &format!(
                    "logging status: lock_timeouts={} emergency_failures={} missed_operation_updates={}",
                    LOCK_TIMEOUTS.load(Ordering::Relaxed),
                    EMERGENCY_FAILURES.load(Ordering::Relaxed),
                    MISSED_OPERATIONS.load(Ordering::Relaxed)
                ),
            );
            let _ = file.checkpoint();
        }
    }
    crate::lua::maintain_logs(true);
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(format_args!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_crash_write_does_not_claim_buffering_or_a_closed_file_was_persisted() {
        let mut sink = Sink { file: None, pending: VecDeque::new(), bytes: 0, dropped: 0 };
        assert!(!critical(&mut sink, "crash: no file"));
        assert_eq!(sink.pending.len(), 1, "the startup attempt is buffered, not reported as persisted");
    }
    #[test]
    fn a_held_normal_log_uses_the_separate_writer_and_a_held_fallback_reports_failure() {
        let dir = std::env::temp_dir().join(format!("harmony-crash-fallback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let normal = Mutex::new(Sink { file: None, pending: VecDeque::new(), bytes: 0, dropped: 0 });
        let fallback = Mutex::new(Some(Journal::open(&dir, "core-emergency").unwrap()));
        let held = normal.lock().unwrap();
        assert!(within(&normal, &fallback, "crash: normal log held", Duration::ZERO));
        let emergency = fallback.lock().unwrap();
        let path = emergency.as_ref().unwrap().path();
        assert!(std::fs::read_to_string(path).unwrap().contains("crash: normal log held"));
        assert!(!within(&normal, &fallback, "crash: both held", Duration::ZERO));
        drop(emergency);
        drop(held);
        drop(fallback);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_startup_buffer_is_bounded_and_redacts_before_retaining() {
        let mut sink = Sink { file: None, pending: VecDeque::new(), bytes: 0, dropped: 0 };
        for _ in 0..1000 {
            put(&mut sink, Kind::Note, format!("-auth_token=private {}", "x".repeat(2000)));
        }
        assert!(sink.bytes <= PENDING_BYTES);
        assert!(sink.dropped > 0);
        assert!(sink.pending.iter().all(|(_, line)| !line.contains("private")));
    }
}
