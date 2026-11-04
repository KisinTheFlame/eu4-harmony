//! Saves compressed in the background (docs/game-speed.md, S3).
//!
//! The game compresses a save's entries into its zip on the main thread, at
//! zlib's level 3: about 40% of an ironman autosave's time late in a game
//! (docs/research/main-thread.md). While one of the local saves runs (the
//! save `0x5c6500` and the ironman save `0x5c7ac0`), the framework has the
//! game deflate them at level 0, which copies the data into stored blocks
//! without compressing it, so the game writes, checks and moves its file as it
//! always does, only sooner. (Not the zip's method 0, "stored": the game's
//! code for it reads its data from where the stream stands, after the end,
//! and wrote a save of zeros when it was tried; docs/prototype-log.md.)
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
//! The game's deflate at level 0 still passes the data through zlib and takes
//! its CRC a byte at a time, a quarter of an autosave's stop
//! (docs/research/saves.md, "Where a save's zip goes"). So at level 0 the
//! framework copies an entry's data into the zip's stored blocks itself, its
//! CRC taken eight bytes at a step: the same entry, framed in blocks of its
//! own.
use crate::game::menu::save_file::crc32_update;
use crate::hooks::HookFrame;
use crate::memory::Memory;
use crate::points::{Function, Point};
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
/// `(zip, data stream, buffer, its size, &crc, level) -> bytes written`: an
/// entry's data deflated into the zip's stream (`[zip]`), from the start of
/// the data stream to its size, its CRC carried in the register at `crc`
/// (`!0` at the start), which it leaves inverted. Where it reads the stream's
/// size, moves to a place in it and reads it, and where it writes.
const DEFLATE: usize = 0x153c790;
const DEFLATE_READS: usize = 0x153c834;
const DEFLATE_WRITES: usize = 0x153c8f2;
/// A stream's methods: its size, a move to a place, a read, a write.
const STREAM_SIZE: usize = 0x88;
const STREAM_SEEK: usize = 0x60;
const STREAM_READ: usize = 0x20;
const STREAM_WRITE: usize = 0x50;
/// A stored block holds at most this many bytes; the framework reads and
/// writes sixteen at a time.
const STORED_BLOCK: usize = 0xffff;
const STORED_BATCH: usize = 16 * STORED_BLOCK;

const STORED: u16 = 0;
const DEFLATED: u16 = 8;
/// The level the save asks for, and the one it gets during a local save.
const GAME_LEVEL: usize = 3;
const COPY_LEVEL: usize = 0;

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
/// Where the game keeps its saves, in the user folder.
const SAVE_GAMES: &str = "save games";

