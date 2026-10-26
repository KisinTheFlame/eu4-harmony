//! The startup report, `report.txt` next to the logs: what the core found and
//! loaded, and how long each extension's callbacks have taken so far. It is
//! rewritten at startup, at every session start and every few minutes, since
//! the end of the game process cannot be observed safely.
use std::{fs, path::PathBuf, sync::Mutex};

struct Report {
    path: Option<PathBuf>,
    lines: Vec<String>,
}

static REPORT: Mutex<Report> = Mutex::new(Report { path: None, lines: Vec::new() });

pub fn open(path: PathBuf) {
    crate::log::rotate(&path);
    REPORT.lock().unwrap_or_else(|e| e.into_inner()).path = Some(path);
}

pub fn line(text: impl Into<String>) {
    REPORT.lock().unwrap_or_else(|e| e.into_inner()).lines.push(text.into());
}

/// Writes the report with the current extension timing.
pub fn write() {
    let timing = crate::ext::timing_lines();
    let report = REPORT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(path) = &report.path else { return };
    let mut text = report.lines.join("\n");
    text.push_str(&format!("\n\nExtension callbacks (as of {}):\n", crate::log::now()));
    if timing.is_empty() {
        text.push_str("  none loaded\n");
    }
    for line in timing {
        text.push_str(&format!("  {line}\n"));
    }
    if let Err(e) = fs::write(path, text) {
        crate::log!("report: {e}");
    }
}
