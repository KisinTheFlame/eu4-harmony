//! The troubleshooting files: logs, recent crash folders, selected Windows
//! crash evidence and settings, packed from the About page within a 24 MiB
//! archive cap (GitHub Issues attachment friendly). No saves.
//!
//! Packing runs on its own thread. Text files are read one at a time for
//! credential redaction. Oversized minidumps that would exceed the archive
//! budget are slimmed in memory to `MiniDumpNormal`-equivalent stacks/modules
//! before compression without modifying the original files on disk.
use crate::base::zip;
use eu4_harmony_folders::UserFolder;
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

pub(crate) mod local_dumps;
mod minidump;
mod windows_crashes;
mod windows_dumps;

/// Hard target for the output troubleshooting zip (fits GitHub Issues' 25 MiB cap).
const MAX_ARCHIVE_BYTES: u64 = 24 * 1024 * 1024;
/// Reserved headroom for `about.txt` and ZIP end-of-central-directory records.
const ARCHIVE_RESERVE_BYTES: u64 = 256 * 1024;
/// Maximum recent game crash folders included in one troubleshooting archive.
const MAX_CRASH_FOLDERS: usize = 3;
/// Maximum recent matched Windows minidumps included in one troubleshooting archive.
const MAX_WINDOWS_DUMPS: usize = 3;
/// Binary dumps larger than this on disk are slimmed first when budget is tight.
const SLIM_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;

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
            Ok((file, bytes)) => {
                log!("troubleshooting files saved to {} ({bytes} bytes)", file.display());
                let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                State::Saved(format!("{name} ({})", format_bytes(bytes)))
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

struct Source {
    name: String,
    path: PathBuf,
    /// Windows dumps are checked on the handle that will be packed, in case
    /// Windows rotates them between enumeration and collection.
    opened: Option<File>,
}

impl Source {
    fn new(name: String, path: PathBuf) -> Self {
        Self { name, path, opened: None }
    }
}

fn files_in(dir: &Path, under: &str, out: &mut Vec<Source>) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file() && !matches!(eu4_harmony_logging::classify_file(&name), eu4_harmony_logging::Artifact::Staging) {
            out.push(Source::new(format!("{under}/{name}"), entry.path()));
        }
    }
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / MIB)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

