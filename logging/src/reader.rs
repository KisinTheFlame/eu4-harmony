//! Reading bounded Harmony journals, classifying log-folder files, following
//! incremental marks across rotations and fallback stems, and extracting
//! startup or crash failure evidence.
use crate::{owned, segment, HEADER, SEGMENTS};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const CORE: &str = "core";
pub const CORE_EMERGENCY: &str = "core-emergency";
pub const PROXY: &str = "proxy";

/// What kind of file `name` is inside a `eu4-harmony/logs` directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Artifact<'a> {
    /// A readable journal segment: rank `0` is current (`.log`), `1` is
    /// `.prev.log`, `2` is `.2.log`, `3` is `.3.log`.
    Segment { stem: &'a str, rank: u8 },
    /// A bounded diagnostic snapshot (`<stem>.snapshot.txt`).
    Snapshot { stem: &'a str },
    /// An in-progress or abandoned snapshot staging file (`*.snapshot*.new`).
    Staging,
    /// Anything else in the directory (`report.txt`, tool outputs, etc.).
    Other,
}

pub fn classify_file(name: &str) -> Artifact<'_> {
    if name.ends_with(".new") && (name.ends_with(".snapshot.new") || name.contains(".snapshot-")) {
        return Artifact::Staging;
    }
    if let Some(stem) = name.strip_suffix(".snapshot.txt").filter(|s| !s.is_empty()) {
        return Artifact::Snapshot { stem };
    }
    for (suffix, rank) in [(".3.log", 3u8), (".2.log", 2), (".prev.log", 1), (".log", 0)] {
        if let Some(stem) = name.strip_suffix(suffix).filter(|s| !s.is_empty()) {
            return Artifact::Segment { stem, rank };
        }
    }
    Artifact::Other
}

pub fn is_header(line: &str) -> bool {
    line.starts_with(HEADER)
}

/// Extracts the message written by `source` from one journal line, skipping
/// headers and lines from any other source.
pub fn record_message<'a>(line: &'a str, source: &str) -> Option<&'a str> {
    if is_header(line) {
        return None;
    }
    let (_, after) = line.split_once(" source=")?;
    after.strip_prefix(source)?.strip_prefix(' ')
}

/// Extracts `(source, message)` from any structured journal record line.
pub fn split_record(line: &str) -> Option<(&str, &str)> {
    if is_header(line) {
        return None;
    }
    let (_, after) = line.split_once(" source=")?;
    after.split_once(' ')
}

fn read_lossy(path: &Path) -> String {
    fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default()
}

fn legacy_tail_original_bytes(header: &str, source: &str) -> Option<usize> {
    let rest = header.strip_prefix(HEADER)?.strip_prefix("source=")?.strip_prefix(source)?.trim_start();
    let bytes = rest.strip_prefix("legacy-tail original_bytes=")?;
    bytes.split_whitespace().next()?.parse().ok()
}

/// Ordered readable segment paths for `source` in `dir`, oldest to newest.
/// Includes the primary `<source>` stem and any owned fallback stems
/// (`<source>-<run>-<serial>`) created when an earlier segment was held open.
pub fn source_segments(dir: &Path, source: &str) -> Vec<PathBuf> {
    let mut stems = vec![source.to_string()];
    let prefix = format!("{source}-");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Artifact::Segment { stem, .. } = classify_file(&name) {
                if stem.starts_with(&prefix) && owned(&entry.path(), source) && !stems.iter().any(|s| s == stem) {
                    stems.push(stem.to_string());
                }
            }
        }
    }
    if stems.len() > 1 {
        stems[1..].sort_by_key(|stem| {
            let current = segment(dir, stem, 0);
            current.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
        });
    }
    let mut out = Vec::new();
    for stem in &stems {
        for rank in (0..SEGMENTS).rev() {
            let path = segment(dir, stem, rank);
            if path.is_file() && (stem == source || owned(&path, source)) {
                out.push(path);
            }
        }
    }
    out
}

/// Where a single journal source stood at a moment, across its segments and any
/// fallback stems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMark {
    /// Map from a segment's identifying first line to its byte length when marked.
    seen: HashMap<String, usize>,
    /// Length of an unowned legacy `<source>.log` present when marked, so a
    /// subsequent legacy-tail adoption does not replay pre-mark lines.
    unowned_len: Option<usize>,
}

impl SourceMark {
    pub fn of(dir: &Path, source: &str) -> Self {
        let mut seen = HashMap::new();
        let mut unowned_len = None;
        for path in source_segments(dir, source) {
            let text = read_lossy(&path);
            if let Some(first) = text.lines().next() {
                if !is_header(first) && path.file_name().and_then(|n| n.to_str()) == Some(&format!("{source}.log")) {
                    unowned_len = Some(text.len());
                }
                seen.insert(first.to_string(), text.len());
            }
        }
        Self { seen, unowned_len }
    }

