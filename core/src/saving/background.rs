//! Saves compressed in the background (docs/plans/game-speed.md, S3).
//!
//! The game compresses a save's entries into its zip on the main thread, at
//! zlib's level 3: about 40% of an ironman autosave's time late in a game
//! (docs/research/main-thread.md). While one of the local saves runs (the
//! save `0x5c6500` and the ironman save `0x5c7ac0`), the framework has the
//! game deflate them at level 0, which copies the data into stored blocks
//! without compressing it, so the game writes, checks and moves its file as it
//! always does, only sooner. (Not the zip's method 0, "stored": the game's
//! code for it reads its data from where the stream stands, after the end,
//! and wrote a save of zeros when it was tried;
//! docs/prototype-log/2026-11-04-game-speed-s1-s4.md.)
//! Each file the save moved into place is then handed to a thread of the
//! framework's, which deflates its uncompressed entries again at a real level,
//! reads the result back, and puts it in the file's place, unless the file
//! changed in the meantime. It first checks every entry: its CRC, and that a
//! save's own entries begin as the game begins them, with `EU4`. A save the
//! thread cannot compress stays as the game wrote it, uncompressed and whole.
//!
//! The next local save, a load and a restart wait for the thread first, so it
//! never puts an older file over one the game wrote since. The game's exit
//! does not: a file it had not finished stays uncompressed, and the temporary
//! file it was writing, if any, is removed at the next start. Cloud saves take
//! another path (`0x5c8ba0`) and keep the game's compression.
//!
//! The game's deflate at level 0 is the framework's copy (`super::stored`).
use crate::hooks::Call;
use crate::points::Point;
use crate::speed::SpeedUp;
use crate::zip::{self, DEFLATED, STORED};
use crate::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// `(request)`: the local save and the local ironman save.
const LOCAL_SAVE: usize = 0x5c6500;
const IRONMAN_SAVE: usize = 0x5c7ac0;
/// `(zip, name, data, method, level)`: adds an entry to a zip being written.
const ADD_ENTRY: usize = 0x153b9a0;
/// `(from, to) -> bool`: the game's file move, both paths in the folder the
/// game writes to.
const MOVE_FILE: usize = 0x1715bb0;

/// The level the save asks for, and the one it gets during a local save,
/// which the game's deflate copies at (`super::stored`).
const GAME_LEVEL: usize = 3;
pub(super) const COPY_LEVEL: usize = 0;

/// How hard the thread compresses: zlib's default, a smaller file than the
/// game's level 3 in a little more time, which the main thread does not wait
/// for.
const LEVEL: u8 = 6;

/// The entries a save's zip holds that the game writes with its header, and
/// the header they begin with.
const SAVE_ENTRIES: [&[u8]; 3] = [b"meta", b"gamestate", b"ai"];
const HEADER: &[u8] = b"EU4";

/// What the thread adds to a save's name for the file it writes before
/// moving it into the save's place.
const TMP_SUFFIX: &str = ".eu4harmony-tmp";
// Where the game keeps its saves, in the user folder.
use eu4_harmony_folders::SAVE_GAMES;

/// Left to the game, a local save's entries are deflated at the game's level
/// on the main thread, as before.
pub const SPEED_UP: SpeedUp = SpeedUp { name: "background-compression", group: "saving", points, checkable: false };

