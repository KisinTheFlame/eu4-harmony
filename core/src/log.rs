//! Framework log. Lines written before the log file is opened are buffered and
//! flushed on open: the file lives in the user directory, which is only known
//! once the game has started its file system.
use std::{
    fs::{self, File},
    io::Write as _,
    path::Path,
    sync::Mutex,
};
use windows_sys::Win32::System::{SystemInformation::GetLocalTime, Threading::GetCurrentThreadId};

struct Sink {
    file: Option<File>,
    pending: Vec<String>,
}

static SINK: Mutex<Sink> = Mutex::new(Sink { file: None, pending: Vec::new() });

/// Moves an existing `name.ext` to `name.prev.ext`, keeping one earlier run.
pub fn rotate(path: &Path) {
    if path.exists() {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("log");
        let _ = fs::rename(path, path.with_extension(format!("prev.{ext}")));
    }
}

pub fn open(path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    rotate(path);
    let mut file = File::create(path)?;
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    for line in sink.pending.drain(..) {
        let _ = file.write_all(line.as_bytes());
    }
    sink.file = Some(file);
    Ok(())
}

pub fn is_open() -> bool {
    SINK.lock().unwrap_or_else(|e| e.into_inner()).file.is_some()
}

/// Local wall-clock time as `hh:mm:ss.mmm`.
pub fn now() -> String {
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    format!("{:02}:{:02}:{:02}.{:03}", t.wHour, t.wMinute, t.wSecond, t.wMilliseconds)
}

pub fn write(args: std::fmt::Arguments) {
    let line = format!("{} tid={} {}\n", now(), unsafe { GetCurrentThreadId() }, args);
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