/// Recent crash folders (up to [`MAX_CRASH_FOLDERS`], newest first) and logs/settings.
/// Saves are never enumerated. Returns `(sources, omitted_crash_folder_count)`.
fn sources(user: &UserFolder) -> crate::base::Result<(Vec<Source>, usize)> {
    let mut out = Vec::new();
    files_in(&user.logs(), "harmony-logs", &mut out).map_err(|e| format!("cannot list framework logs: {e}"))?;
    files_in(&user.game_logs(), "game-logs", &mut out).map_err(|e| format!("cannot list game logs: {e}"))?;
    let mut crash_dirs: Vec<(SystemTime, String)> = user
        .crash_folders()?
        .into_iter()
        .map(|name| {
            let modified = std::fs::metadata(user.crashes().join(&name)).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            (modified, name)
        })
        .collect();
    // Sort newest first; tie-break by descending folder name.
    crash_dirs.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let omitted_crashes = crash_dirs.len().saturating_sub(MAX_CRASH_FOLDERS);
    for (_, name) in crash_dirs.into_iter().take(MAX_CRASH_FOLDERS) {
        files_in(&user.crashes().join(&name), &format!("crashes/{name}"), &mut out).map_err(|e| format!("cannot list crash folder {name}: {e}"))?;
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
    out.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    Ok((out, omitted_crashes))
}

/// This core, when and where it runs, the command line and the playset.
fn about(user: &UserFolder, now: &str) -> Vec<String> {
    let mut lines = vec![
        format!("Harmony {} (api {})", env!("CARGO_PKG_VERSION"), eu4_harmony_remote::API_VERSION),
        format!("packed {now}"),
        format!("game: {}", crate::base::game_dir().map_or("not known".into(), |d| d.display().to_string())),
        format!("user folder: {}", user.path().display()),
        format!("command line: {}", eu4_harmony_logging::redact(crate::base::paths::command_line())),
        format!("run: {}", eu4_harmony_logging::run_id()),
        format!(
            "collection: logs, settings, up to {MAX_CRASH_FOLDERS} recent crash folders and {MAX_WINDOWS_DUMPS} matched Windows minidumps within {} MiB cap; oversized minidumps slimmed to stacks/modules in-memory without changing disk files; text credentials redacted; no saves",
            MAX_ARCHIVE_BYTES / (1024 * 1024)
        ),
        "playset:".into(),
    ];
    lines.extend(crate::base::mods::playset().iter().map(|m| format!("  {m}")));
    lines
}

/// MS-DOS's time and date of `t`, as a zip entry holds them.
fn dos_time(t: &windows_sys::Win32::Foundation::SYSTEMTIME) -> (u16, u16) {
    let time = (t.wHour << 11) | (t.wMinute << 5) | (t.wSecond / 2);
    let date = (t.wYear.saturating_sub(1980) << 9) | (t.wMonth << 5) | t.wDay;
    (time, date)
}

/// The desktop, where the player finds the file.
fn desktop() -> crate::base::Result<PathBuf> {
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

fn is_binary_dump(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["dmp", "mdmp", "bin"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Deflates `data` into memory, stopping early and returning `None` if the
/// compressed output would exceed `budget` bytes.
fn deflate_bounded(data: &[u8], budget: u64) -> Option<(Vec<u8>, u32)> {
    use miniz_oxide::deflate::core::CompressorOxide;
    use miniz_oxide::deflate::stream::deflate;
    use miniz_oxide::{DataFormat, MZFlush, MZStatus};
    let mut compressor = Box::<CompressorOxide>::default();
    compressor.set_format_and_level(DataFormat::Raw, 6);
    let mut out = Vec::new();
    let mut chunk = vec![0u8; 64 << 10];
    let crc = !zip::crc32_update(!0, data);
    let mut offset = 0;
    loop {
        let end = (offset + (64 << 10)).min(data.len());
        let slice = &data[offset..end];
        let flush = if end == data.len() { MZFlush::Finish } else { MZFlush::None };
        let mut consumed = 0;
        loop {
            let res = deflate(&mut compressor, &slice[consumed..], &mut chunk, flush);
            consumed += res.bytes_consumed;
            if out.len() as u64 + res.bytes_written as u64 > budget {
                return None;
            }
            out.extend_from_slice(&chunk[..res.bytes_written]);
            match res.status {
                Ok(MZStatus::StreamEnd) => return Some((out, crc)),
                Ok(_) if consumed == slice.len() && flush == MZFlush::None => break,
                Ok(_) if res.bytes_consumed != 0 || res.bytes_written != 0 => (),
                _ => return None,
            }
        }
        offset = end;
    }
}

/// Deflates a file up to `original` bytes into memory, returning `None` if the
/// compressed size exceeds `budget`.
fn deflate_file_bounded(file: &mut File, original: u64, budget: u64) -> std::io::Result<Option<(Vec<u8>, u32, usize)>> {
    use miniz_oxide::deflate::core::CompressorOxide;
    use miniz_oxide::deflate::stream::deflate;
    use miniz_oxide::{DataFormat, MZFlush, MZStatus};
    file.seek(SeekFrom::Start(0))?;
    let mut input = file.take(original);
    let mut compressor = Box::<CompressorOxide>::default();
    compressor.set_format_and_level(DataFormat::Raw, 6);
    let mut out = Vec::new();
    let mut in_buf = vec![0u8; 64 << 10];
    let mut out_buf = vec![0u8; 64 << 10];
    let mut crc = !0u32;
    let mut unpacked = 0usize;
    loop {
        let len = match input.read(&mut in_buf) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        unpacked = unpacked.checked_add(len).ok_or_else(|| std::io::Error::other("dump size overflow"))?;
        crc = zip::crc32_update(crc, &in_buf[..len]);
        let flush = if len == 0 { MZFlush::Finish } else { MZFlush::None };
        let mut consumed = 0;
        loop {
            let res = deflate(&mut compressor, &in_buf[consumed..len], &mut out_buf, flush);
            consumed += res.bytes_consumed;
            if out.len() as u64 + res.bytes_written as u64 > budget {
                return Ok(None);
            }
            out.extend_from_slice(&out_buf[..res.bytes_written]);
            match res.status {
                Ok(MZStatus::StreamEnd) => {
                    if input.limit() != 0 {
                        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "source shrank while being collected"));
                    }
                    return Ok(Some((out, !crc, unpacked)));
                }
                Ok(_) if consumed == len && len != 0 => break,
                Ok(_) if res.bytes_consumed != 0 || res.bytes_written != 0 => (),
                _ => return Err(std::io::Error::other("deflate failed to make progress")),
            }
        }
        if len == 0 {
            break;
        }
    }
    Ok(Some((out, !crc, unpacked)))
}

fn collect<W: std::io::Write>(output: W, sources: Vec<Source>, summary: Vec<String>, windows_crashes: Option<&str>, when: (u16, u16)) -> std::io::Result<W> {
    collect_with_cap(output, sources, summary, windows_crashes, when, MAX_ARCHIVE_BYTES)
}

fn collect_with_cap<W: std::io::Write>(
    output: W,
    sources: Vec<Source>,
    mut summary: Vec<String>,
    windows_crashes: Option<&str>,
    when: (u16, u16),
    max_archive_bytes: u64,
) -> std::io::Result<W> {
    summary.push("files:".into());
    let mut stream = zip::Stream::new(output);
    let mut payload = 0u64;

    let (mut dumps, texts): (Vec<Source>, Vec<Source>) = sources.into_iter().partition(|s| is_binary_dump(&s.path));
    // Pack newest dumps first so recent crash evidence gets priority in the budget.
    dumps.sort_unstable_by(|a, b| {
        let mtime = |s: &Source| std::fs::metadata(&s.path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        mtime(b).cmp(&mtime(a)).then_with(|| b.name.cmp(&a.name))
    });

    // 1. Pack all text logs, settings, and crash text summaries first.
    for source in texts {
        let file = match source.opened.map_or_else(|| File::open(&source.path), Ok) {
            Ok(file) => file,
            Err(e) => {
                summary.push(format!("  {}: not read, {e}", source.name));
                continue;
            }
        };
        let original = file.metadata()?.len();
        let mut input = file.take(original);
        let mut bytes = Vec::new();
        input.read_to_end(&mut bytes)?;
        if input.limit() != 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("{} shrank while being collected", source.name)));
        }
        let text = eu4_harmony_logging::redact(&String::from_utf8_lossy(&bytes));
        let budget = max_archive_bytes.saturating_sub(ARCHIVE_RESERVE_BYTES).saturating_sub(stream.estimated_bytes());
        if let Some((compressed, crc)) = deflate_bounded(text.as_bytes(), budget) {
            let entry = stream.write_deflated(source.name.as_bytes(), &compressed, text.len(), crc, when)?;
            summary.push(format!(
                "  {}: original_bytes={original} collected_bytes={} packed_bytes={} credential_redaction=true",
                source.name, entry.unpacked, entry.packed
            ));
            payload += entry.unpacked as u64;
        } else {
            summary.push(format!("  {}: skipped (exceeds {} MiB archive cap; original_bytes={original})", source.name, max_archive_bytes / (1024 * 1024)));
        }
    }

    if let Some(text) = windows_crashes {
        let text = eu4_harmony_logging::redact(text);
        let entry = stream.deflate(b"windows-crashes.txt", text.as_bytes(), when)?;
        summary.push(format!(
            "  windows-crashes.txt: generated from Windows Application Error events; collected_bytes={} packed_bytes={} credential_redaction=true",
            entry.unpacked, entry.packed
        ));
        payload += entry.unpacked as u64;
    }

    // 2. Pack binary dumps newest first within the remaining archive cap.
    //    Oversized dumps are slimmed in memory to stack/module/exception streams
    //    when full compression would exceed the budget.
    for source in dumps {
        let mut file = match source.opened.map_or_else(|| File::open(&source.path), Ok) {
            Ok(file) => file,
            Err(e) => {
                summary.push(format!("  {}: not read, {e}", source.name));
                continue;
            }
        };
        let original = file.metadata()?.len();
        let entry_overhead = 256u64 + source.name.len() as u64 * 2;
        let budget = max_archive_bytes.saturating_sub(ARCHIVE_RESERVE_BYTES).saturating_sub(stream.estimated_bytes()).saturating_sub(entry_overhead);
        if budget == 0 {
            summary.push(format!("  {}: skipped (archive cap reached; original_bytes={original})", source.name));
            continue;
        }

        // Try full dump first if it is modest in size; otherwise slim oversized dumps first.
        let try_full_first = original <= SLIM_THRESHOLD_BYTES;
        let mut written = false;

        if try_full_first {
            if let Some((compressed, crc, unpacked)) = deflate_file_bounded(&mut file, original, budget)? {
                let entry = stream.write_deflated(source.name.as_bytes(), &compressed, unpacked, crc, when)?;
                summary.push(format!(
                    "  {}: original_bytes={original} collected_bytes={} packed_bytes={} slimmed=false credential_redaction=false",
                    source.name, entry.unpacked, entry.packed
                ));
                payload += entry.unpacked as u64;
                written = true;
            }
        }

        if !written {
            match minidump::slim(&mut file) {
                Ok(slimmed) if (slimmed.len() as u64) < original => {
                    if let Some((compressed, crc)) = deflate_bounded(&slimmed, budget) {
                        let entry = stream.write_deflated(source.name.as_bytes(), &compressed, slimmed.len(), crc, when)?;
                        summary.push(format!(
                            "  {}: original_bytes={original} collected_bytes={} packed_bytes={} slimmed=true credential_redaction=false",
                            source.name, entry.unpacked, entry.packed
                        ));
                        payload += entry.unpacked as u64;
                        written = true;
                    }
                }
                _ => {}
            }
        }

        // If slimming was unavailable (e.g. non-MDMP binary or already slim) and we skipped trying full first, try full within budget.
        if !written && !try_full_first {
            if let Some((compressed, crc, unpacked)) = deflate_file_bounded(&mut file, original, budget)? {
                let entry = stream.write_deflated(source.name.as_bytes(), &compressed, unpacked, crc, when)?;
                summary.push(format!(
                    "  {}: original_bytes={original} collected_bytes={} packed_bytes={} slimmed=false credential_redaction=false",
                    source.name, entry.unpacked, entry.packed
                ));
                payload += entry.unpacked as u64;
                written = true;
            }
        }

        if !written {
            summary.push(format!("  {}: skipped (exceeds remaining archive budget of {budget} bytes; original_bytes={original})", source.name));
        }
    }

    summary.push(format!("payload_bytes={payload}; about.txt is additional metadata"));
    stream.deflate(b"about.txt", (summary.join("\r\n") + "\r\n").as_bytes(), when)?;
    stream.finish()
}