fn points() -> Vec<Point> {
    vec![
        Point::observe(LOCAL_SAVE, "48 89 5c 24 10 48 89 74 24 18 57 41 54 41 55", "local save", 1, Some(save_before), Some(save_after)),
        Point::observe(IRONMAN_SAVE, "48 89 5c 24 18 48 89 74 24 20 57 41 54 41 55", "local ironman save", 1, Some(save_before), Some(save_after)),
        Point::observe(ADD_ENTRY, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18", "zip entry", 5, Some(entry_before), None),
        Point::observe(MOVE_FILE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20", "file move", 2, None, Some(moved)),
    ]
}

/// A local save is under way on the main thread.
static SAVING: AtomicBool = AtomicBool::new(false);
/// When the local save under way began.
static BEGAN: Mutex<Option<Instant>> = Mutex::new(None);
/// The files the save under way moved into place, by their paths in the
/// folder the game writes to.
static MOVED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The folder the game writes to: the user folder.
static FOLDER: OnceLock<PathBuf> = OnceLock::new();

/// Where the game writes its files, once the user folder is known; at
/// startup, before any save.
pub fn set_folder(user_dir: &Path) {
    let _ = FOLDER.set(user_dir.to_path_buf());
    let removed = remove_leftovers(&user_dir.join(SAVE_GAMES));
    if removed > 0 {
        log!("saves: removed {removed} temporary file(s) left by a compression the process's end cut short");
    }
}

/// Removes the temporary files in `dir` the thread was writing when the
/// process ended (the save beside each stays as the game wrote it); how many.
fn remove_leftovers(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(TMP_SUFFIX)).filter(|e| std::fs::remove_file(e.path()).is_ok()).count()
}

fn save_before(_: &mut Call) {
    wait();
    MOVED.lock().unwrap_or_else(|e| e.into_inner()).clear();
    SAVING.store(crate::thread_id() == crate::main_thread(), Ordering::SeqCst);
    *BEGAN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
}

fn save_after(_: &mut Call) {
    if !SAVING.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(began) = BEGAN.lock().unwrap_or_else(|e| e.into_inner()).take() {
        log!("saves: a local save stopped the game for {:.0} ms", began.elapsed().as_secs_f64() * 1000.0);
    }
    let moved = std::mem::take(&mut *MOVED.lock().unwrap_or_else(|e| e.into_inner()));
    let Some(folder) = FOLDER.get() else { return };
    for path in moved {
        hand_over(folder.join(path));
    }
}

fn entry_before(call: &mut Call) {
    if SAVING.load(Ordering::SeqCst)
        && call.arg(3) as u32 == DEFLATED as u32
        && call.arg(4) as u32 == GAME_LEVEL as u32
        && crate::thread_id() == crate::main_thread()
    {
        call.set_arg(4, COPY_LEVEL);
    }
}

fn moved(call: &mut Call) {
    if !SAVING.load(Ordering::SeqCst) || call.result_as::<u8>().unwrap_or(0) == 0 || call.arg(1) == 0 {
        return;
    }
    let to = unsafe { std::ffi::CStr::from_ptr(call.arg(1) as *const std::ffi::c_char) };
    match to.to_str() {
        Ok(to) => MOVED.lock().unwrap_or_else(|e| e.into_inner()).push(to.to_string()),
        Err(_) => log!("saves: a file moved to a path that is not UTF-8 stays uncompressed"),
    }
}

/// The game waits for the thread: a file another program holds is not tried
/// again.
static WAITED_FOR: AtomicBool = AtomicBool::new(false);

struct Waited;

impl Drop for Waited {
    fn drop(&mut self) {
        WAITED_FOR.store(false, Ordering::SeqCst);
    }
}

/// How often and how far apart the thread tries to put its file in the
/// save's place while another program holds the save open, as a program
/// that copies each save it sees may: two seconds in all.
const MOVE_TRIES: u32 = 10;
const MOVE_AGAIN: Duration = Duration::from_millis(200);

/// Files handed to the thread and not yet done.
struct Pending {
    count: Mutex<usize>,
    done: Condvar,
}

static PENDING: Pending = Pending { count: Mutex::new(0), done: Condvar::new() };
static WORKER: OnceLock<Mutex<Sender<PathBuf>>> = OnceLock::new();

/// Hands a file to the thread, which it starts the first time.
fn hand_over(path: PathBuf) {
    let sender = WORKER.get_or_init(|| {
        let (send, receive) = mpsc::channel::<PathBuf>();
        let started = std::thread::Builder::new().name("eu4-harmony saves".into()).spawn(move || {
            for path in receive {
                let start = Instant::now();
                match compact_file(&path) {
                    Ok(Some((before, after))) => {
                        log!("saves: {} compressed in the background, {before} to {after} bytes, in {:.1?}", path.display(), start.elapsed())
                    }
                    Ok(None) => {}
                    Err(e) => log!("saves: {} stays uncompressed, as the game wrote it: {e}", path.display()),
                }
                let mut count = PENDING.count.lock().unwrap_or_else(|e| e.into_inner());
                *count -= 1;
                PENDING.done.notify_all();
            }
        });
        if let Err(e) = started {
            log!("saves: no thread to compress saves in: {e}");
        }
        Mutex::new(send)
    });
    *PENDING.count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
    if sender.lock().unwrap_or_else(|e| e.into_inner()).send(path).is_err() {
        let mut count = PENDING.count.lock().unwrap_or_else(|e| e.into_inner());
        *count -= 1;
    }
}

/// Returns once every file handed to the thread is done with: before a local
/// save, a load and a restart.
pub fn wait() {
    let start = Instant::now();
    let mut count = PENDING.count.lock().unwrap_or_else(|e| e.into_inner());
    if *count == 0 {
        return;
    }
    WAITED_FOR.store(true, Ordering::SeqCst);
    let _waited = Waited;
    while *count > 0 {
        let (next, timeout) = PENDING.done.wait_timeout(count, Duration::from_secs(60)).unwrap_or_else(|e| e.into_inner());
        count = next;
        if timeout.timed_out() {
            log!("saves: still compressing after a minute; going on without it");
            return;
        }
    }
    log!("saves: waited {:.1?} for the background compression", start.elapsed());
}

/// Compresses the uncompressed entries of the zip at `path` and puts the
/// result in its place; None when it has none, or is no zip (a plain save).
fn compact_file(path: &Path) -> Result<Option<(usize, usize)>> {
    let error = |e: std::io::Error| e.to_string();
    let before = std::fs::metadata(path).map_err(error)?;
    let bytes = std::fs::read(path).map_err(error)?;
    let Some(packed) = compact(&bytes)? else { return Ok(None) };
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(TMP_SUFFIX);
    let tmp = PathBuf::from(tmp);
    let written = (|| {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp).map_err(error)?;
        file.write_all(&packed).map_err(error)?;
        file.sync_all().map_err(error)?;
        drop(file);
        for tried in 1.. {
            let now = std::fs::metadata(path).map_err(error)?;
            if now.len() != before.len() || now.modified().ok() != before.modified().ok() {
                return Err("the file changed while it was compressed".to_string());
            }
            match std::fs::rename(&tmp, path) {
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && tried < MOVE_TRIES && !WAITED_FOR.load(Ordering::SeqCst) => {
                    std::thread::sleep(MOVE_AGAIN)
                }
                done => return done.map_err(error),
            }
        }
        unreachable!()
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written.map(|()| Some((bytes.len(), packed.len())))
}

/// An entry of a zip file and its data as the file holds it: stored, or
/// deflated.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    entry: zip::Entry,
    data: Vec<u8>,
}

/// The entries of a zip file, through its central directory; None when the
/// bytes are no zip file.
fn entries(bytes: &[u8]) -> Result<Option<Vec<Entry>>> {
    if !bytes.starts_with(zip::LOCAL) {
        return Ok(None);
    }
    let mut file = std::io::Cursor::new(bytes);
    let entries = zip::directory(&mut file, bytes.len())?;
    let read = entries.into_iter().map(|entry| Ok(Entry { data: zip::packed(&mut file, &entry)?, entry }));
    read.collect::<Result<_>>().map(Some)
}

/// A zip file of `entries`, in their order.
fn write(entries: &[Entry]) -> Vec<u8> {
    zip::write(entries.iter().map(|e| (&e.entry, e.data.as_slice())))
}

/// Whether an entry's data is not compressed: stored, or deflated at level 0,
/// whose first block (like all its blocks) copies the data: block type 0,
/// in bits 1 and 2 of its first byte.
fn uncompressed(e: &Entry) -> bool {
    e.entry.method == STORED || (e.entry.method == DEFLATED && e.data.first().is_some_and(|b| (b >> 1) & 3 == 0))
}

/// What an entry holds, checked against its CRC and, for a save's own
/// entries, against the header the game begins them with.
fn contents(e: &Entry) -> Result<Vec<u8>> {
    let data = zip::unpack(&e.entry, &e.data, usize::MAX)?;
    if SAVE_ENTRIES.contains(&e.entry.name.as_slice()) && !data.starts_with(HEADER) {
        return Err(format!("{} does not begin as a save does", String::from_utf8_lossy(&e.entry.name)));
    }
    Ok(data)
}

/// The zip file `bytes` with its uncompressed entries deflated, checked by
/// reading every entry back; None when it is no zip file or has nothing
/// uncompressed.
fn compact(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    let Some(mut entries) = entries(bytes)? else { return Ok(None) };
    if !entries.iter().any(uncompressed) {
        return Ok(None);
    }
    for e in &entries {
        contents(e)?;
    }
    for e in entries.iter_mut().filter(|e| uncompressed(e)) {
        e.data = miniz_oxide::deflate::compress_to_vec(&contents(e)?, LEVEL);
        e.entry.method = DEFLATED;
        e.entry.packed = e.data.len();
    }
    let packed = write(&entries);
    // Every entry read back, as the game will read it.
    let read = self::entries(&packed)?.ok_or("the zip written is no zip")?;
    if read.len() != entries.len() {
        return Err("the zip written lost an entry".into());
    }
    for e in &read {
        contents(e).map_err(|e| format!("read back: {e}"))?;
    }
    Ok(Some(packed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saving::stored::{stored_blocks, STORED_BLOCK};

    fn stored(name: &str, data: &[u8]) -> Entry {
        let entry = zip::Entry { time: 0x6a2b, date: 0x5b64, attributes: 0x20, ..zip::Entry::stored(name.as_bytes(), data) };
        Entry { entry, data: data.to_vec() }
    }

    fn texts() -> Vec<(&'static str, Vec<u8>)> {
        let gamestate: Vec<u8> = (0..2000).flat_map(|i| format!("province={i}\n{{\n\towner=\"FRA\"\n}}\n").into_bytes()).collect();
        let gamestate = [b"EU4txt\n".to_vec(), gamestate].concat();
        vec![("meta", b"EU4txt\ndate=1444.11.11\n".to_vec()), ("gamestate", gamestate), ("ai", b"EU4txt\nai={ }\n".to_vec())]
    }

    #[test]
    fn a_stored_save_is_deflated_and_reads_back_whole() {
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| stored(n, d)).collect();
        let packed = compact(&write(&entries)).unwrap().unwrap();
        assert!(packed.len() < write(&entries).len() / 4);
        let read = super::entries(&packed).unwrap().unwrap();
        assert_eq!(read.len(), 3);
        for ((name, data), e) in texts().iter().zip(&read) {
            let z = &e.entry;
            assert_eq!((z.name.as_slice(), z.method, z.time, z.date, z.attributes), (name.as_bytes(), DEFLATED, 0x6a2b, 0x5b64, 0x20));
            assert_eq!(&zip::inflate(&e.data, data.len()).unwrap(), data);
        }
    }

    #[test]
    fn the_frameworks_reader_reads_what_was_compressed() {
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| stored(n, d)).collect();
        let packed = compact(&write(&entries)).unwrap().unwrap();
        let dir = std::env::temp_dir().join(format!("eu4harmony-background-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.eu4");
        std::fs::write(&path, &packed).unwrap();
        let header = crate::saves::file::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(header.date.is_some());
    }

    /// An entry deflated at level 0, as the game writes it during a local save.
    fn copied(name: &str, data: &[u8]) -> Entry {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 0);
        let entry = zip::Entry { method: DEFLATED, packed: packed.len(), ..stored(name, data).entry };
        Entry { entry, data: packed }
    }

    #[test]
    fn entries_deflated_at_level_0_are_deflated_again() {
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| copied(n, d)).collect();
        assert!(entries.iter().all(uncompressed));
        let packed = compact(&write(&entries)).unwrap().unwrap();
        let read = super::entries(&packed).unwrap().unwrap();
        assert!(!read.iter().any(uncompressed));
        for ((_, data), e) in texts().iter().zip(&read) {
            assert_eq!(&contents(e).unwrap(), data);
        }
    }

    #[test]
    fn stored_blocks_read_back_as_the_data_and_count_as_uncompressed() {
        let data: Vec<u8> = (0u32..200_000).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect();
        for len in [0, 1, STORED_BLOCK, STORED_BLOCK + 1, 200_000] {
            let mut out = Vec::new();
            // In two batches, as the framework writes an entry.
            let (a, b) = data[..len].split_at(len / 2);
            stored_blocks(a, false, &mut out);
            stored_blocks(b, true, &mut out);
            assert_eq!(zip::inflate(&out, len).unwrap(), &data[..len], "{len} bytes");
            let entry = Entry { entry: zip::Entry { method: DEFLATED, ..stored("gamestate", &data[..len]).entry }, data: out };
            assert!(uncompressed(&entry));
        }
    }

    #[test]
    fn a_save_of_zeros_is_refused_though_its_crc_matches() {
        // What the game's stored method wrote when it was tried.
        let zeros = vec![0u8; 50_000];
        let entries = [copied("meta", &zeros), copied("gamestate", &zeros), copied("ai", &zeros)];
        assert!(compact(&write(&entries)).unwrap_err().contains("does not begin as a save does"));
    }

    #[test]
    fn a_compressed_zip_and_a_plain_save_are_left_as_they_are() {
        let mut deflated = stored("meta", b"EU4txt\n");
        deflated.data = miniz_oxide::deflate::compress_to_vec(&deflated.data, 3);
        deflated.entry.method = DEFLATED;
        assert_eq!(compact(&write(&[deflated])).unwrap(), None);
        assert_eq!(compact(b"EU4txt\ndate=1444.11.11\n").unwrap(), None);
    }

    #[test]
    fn a_damaged_entry_is_refused() {
        let mut e = copied("gamestate", b"EU4txt\nprovinces={ }\n");
        e.entry.crc ^= 1;
        assert!(compact(&write(&[e])).is_err());
        let mut cut = write(&[stored("meta", b"EU4txt\n")]);
        cut.truncate(cut.len() - 10);
        assert!(compact(&cut).is_err());
    }

    #[test]
    fn a_file_is_compressed_in_place() {
        let dir = std::env::temp_dir().join(format!("eu4harmony-compact-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("autosave.eu4");
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| stored(n, d)).collect();
        std::fs::write(&path, write(&entries)).unwrap();
        let (before, after) = compact_file(&path).unwrap().unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len() as usize, after);
        assert!(after < before);
        // Compressed already: left alone.
        assert_eq!(compact_file(&path).unwrap(), None);
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(left, ["autosave.eu4"]);
    }

    #[test]
    fn only_the_threads_temporary_files_are_removed_at_startup() {
        let dir = std::env::temp_dir().join(format!("eu4harmony-leftovers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["Austria.eu4", "Austria.eu4.eu4harmony-tmp", "Castilla.tmp", "autosave.eu4.eu4harmony-tmp"] {
            std::fs::write(dir.join(name), b"PK").unwrap();
        }
        let removed = remove_leftovers(&dir);
        let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        left.sort();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(left, ["Austria.eu4", "Castilla.tmp"]);
        assert_eq!(remove_leftovers(&dir), 0);
    }
}
