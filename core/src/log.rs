//! Framework log. Lines written before the log file is opened are buffered and
//! flushed on open, so bootstrap diagnostics are never lost.
use std::{
    fs::{self, File},
    io::Write as _,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

struct Sink {
    file: Option<File>,
    pending: Vec<String>,
}

static SINK: Mutex<Sink> = Mutex::new(Sink { file: None, pending: Vec::new() });

pub fn open(path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let previous = path.with_extension("prev.log");
    if path.exists() {
        let _ = fs::rename(path, &previous);
    }
    let mut file = File::create(path)?;
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    for line in sink.pending.drain(..) {
        let _ = file.write_all(line.as_bytes());
    }
    sink.file = Some(file);
    Ok(())
}

pub fn write(args: std::fmt::Arguments) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs() % 86400;
    let line = format!(
        "{:02}:{:02}:{:02}.{:03} tid={} {}\n",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        now.subsec_millis(),
        unsafe { GetCurrentThreadId() },
        args
    );
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    match sink.file.as_mut() {
        Some(file) => {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
        None => {
            if sink.pending.len() < 10_000 {
                sink.pending.push(line);
            }
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(format_args!($($arg)*)) };
}