pub fn points() -> Vec<Point> {
    vec![
        Point::observe(LOCAL_SAVE, "48 89 5c 24 10 48 89 74 24 18 57 41 54 41 55", "local save", 1, Some(save_before), Some(save_after)),
        Point::observe(IRONMAN_SAVE, "48 89 5c 24 18 48 89 74 24 20 57 41 54 41 55", "local ironman save", 1, Some(save_before), Some(save_after)),
        Point::observe(ADD_ENTRY, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18", "zip entry", 5, Some(entry_before), None),
        Point::observe(MOVE_FILE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20", "file move", 2, None, Some(moved)),
        Point::replace(DEFLATE, "48 8b c4 48 89 58 10 48 89 68 18 48 89 70 20 48", "zip deflate", deflate as Deflate, &ORIGINAL_DEFLATE),
        Point::keep(
            DEFLATE_READS,
            "49 8b 1e 49 8b ce ff 93 88 00 00 00 2b c7 49 8b ce 8b d0 ff 53 60 49 8b 06 41 3b fc 41 8b dc 48 8b d5 0f 4c df 49 8b ce 44 8b c3 ff 50 20",
            "zip deflate reads its data",
        )
        .uses(&[STREAM_SIZE, STREAM_SEEK, STREAM_READ]),
        Point::keep(DEFLATE_WRITES, "48 8b 84 24 d0 00 00 00 49 8d 14 2c 03 f3 4c 63 c3 48 8b 08 48 8b 01 ff 50 50", "zip deflate writes")
            .uses(&[STREAM_WRITE]),
    ]
}

type Deflate = extern "C" fn(usize, usize, usize, usize, *mut u32, i32) -> u32;
static ORIGINAL_DEFLATE: Function<Deflate> = Function::new();

/// The game's deflate, but at the level a local save's entries get, which
/// copies, the framework's copy into stored blocks.
extern "C" fn deflate(zip: usize, data: usize, buffer: usize, size: usize, crc: *mut u32, level: i32) -> u32 {
    if level != COPY_LEVEL as i32 || crate::saves::game_writes_saves() {
        return ORIGINAL_DEFLATE.get()(zip, data, buffer, size, crc, level);
    }
    crate::guarded("zip deflate", || match store(crate::hooked_memory(), zip, data, crc) {
        Ok(written) => written,
        Err(e) => {
            // What the game's deflate answers when zlib fails.
            log!("saves: an entry could not be stored: {e}");
            0
        }
    })
}

/// Copies the data stream into the zip's stream as stored blocks; the bytes
/// written.
fn store(m: Memory, zip: usize, data: usize, crc: *mut u32) -> Result<u32> {
    let size = m.call(m.vslot(data, STREAM_SIZE)?, &[data])? as u32 as usize;
    m.call(m.vslot(data, STREAM_SEEK)?, &[data, 0])?;
    let out = m.ptr(zip)?;
    let (read, write) = (m.vslot(data, STREAM_READ)?, m.vslot(out, STREAM_WRITE)?);
    let mut register = unsafe { *crc };
    let mut input = vec![0u8; STORED_BATCH];
    let mut output = Vec::with_capacity(STORED_BATCH + STORED_BATCH / STORED_BLOCK * 5 + 5);
    let (mut left, mut written) = (size, 0usize);
    loop {
        let n = left.min(STORED_BATCH);
        m.call(read, &[data, input.as_mut_ptr() as usize, n])?;
        register = crc32_update(register, &input[..n]);
        left -= n;
        output.clear();
        stored_blocks(&input[..n], left == 0, &mut output);
        m.call(write, &[out, output.as_ptr() as usize, output.len()])?;
        written += output.len();
        if left == 0 {
            break;
        }
    }
    unsafe { *crc = !register };
    u32::try_from(written).map_err(|_| "an entry of 4 GB or more".to_string())
}

/// `data` as deflate's stored blocks, the last of them marked final when
/// `last`.
fn stored_blocks(data: &[u8], last: bool, out: &mut Vec<u8>) {
    if data.is_empty() {
        if last {
            out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
        }
        return;
    }
    let mut blocks = data.chunks(STORED_BLOCK).peekable();
    while let Some(block) = blocks.next() {
        let len = block.len() as u16;
        out.push(u8::from(last && blocks.peek().is_none()));
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
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

extern "C" fn save_before(_: *mut HookFrame) {
    crate::guarded("local save", || {
        wait();
        MOVED.lock().unwrap_or_else(|e| e.into_inner()).clear();
        SAVING.store(crate::thread_id() == crate::main_thread(), Ordering::SeqCst);
        *BEGAN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    });
}

extern "C" fn save_after(_: *mut HookFrame) {
    crate::guarded("local save", || {
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
    });
}

extern "C" fn entry_before(frame: *mut HookFrame) {
    let h = unsafe { &mut *frame };
    if SAVING.load(Ordering::SeqCst)
        && h.args[3] as u32 == DEFLATED as u32
        && h.args[4] as u32 == GAME_LEVEL as u32
        && crate::thread_id() == crate::main_thread()
    {
        h.args[4] = COPY_LEVEL;
    }
}

extern "C" fn moved(frame: *mut HookFrame) {
    crate::guarded("file move", || {
        let h = unsafe { &*frame };
        if !SAVING.load(Ordering::SeqCst) || h.fault != 0 || h.result as u8 == 0 || h.args[1] == 0 {
            return;
        }
        let to = unsafe { std::ffi::CStr::from_ptr(h.args[1] as *const std::ffi::c_char) };
        match to.to_str() {
            Ok(to) => MOVED.lock().unwrap_or_else(|e| e.into_inner()).push(to.to_string()),
            Err(_) => log!("saves: a file moved to a path that is not UTF-8 stays uncompressed"),
        }
    });
}

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
        let now = std::fs::metadata(path).map_err(error)?;
        if now.len() != before.len() || now.modified().ok() != before.modified().ok() {
            return Err("the file changed while it was compressed".to_string());
        }
        std::fs::rename(&tmp, path).map_err(error)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written.map(|()| Some((bytes.len(), packed.len())))
}

/// An entry of a zip file, as its central directory has it.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    name: Vec<u8>,
    method: u16,
    time: u16,
    date: u16,
    crc: u32,
    unpacked: u32,
    /// Its data as the file holds it: stored, or deflated.
    data: Vec<u8>,
    attributes: u32,
}

fn u16_at(data: &[u8], at: usize) -> Result<u16> {
    data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| "zip file cut short".to_string())
}

fn u32_at(data: &[u8], at: usize) -> Result<u32> {
    data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "zip file cut short".to_string())
}

