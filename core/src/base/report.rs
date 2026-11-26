//! The startup report, `report.txt` next to the logs: what the core found and
//! loaded, and how long each extension's handlers have taken so far. It is
//! rewritten at startup, at every session start and every few minutes, since
//! the end of the game process cannot be observed safely.
use super::log;
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

struct Report {
    path: Option<PathBuf>,
    lines: Vec<String>,
}

struct SectionProviders {
    timing: fn() -> Vec<String>,
    speed_checks: fn() -> Vec<String>,
}

static REPORT: Mutex<Report> = Mutex::new(Report { path: None, lines: Vec::new() });
static PROVIDERS: OnceLock<SectionProviders> = OnceLock::new();

pub(crate) fn set_providers(timing: fn() -> Vec<String>, speed_checks: fn() -> Vec<String>) {
    let _ = PROVIDERS.set(SectionProviders { timing, speed_checks });
}

pub fn open(path: PathBuf) {
    log::rotate(&path);
    REPORT.lock().unwrap_or_else(|e| e.into_inner()).path = Some(path);
}

pub fn line(text: impl Into<String>) {
    REPORT.lock().unwrap_or_else(|e| e.into_inner()).lines.push(text.into());
}

/// Writes the report with the current extension timing.
pub fn write() {
    let timing = PROVIDERS.get().map_or_else(Vec::new, |p| (p.timing)());
    let report = REPORT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(path) = &report.path else { return };
    let mut text = report.lines.join("\n");
    text.push_str(&format!("\n\nExtension handlers (as of {}):\n", log::now()));
    if timing.is_empty() {
        text.push_str("  none loaded\n");
    }
    for line in timing {
        text.push_str(&format!("  {line}\n"));
    }
    let checks = PROVIDERS.get().map_or_else(Vec::new, |p| (p.speed_checks)());
    if !checks.is_empty() {
        text.push_str(&format!("\nSpeed-ups checked against the game (as of {}):\n", log::now()));
        for line in checks {
            text.push_str(&format!("  {line}\n"));
        }
    }
    if let Err(e) = fs::write(path, text) {
        log!("report: {e}");
    }
}