/// Packs the troubleshooting files onto the desktop; returns `(path, archive_bytes)`.
fn pack() -> crate::base::Result<(PathBuf, u64)> {
    let game = crate::base::game_dir().ok_or("the game folder is not known")?;
    let user = UserFolder::new(crate::base::paths::user_dir(game)?);
    let t = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    let now = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    crate::base::log::checkpoint();
    let mut summary = about(&user, &now);
    let (mut sources, omitted_crashes) = sources(&user)?;
    if omitted_crashes > 0 {
        summary.push(format!("game crash folders: kept newest {MAX_CRASH_FOLDERS}, omitted {omitted_crashes} older folder(s)"));
    }
    if let Some(root) = eu4_harmony_folders::windows_dump_root() {
        let configured = local_dumps::dump_folder().unwrap_or_else(|e| {
            summary.push(format!("Windows minidumps: cannot read the configured dump folder: {e}"));
            None
        });
        let (mut dumps, skipped) = windows_dumps::find(&root, configured.as_deref(), &sources);
        dumps.sort_unstable_by(|a, b| {
            let mtime = |s: &Source| std::fs::metadata(&s.path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            mtime(b).cmp(&mtime(a)).then_with(|| b.name.cmp(&a.name))
        });
        let omitted_dumps = dumps.len().saturating_sub(MAX_WINDOWS_DUMPS);
        dumps.truncate(MAX_WINDOWS_DUMPS);
        summary.push(format!(
            "Windows minidumps: {} matched (kept up to {MAX_WINDOWS_DUMPS}, omitted {omitted_dumps} older), {skipped} unavailable or unmatched",
            dumps.len() + omitted_dumps
        ));
        sources.extend(dumps);
    } else {
        summary.push("Windows minidumps: cannot find LOCALAPPDATA".into());
    }
    let windows_crashes = windows_crashes::report(game);
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
    if let Err(e) = collect(file, sources, summary, Some(&windows_crashes), dos_time(&t)) {
        let _ = std::fs::remove_file(&path);
        return Err(format!("cannot write {}: {e}", path.display()));
    }
    let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
    Ok((path, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

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
    fn the_files_are_logs_settings_and_recent_crashes_but_no_saves_or_staging_files() {
        let dir = scratch("sources");
        let user = UserFolder::new(&dir);
        put(&user.core_log(), "core");
        put(&user.report(), "report");
        put(&user.logs().join("core.snapshot-run-1.new"), "in progress");
        put(&user.game_logs().join("error.log"), "error");
        for name in ["eu4_2026_11_08", "eu4_2026_11_09", "eu4_2026_11_10", "eu4_2026_11_11"] {
            put(&user.crashes().join(name).join("exception.txt"), name);
        }
        put(&user.harmony().join("settings.txt"), "harmony");
        put(&dir.join("settings.txt"), "game");
        put(&dir.join("dlc_load.json"), "{}");
        put(&user.save_games().join("a.eu4"), "save");
        let (listed, omitted) = sources(&user).unwrap();
        assert_eq!(omitted, 1, "4 crash folders keep the newest 3");
        let names: Vec<String> = listed.into_iter().map(|s| s.name).collect();
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
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_dumps_stay_within_archive_cap_and_record_disposition() {
        let dir = scratch("capped-dumps");
        // Write a pseudo-random incompressible dump larger than a small test cap.
        let path = dir.join("big.dmp");
        let mut seed = 0x1234_5678_9abc_def0u64;
        let mut noise = vec![0u8; 512 * 1024];
        for b in &mut noise {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *b = seed as u8;
        }
        std::fs::write(&path, &noise).unwrap();
        let candidates = vec![Source::new("crashes/eu4_20261114/minidump.dmp".into(), path)];
        let cap = 384 * 1024;
        let archive_bytes = collect_with_cap(Vec::new(), candidates, Vec::new(), None, (0, 0x21), cap).unwrap();
        assert!((archive_bytes.len() as u64) <= cap);
        let mut archive = std::io::Cursor::new(archive_bytes);
        let summary = String::from_utf8(zip::find(&mut archive, b"about.txt", 4096).unwrap()).unwrap();
        assert!(summary.contains("skipped (exceeds remaining archive budget"), "{summary}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn large_logs_keep_their_beginning_and_end_and_redact_complete_multiline_values() {
        let dir = scratch("whole-text");
        let path = dir.join("error.log");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"first useful line\n").unwrap();
        for _ in 0..9 {
            file.write_all(&vec![b'x'; 1 << 20]).unwrap();
        }
        file.write_all(b"\n-auth_token=private-value\n{\"password\":\"first-secret\nsecond-secret\"}\nlast useful line\n").unwrap();
        drop(file);
        // Both kinds of text exceeded their former per-file limits.
        let candidates = ["game-logs/error.log", "harmony-logs/core.log"].into_iter().map(|name| Source::new(name.into(), path.clone())).collect();
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), None, (0, 0x21)).unwrap());
        for name in [b"game-logs/error.log".as_slice(), b"harmony-logs/core.log"] {
            let bytes = zip::find(&mut archive, name, 10 << 20).unwrap();
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.starts_with("first useful line\n"));
            assert!(text.ends_with("last useful line\n"));
            assert_eq!(text.bytes().filter(|&byte| byte == b'x').count(), 9 << 20);
            assert!(!text.contains("private-value"));
            assert!(!text.contains("first-secret"));
            assert!(!text.contains("second-secret"));
            assert_eq!(text.matches("[redacted]").count(), 2);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn files_beyond_the_old_count_limit_are_all_collected() {
        let dir = scratch("all-candidates");
        for n in 0..1031 {
            put(&dir.join(format!("record-{n:04}.log")), "evidence");
        }
        let mut candidates = Vec::new();
        files_in(&dir, "harmony-logs", &mut candidates).unwrap();
        assert_eq!(candidates.len(), 1031);
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), None, (0, 0x21)).unwrap());
        let entries = zip::directory(&mut archive, 1 << 20).unwrap();
        assert_eq!(entries.len(), 1032);
        for n in 0..1031 {
            let name = format!("harmony-logs/record-{n:04}.log");
            let entry = entries.iter().find(|e| e.name == name.as_bytes()).unwrap();
            assert_eq!(zip::read(&mut archive, entry, 4096).unwrap(), b"evidence");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_sources_are_named_in_the_summary() {
        let dir = scratch("missing-source");
        let candidates = vec![Source::new("crashes/missing/minidump.dmp".into(), dir.join("missing.dmp"))];
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), None, (0, 0x21)).unwrap());
        let summary = String::from_utf8(zip::find(&mut archive, b"about.txt", 4096).unwrap()).unwrap();
        assert!(summary.contains("crashes/missing/minidump.dmp: not read,"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn windows_evidence_or_a_query_failure_is_included_in_the_archive() {
        for report in ["run=902-1dd76a22ea180f6 exception_code=0xc0000374\n", "Collection incomplete: Access is denied.\n"] {
            let mut archive = std::io::Cursor::new(collect(Vec::new(), Vec::new(), Vec::new(), Some(report), (0, 0x21)).unwrap());
            assert_eq!(zip::find(&mut archive, b"windows-crashes.txt", 4096).unwrap(), report.as_bytes());
            let summary = String::from_utf8(zip::find(&mut archive, b"about.txt", 4096).unwrap()).unwrap();
            assert!(summary.contains(&format!("collected_bytes={}", report.len())));
            assert!(summary.contains(&format!("payload_bytes={};", report.len())));
        }
    }

    #[test]
    fn windows_dumps_are_packed_only_for_a_matching_process_in_this_user_folder() {
        fn dump(pid: u32, created: u32) -> Vec<u8> {
            let mut bytes = vec![0u8; 264];
            let mut put = |at: usize, words: &[u32]| {
                for (index, word) in words.iter().enumerate() {
                    let offset = at + index * 4;
                    bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
                }
            };
            put(0, &[u32::from_le_bytes(*b"MDMP"), 0xA793, 2, 32]);
            put(32, &[15, 24, 56, 6, 168, 84]);
            put(56, &[24, 3, pid, created, 0, 0]);
            put(244, &[12, 252]);
            bytes
        }

        let dir = scratch("windows-dumps");
        let user = UserFolder::new(dir.join("user"));
        let root = dir.join("CrashDumps");
        let local = root.join("eu4-harmony");
        std::fs::create_dir_all(&local).unwrap();
        let created = 1_794_926_671u32;
        let run = eu4_harmony_logging::process_run_id(902, u64::from(created) * 10_000_000 + 116_444_736_000_012_345);
        put(&user.core_log(), &format!("# Harmony owned log v1 source=core run={run} segment=1-1\n"));
        let payload = dump(902, created);
        let custom = dir.join("administrator-selected-folder");
        std::fs::create_dir(&custom).unwrap();
        std::fs::write(custom.join("eu4.exe.902.dmp"), &payload).unwrap();
        let (candidates, _) = sources(&user).unwrap();
        let (dumps, _) = windows_dumps::find(&root, Some(&custom), &candidates);
        assert_eq!(dumps.len(), 1);
        assert_eq!(dumps[0].path, custom.join("eu4.exe.902.dmp"));
        drop(dumps);
        std::fs::write(local.join("eu4.exe.902.dmp"), &payload).unwrap();
        let exact = u64::from(created) * 10_000_000 + 116_444_736_000_012_345;
        std::fs::write(local.join(format!("harmony-902-{exact:016x}.dmp")), &payload).unwrap();
        std::fs::write(local.join(format!("harmony-902-{:016x}.dmp", exact + 1)), &payload).unwrap();
        std::fs::write(root.join("eu4.exe.902.dmp"), dump(902, created - 1)).unwrap();
        std::fs::write(root.join("eu4.exe.903.dmp"), dump(903, created)).unwrap();
        std::fs::write(root.join("other.exe.902.dmp"), &payload).unwrap();
        std::fs::write(root.join("eu4.exe.904.dmp"), &payload[..252]).unwrap();
        let (mut candidates, _) = sources(&user).unwrap();
        let (dumps, skipped) = windows_dumps::find(&root, Some(&local), &candidates);
        assert_eq!((dumps.len(), skipped), (1, 4));
        candidates.extend(dumps);
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), None, (0, 0x21)).unwrap());
        let path = format!("windows-dumps/{run}/minidump.dmp");
        assert_eq!(zip::find(&mut archive, path.as_bytes(), 4096).unwrap(), payload);
        std::fs::remove_dir_all(&dir).unwrap();
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