/// The entries of a zip file, through its central directory; None when the
/// bytes are no zip file.
fn entries(zip: &[u8]) -> Result<Option<Vec<Entry>>> {
    if !zip.starts_with(b"PK\x03\x04") {
        return Ok(None);
    }
    let tail = zip.len().saturating_sub(66_000);
    let end = tail + zip[tail..].windows(4).rposition(|w| w == b"PK\x05\x06").ok_or("no zip directory")?;
    let (count, at) = (u16_at(zip, end + 10)? as usize, u32_at(zip, end + 16)? as usize);
    let mut at = at;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(zip, at)? != 0x0201_4b50 {
            return Err("bad zip directory entry".into());
        }
        let flags = u16_at(zip, at + 8)?;
        if flags & 1 != 0 {
            return Err("an encrypted zip entry".into());
        }
        let (name_len, extra_len, comment_len) = (u16_at(zip, at + 28)? as usize, u16_at(zip, at + 30)? as usize, u16_at(zip, at + 32)? as usize);
        let packed = u32_at(zip, at + 20)? as usize;
        let local = u32_at(zip, at + 42)? as usize;
        let data_at = local + 30 + u16_at(zip, local + 26)? as usize + u16_at(zip, local + 28)? as usize;
        out.push(Entry {
            name: zip.get(at + 46..at + 46 + name_len).ok_or("zip file cut short")?.to_vec(),
            method: u16_at(zip, at + 10)?,
            time: u16_at(zip, at + 12)?,
            date: u16_at(zip, at + 14)?,
            crc: u32_at(zip, at + 16)?,
            unpacked: u32_at(zip, at + 24)?,
            data: zip.get(data_at..data_at + packed).ok_or("zip file cut short")?.to_vec(),
            attributes: u32_at(zip, at + 38)?,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(Some(out))
}

/// A zip file of `entries`, in their order.
fn zip(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.iter().map(|e| e.data.len() + 128).sum());
    let mut directory = Vec::new();
    let common = |out: &mut Vec<u8>, e: &Entry| {
        for v in [20u16, 0, e.method, e.time, e.date] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.crc, e.data.len() as u32, e.unpacked] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
    };
    for e in entries {
        let local = out.len() as u32;
        out.extend_from_slice(b"PK\x03\x04");
        common(&mut out, e);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.data);
        directory.extend_from_slice(b"PK\x01\x02");
        directory.extend_from_slice(&20u16.to_le_bytes());
        common(&mut directory, e);
        for v in [0u16, 0, 0] {
            directory.extend_from_slice(&v.to_le_bytes());
        }
        directory.extend_from_slice(&e.attributes.to_le_bytes());
        directory.extend_from_slice(&local.to_le_bytes());
        directory.extend_from_slice(&e.name);
    }
    let at = out.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(b"PK\x05\x06");
    for v in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Whether an entry's data is not compressed: stored, or deflated at level 0,
/// whose first block (like all its blocks) copies the data: block type 0,
/// in bits 1 and 2 of its first byte.
fn uncompressed(e: &Entry) -> bool {
    e.method == STORED || (e.method == DEFLATED && e.data.first().is_some_and(|b| (b >> 1) & 3 == 0))
}

