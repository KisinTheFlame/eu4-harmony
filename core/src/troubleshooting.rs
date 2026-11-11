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
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How many of the last crash folders are packed.
const CRASHES: usize = 3;
/// A file larger than this is left out, and the summary says so.
const LARGEST: u64 = 512 << 20;

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

/// One file to pack: its name in the zip file, and where it is.
struct Source {
    name: String,
    path: PathBuf,
}

/// The files of `dir`, not its folders, each named in the zip file under
/// `under`.
fn files_in(dir: &Path, under: &str) -> Vec<Source> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Source> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| Source { name: format!("{under}/{}", e.file_name().to_string_lossy()), path: e.path() })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Every file the troubleshooting files hold of the user folder `user`.
fn sources(user: &UserFolder) -> Vec<Source> {
    let mut out = files_in(&user.logs(), "harmony-logs");
    out.extend(files_in(&user.game_logs(), "game-logs"));
    let crashes = user.crash_folders().unwrap_or_default();
    for name in crashes.iter().rev().take(CRASHES).rev() {
        out.extend(files_in(&user.crashes().join(name), &format!("crashes/{name}")));
    }
    for (name, path) in [
        ("settings/harmony-settings.txt", user.harmony().join("settings.txt")),
        ("settings/settings.txt", user.path().join("settings.txt")),
        ("settings/dlc_load.json", user.path().join("dlc_load.json")),
    ] {
        if path.is_file() {
            out.push(Source { name: name.into(), path });
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
        format!("command line: {}", crate::paths::command_line()),
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
    let mut summary = about(&user, &now);
    summary.push("files:".into());
    let mut files = Vec::new();
    for source in sources(&user) {
        let size = std::fs::metadata(&source.path).map(|m| m.len()).unwrap_or(0);
        if size > LARGEST {
            summary.push(format!("  {}: left out, {size} bytes", source.name));
            continue;
        }
        // The game and the framework write some of them still: read as they
        // are now, beside their writers.
        match std::fs::read(&source.path) {
            Ok(data) => {
                summary.push(format!("  {}: {} bytes", source.name, data.len()));
                files.push((source.name, data));
            }
            Err(e) => summary.push(format!("  {}: not read, {e}", source.name)),
        }
    }
    files.insert(0, ("about.txt".into(), (summary.join("\r\n") + "\r\n").into_bytes()));
    let name = format!("eu4-harmony-troubleshooting-{:04}{:02}{:02}-{:02}{:02}{:02}.zip", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond);
    let file = desktop()?.join(name);
    std::fs::write(&file, zip_of(&files, dos_time(&t))).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    Ok(file)
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
        let names: Vec<String> = sources(&user).into_iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                "harmony-logs/core.log",
                "harmony-logs/report.txt",
                "game-logs/error.log",
                "crashes/eu4_2026_11_09/exception.txt",
                "crashes/eu4_2026_11_10/exception.txt",
                "crashes/eu4_2026_11_11/exception.txt",
                "settings/harmony-settings.txt",
                "settings/settings.txt",
                "settings/dlc_load.json",
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
