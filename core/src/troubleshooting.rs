//! The troubleshooting files: complete logs, crash folders and settings,
//! packed into one zip file on the player's desktop from the About page.
//! No saves, file-size limits, payload budget or file-count limit.
//!
//! Packing runs on its own thread. Binary dumps are compressed in chunks;
//! text files are read one at a time for credential redaction.
use crate::zip;
use eu4_harmony_folders::UserFolder;
use std::fs::{File, OpenOptions};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

struct Source {
    name: String,
    path: PathBuf,
}

impl Source {
    fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
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

/// Every existing crash folder is included; saves are never enumerated.
fn sources(user: &UserFolder) -> crate::Result<Vec<Source>> {
    let mut out = Vec::new();
    files_in(&user.logs(), "harmony-logs", &mut out).map_err(|e| format!("cannot list framework logs: {e}"))?;
    files_in(&user.game_logs(), "game-logs", &mut out).map_err(|e| format!("cannot list game logs: {e}"))?;
    for name in user.crash_folders()? {
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
    Ok(out)
}

/// This core, when and where it runs, the command line and the playset.
fn about(user: &UserFolder, now: &str) -> Vec<String> {
    let mut lines = vec![
        format!("Harmony {} (api {})", env!("CARGO_PKG_VERSION"), eu4_harmony_remote::API_VERSION),
        format!("packed {now}"),
        format!("game: {}", crate::game_dir().map_or("not known".into(), |d| d.display().to_string())),
        format!("user folder: {}", user.path().display()),
        format!("command line: {}", eu4_harmony_logging::redact(crate::paths::command_line())),
        format!("run: {}", eu4_harmony_logging::run_id()),
        "collection: complete files at their opening length; all existing crash folders; no file-size, total-size or file-count limits; text credentials redacted; no saves".into(),
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

fn collect<W: std::io::Write>(output: W, sources: Vec<Source>, mut summary: Vec<String>, when: (u16, u16)) -> std::io::Result<W> {
    summary.push("files:".into());
    let mut stream = zip::Stream::new(output);
    let mut payload = 0u64;
    for source in sources {
        let file = match File::open(&source.path) {
            Ok(file) => file,
            Err(e) => {
                summary.push(format!("  {}: not read, {e}", source.name));
                continue;
            }
        };
        let original = file.metadata()?.len();
        // A live log may keep growing. Capture its opening length, without an
        // arbitrary cap or waiting for its writer to stop.
        let mut input = file.take(original);
        let binary = source.path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["dmp", "mdmp", "bin"].iter().any(|x| e.eq_ignore_ascii_case(x)));
        let entry = if binary {
            stream.deflate(source.name.as_bytes(), &mut input, when)?
        } else {
            let mut bytes = Vec::new();
            input.read_to_end(&mut bytes)?;
            let text = eu4_harmony_logging::redact(&String::from_utf8_lossy(&bytes));
            stream.deflate(source.name.as_bytes(), text.as_bytes(), when)?
        };
        if input.limit() != 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("{} shrank while being collected", source.name)));
        }
        summary.push(format!("  {}: original_bytes={original} collected_bytes={} credential_redaction={}", source.name, entry.unpacked, !binary));
        payload += entry.unpacked as u64;
    }
    summary.push(format!("payload_bytes={payload}; about.txt is additional metadata"));
    stream.deflate(b"about.txt", (summary.join("\r\n") + "\r\n").as_bytes(), when)?;
    stream.finish()
}

/// Packs the troubleshooting files onto the desktop; returns the file.
fn pack() -> crate::Result<PathBuf> {
    let game = crate::game_dir().ok_or("the game folder is not known")?;
    let user = UserFolder::new(crate::paths::user_dir(game)?);
    let t = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    let now = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    crate::log::checkpoint();
    let summary = about(&user, &now);
    let sources = sources(&user)?;
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
    if let Err(e) = collect(file, sources, summary, dos_time(&t)) {
        let _ = std::fs::remove_file(&path);
        return Err(format!("cannot write {}: {e}", path.display()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek as _, Write as _};

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
    fn the_files_are_all_logs_crashes_and_settings_but_no_saves_or_staging_files() {
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
        let names: Vec<String> = sources(&user).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                "crashes/eu4_2026_11_08/exception.txt",
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
    fn crash_dumps_of_the_reported_size_are_kept_whole_beyond_the_old_total_budget() {
        let dir = scratch("whole-dumps");
        let dump_size = 76_924_260;
        let mut candidates = Vec::new();
        for n in 0..2 {
            let path = dir.join(format!("dump-{n}.dmp"));
            let mut file = File::create(&path).unwrap();
            file.write_all(b"MDMP-start").unwrap();
            file.set_len(dump_size).unwrap();
            file.seek(std::io::SeekFrom::End(-8)).unwrap();
            file.write_all(b"dump-end").unwrap();
            candidates.push(Source::new(format!("crashes/eu4_20261114_19072{n}/minidump.dmp"), path));
        }
        let output = File::create(dir.join("collected.zip")).unwrap();
        drop(collect(output, candidates, Vec::new(), (0, 0x21)).unwrap());
        let mut archive = File::open(dir.join("collected.zip")).unwrap();
        let entries = zip::directory(&mut archive, 1 << 20).unwrap();
        for n in 0..2 {
            let name = format!("crashes/eu4_20261114_19072{n}/minidump.dmp");
            let entry = entries.iter().find(|e| e.name == name.as_bytes()).expect("the crash dump must be present");
            let bytes = zip::read(&mut archive, entry, dump_size as usize).unwrap();
            assert_eq!(bytes.len(), dump_size as usize);
            assert_eq!(&bytes[..10], b"MDMP-start");
            assert_eq!(&bytes[bytes.len() - 8..], b"dump-end");
            assert!(bytes[10..bytes.len() - 8].iter().all(|&byte| byte == 0));
        }
        let summary = String::from_utf8(zip::find(&mut archive, b"about.txt", 1 << 20).unwrap()).unwrap();
        assert!(summary.contains("payload_bytes=153848520;"));
        drop(archive);
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
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), (0, 0x21)).unwrap());
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
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), (0, 0x21)).unwrap());
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
        let mut archive = std::io::Cursor::new(collect(Vec::new(), candidates, Vec::new(), (0, 0x21)).unwrap());
        let summary = String::from_utf8(zip::find(&mut archive, b"about.txt", 4096).unwrap()).unwrap();
        assert!(summary.contains("crashes/missing/minidump.dmp: not read,"));
        std::fs::remove_dir_all(dir).unwrap();
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