/// What an entry holds, checked against its CRC and, for a save's own
/// entries, against the header the game begins them with.
fn contents(e: &Entry) -> Result<Vec<u8>> {
    let name = String::from_utf8_lossy(&e.name);
    let data = match e.method {
        STORED => e.data.clone(),
        DEFLATED => crate::game::menu::save_file::inflate(&e.data, e.unpacked as usize)?,
        method => return Err(format!("{name}: zip method {method}")),
    };
    if data.len() != e.unpacked as usize || crate::game::menu::save_file::crc32(&data) != e.crc {
        return Err(format!("{name} does not match its CRC"));
    }
    if SAVE_ENTRIES.contains(&e.name.as_slice()) && !data.starts_with(HEADER) {
        return Err(format!("{name} does not begin as a save does"));
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
        e.method = DEFLATED;
    }
    let packed = zip(&entries);
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
    use crate::game::menu::save_file::crc32;

    fn stored(name: &str, data: &[u8]) -> Entry {
        Entry {
            name: name.into(),
            method: STORED,
            time: 0x6a2b,
            date: 0x5b64,
            crc: crc32(data),
            unpacked: data.len() as u32,
            data: data.to_vec(),
            attributes: 0x20,
        }
    }

    fn texts() -> Vec<(&'static str, Vec<u8>)> {
        let gamestate: Vec<u8> = (0..2000).flat_map(|i| format!("province={i}\n{{\n\towner=\"FRA\"\n}}\n").into_bytes()).collect();
        let gamestate = [b"EU4txt\n".to_vec(), gamestate].concat();
        vec![("meta", b"EU4txt\ndate=1444.11.11\n".to_vec()), ("gamestate", gamestate), ("ai", b"EU4txt\nai={ }\n".to_vec())]
    }

    #[test]
    fn a_stored_save_is_deflated_and_reads_back_whole() {
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| stored(n, d)).collect();
        let packed = compact(&zip(&entries)).unwrap().unwrap();
        assert!(packed.len() < zip(&entries).len() / 4);
        let read = super::entries(&packed).unwrap().unwrap();
        assert_eq!(read.len(), 3);
        for ((name, data), e) in texts().iter().zip(&read) {
            assert_eq!((e.name.as_slice(), e.method, e.time, e.date, e.attributes), (name.as_bytes(), DEFLATED, 0x6a2b, 0x5b64, 0x20));
            assert_eq!(&crate::game::menu::save_file::inflate(&e.data, data.len()).unwrap(), data);
        }
    }

    #[test]
    fn the_frameworks_reader_reads_what_was_compressed() {
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| stored(n, d)).collect();
        let packed = compact(&zip(&entries)).unwrap().unwrap();
        let dir = std::env::temp_dir().join(format!("eu4harmony-background-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.eu4");
        std::fs::write(&path, &packed).unwrap();
        let header = crate::game::menu::save_file::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(header.date.is_some());
    }

    /// An entry deflated at level 0, as the game writes it during a local save.
    fn copied(name: &str, data: &[u8]) -> Entry {
        Entry { method: DEFLATED, data: miniz_oxide::deflate::compress_to_vec(data, 0), ..stored(name, data) }
    }

    #[test]
    fn entries_deflated_at_level_0_are_deflated_again() {
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| copied(n, d)).collect();
        assert!(entries.iter().all(uncompressed));
        let packed = compact(&zip(&entries)).unwrap().unwrap();
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
            assert_eq!(crate::game::menu::save_file::inflate(&out, len).unwrap(), &data[..len], "{len} bytes");
            let entry = Entry { method: DEFLATED, data: out, ..stored("gamestate", &data[..len]) };
            assert!(uncompressed(&entry));
        }
    }

    #[test]
    fn a_save_of_zeros_is_refused_though_its_crc_matches() {
        // What the game's stored method wrote when it was tried.
        let zeros = vec![0u8; 50_000];
        let entries = [copied("meta", &zeros), copied("gamestate", &zeros), copied("ai", &zeros)];
        assert!(compact(&zip(&entries)).unwrap_err().contains("does not begin as a save does"));
    }

    #[test]
    fn a_compressed_zip_and_a_plain_save_are_left_as_they_are() {
        let mut deflated = stored("meta", b"EU4txt\n");
        deflated.data = miniz_oxide::deflate::compress_to_vec(&deflated.data, 3);
        deflated.method = DEFLATED;
        assert_eq!(compact(&zip(&[deflated])).unwrap(), None);
        assert_eq!(compact(b"EU4txt\ndate=1444.11.11\n").unwrap(), None);
    }

    #[test]
    fn a_damaged_entry_is_refused() {
        let mut e = copied("gamestate", b"EU4txt\nprovinces={ }\n");
        e.crc ^= 1;
        assert!(compact(&zip(&[e])).is_err());
        let mut cut = zip(&[stored("meta", b"EU4txt\n")]);
        cut.truncate(cut.len() - 10);
        assert!(compact(&cut).is_err());
    }

    #[test]
    fn a_file_is_compressed_in_place() {
        let dir = std::env::temp_dir().join(format!("eu4harmony-compact-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("autosave.eu4");
        let entries: Vec<Entry> = texts().iter().map(|(n, d)| stored(n, d)).collect();
        std::fs::write(&path, zip(&entries)).unwrap();
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