    /// Raw lines added to `source` in `dir` since this mark, including headers
    /// on newly rolled segments.
    pub fn lines_since(&self, dir: &Path, source: &str) -> Vec<String> {
        let paths = source_segments(dir, source);
        let texts: Vec<String> = paths.iter().map(|p| read_lossy(p)).collect();
        if self.seen.is_empty() {
            return texts.iter().flat_map(|t| t.lines().map(String::from)).collect();
        }
        let mut last_matched = None;
        for (idx, text) in texts.iter().enumerate() {
            let Some(first) = text.lines().next() else { continue };
            if self.seen.contains_key(first) || (self.unowned_len.is_some() && legacy_tail_original_bytes(first, source).is_some()) {
                last_matched = Some(idx);
            }
        }
        let Some(matched_at) = last_matched else {
            // Every marked segment rotated out past `.3.log` (or was replaced):
            // return all retained segments so fatal diagnostics are not missed.
            return texts.iter().flat_map(|t| t.lines().map(String::from)).collect();
        };
        let mut lines = Vec::new();
        for (idx, text) in texts.iter().enumerate() {
            let Some(first) = text.lines().next() else { continue };
            if let Some(&len) = self.seen.get(first) {
                lines.extend(text.get(len..).unwrap_or_default().lines().map(String::from));
            } else if let (Some(was_len), Some(orig_len)) = (self.unowned_len, legacy_tail_original_bytes(first, source)) {
                if orig_len > was_len {
                    let body_start = text.find('\n').map_or(text.len(), |i| i + 1);
                    let body = &text[body_start..];
                    let gained = orig_len - was_len;
                    let start = body.len().saturating_sub(gained);
                    let mut start = start;
                    while start < body.len() && !body.is_char_boundary(start) {
                        start += 1;
                    }
                    lines.extend(body[start..].lines().map(String::from));
                }
            } else if idx > matched_at {
                lines.extend(text.lines().map(String::from));
            }
        }
        lines
    }

    /// Messages written by `source` since this mark, stripped of journal
    /// metadata and headers.
    pub fn messages_since(&self, dir: &Path, source: &str) -> Vec<String> {
        self.lines_since(dir, source).iter().filter_map(|line| record_message(line, source).map(String::from)).collect()
    }
}

/// Whether `line` (either a full journal record or a stripped message) reports
/// a panic, a fatal framework error, or a native crash record.
pub fn is_trouble_line(line: &str) -> bool {
    let message = split_record(line).map_or(line, |(_, msg)| msg);
    line.contains(" panic on thread ") || message.starts_with("panic ") || line.contains("fatal error, ending the game: ") || message.starts_with("crash:")
}

/// Lines in `core` and `core-emergency` since `(core, emergency)` that report
/// a panic, a fatal framework error, or a native `crash:` record.
pub fn troubles_since(dir: &Path, core: &SourceMark, emergency: &SourceMark) -> Vec<String> {
    let mut out: Vec<String> = core
        .lines_since(dir, CORE)
        .into_iter()
        .chain(emergency.lines_since(dir, CORE_EMERGENCY))
        .filter(|line| !is_header(line) && is_trouble_line(line))
        .collect();
    out.dedup();
    out
}

/// Why a game process ended before reaching its main menu, inspected across
/// `core-emergency`, `core`, and `proxy` journals written at or after `since`.
pub fn startup_failure_reason(logs_dir: &Path, since: SystemTime) -> Option<String> {
    for source in [CORE_EMERGENCY, CORE, PROXY] {
        let mut candidates = source_segments(logs_dir, source);
        candidates.reverse();
        for path in candidates {
            let fresh = fs::metadata(&path).and_then(|m| m.modified()).ok().is_some_and(|written| written >= since);
            if !fresh {
                continue;
            }
            let text = read_lossy(&path);
            if let Some(found) = extract_startup_reason(&text, source) {
                return Some(found);
            }
        }
    }
    None
}

/// Extracts the last startup failure, fatal error, crash, panic, or proxy
/// load failure from `log`'s text.
pub fn extract_startup_reason(log: &str, source: &str) -> Option<String> {
    let clean: Vec<&str> = log
        .lines()
        .filter(|line| !is_header(line))
        .map(|line| record_message(line, source).unwrap_or_else(|| line.trim()))
        .filter(|line| !line.is_empty())
        .collect();
    if source == PROXY {
        return clean.last().map(|line| format!("proxy: {line}"));
    }
    let at = clean.iter().rposition(|line| {
        line.contains("fatal error, ending the game")
            || line.contains("startup failed")
            || line.starts_with("crash:")
            || line.contains("panic on thread ")
            || line.starts_with("panic ")
    })?;
    Some(clean[at..].iter().take(4).copied().collect::<Vec<_>>().join(" / "))
}
