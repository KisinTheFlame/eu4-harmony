//! Extension save data: written next to a save after the game finishes writing
//! it, bound to the save's full SHA-256, absolute path and game date, and read
//! back when that exact save is loaded. The `.eu4` file is never changed.
//!
//! The observers follow the game's save pipeline (save request, local writer,
//! file close, final move to the absolute path) and its loader, as recovered for
//! eu4-regional-combat (docs/research/regional-combat/native-save-findings.txt).
use crate::{hooks::HookFrame, memory::Memory, points::Point, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

const SAVE: usize = 0x5cbde0;
const LOCAL_NORMAL: usize = 0x5c6500;
const LOCAL_IRONMAN: usize = 0x5c7ac0;
const FINAL_MOVE: usize = 0x1715bb0;
const ABSOLUTE_PATH: usize = 0x1a5ed40;
const LOAD: usize = 0x5cdc60;
/// Return addresses that tie a file close or move to the save writer that made it.
const CLOSE_FROM_NORMAL: usize = 0x5c6c51;
const CLOSE_FROM_IRONMAN: usize = 0x5c820f;
const MOVE_FROM_NORMAL: usize = 0x5c788d;
const MOVE_FROM_IRONMAN: usize = 0x5c88f4;
const ABSOLUTE_FROM_MOVE: usize = 0x171614f;

const MAX_SAVE: u64 = 512 * 1024 * 1024;
const MAX_JSON: usize = 16 * 1024 * 1024;
const IO_BUDGET: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    absolute_path: String,
    sha256: String,
    size: u64,
}

/// The file that holds the extension save data of one save.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveDataFile {
    schema: u32,
    identity: Identity,
    date: i32,
    /// Extension id -> its data.
    extensions: BTreeMap<String, Value>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Lower-cased absolute path without the `\\?\` prefix, as the identity key.
fn slot(path: &Path) -> Result<String> {
    let mut s = path.to_str().ok_or("non-Unicode save path")?.to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        s = format!(r"\\{rest}");
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_string();
    }
    Ok(s.replace('/', "\\").to_lowercase())
}

/// A save file opened without write/delete sharing while it is hashed.
struct LockedSave {
    _file: File,
    identity: Identity,
}

impl LockedSave {
    fn open(path: &Path) -> Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        if !path.is_absolute() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("eu4")) {
            return Err(format!("not an absolute .eu4 path: {}", path.display()));
        }
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new().read(true).share_mode(1).open(&path).map_err(|e| format!("open save: {e}"))?;
        let before = file.metadata().map_err(|e| e.to_string())?;
        if before.len() == 0 || before.len() > MAX_SAVE {
            return Err("save size out of bounds".into());
        }
        let started = Instant::now();
        let mut sha = Sha256::new();
        let mut count = 0u64;
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            if started.elapsed() > IO_BUDGET {
                return Err("hashing the save took too long".into());
            }
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            count += n as u64;
            sha.update(&buf[..n]);
        }
        let after = file.metadata().map_err(|e| e.to_string())?;
        if count != before.len() || after.len() != before.len() || before.modified().ok() != after.modified().ok() {
            return Err("save changed while hashing".into());
        }
        Ok(Self { _file: file, identity: Identity { absolute_path: slot(&path)?, sha256: hex(&sha.finalize()), size: count } })
    }
}

struct Store {
    directory: PathBuf,
}

impl Store {
    fn path(&self, id: &Identity) -> PathBuf {
        self.directory.join(format!("{}-{}.json", hex(&Sha256::digest(id.absolute_path.as_bytes())), id.sha256))
    }

    fn read(&self, id: &Identity, date: i32) -> Result<Option<BTreeMap<String, Value>>> {
        let raw = match fs::read(self.path(id)) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if raw.len() > MAX_JSON {
            return Err("save data file too large".into());
        }
        let value: SaveDataFile = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if value.schema != 1 || value.identity != *id || value.date != date {
            return Err("save data belongs to a different save or date".into());
        }
        Ok(Some(value.extensions))
    }

    fn write(&self, value: &SaveDataFile) -> Result<PathBuf> {
        let data = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if data.len() > MAX_JSON {
            return Err("save data too large".into());
        }
        fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        let temp = self.directory.join(format!(".eu4h-{}.tmp", std::process::id()));
        let dest = self.path(&value.identity);
        let result = (|| {
            let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(&temp).map_err(|e| e.to_string())?;
            f.write_all(&data).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            drop(f);
            replace_file(&temp, &dest)
        })();
        let _ = fs::remove_file(&temp);
        result.map(|_| dest)
    }
}

fn replace_file(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

struct Context {
    memory: Memory,
    store: Store,
}
static CONTEXT: OnceLock<Context> = OnceLock::new();
static LOAD_DEPTH: AtomicUsize = AtomicUsize::new(0);

/// Data loaded with the last save, waiting for the session that loaded it.
static LOADED: Mutex<Option<BTreeMap<String, Value>>> = Mutex::new(None);
/// Current data of every extension, captured at save time.
static CURRENT: Mutex<BTreeMap<String, Value>> = Mutex::new(BTreeMap::new());

pub fn load_in_progress() -> bool {
    LOAD_DEPTH.load(Ordering::SeqCst) != 0
}

/// Data the last load brought along, if the session has not taken it yet.
pub fn take_loaded() -> Option<BTreeMap<String, Value>> {
    LOADED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn get(id: &str) -> Option<Value> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned()
}

pub fn set(id: &str, value: Option<Value>) {
    let mut current = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    match value {
        Some(v) => current.insert(id.to_string(), v),
        None => current.remove(id),
    };
}

/// Replaces all extensions' data at a session start.
pub fn replace_all(data: BTreeMap<String, Value>) {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = data;
}

fn world_date(m: &Memory) -> Result<(usize, i32, u8)> {
    use crate::objects::campaign::{globals, world};
    let at = m.ptr(m.at(globals::WORLD))?;
    if at == 0 {
        return Err("no world".into());
    }
    Ok((at, m.i32(at + world::DATE)?, m.u8(at + world::DAILY_UPDATE)?))
}

fn wide_cstring(m: &Memory, p: usize) -> Result<String> {
    let mut chars = Vec::new();
    for n in 0..4096 {
        let c = m.u16(p + n * 2)?;
        if c == 0 {
            return String::from_utf16(&chars).map_err(|e| e.to_string());
        }
        chars.push(c);
    }
    Err("path too long".into())
}

fn cstring(m: &Memory, p: usize) -> Result<String> {
    let mut bytes = Vec::new();
    for n in 0..4096 {
        let c = m.u8(p + n)?;
        if c == 0 {
            return String::from_utf8(bytes).map_err(|e| e.to_string());
        }
        bytes.push(c);
    }
    Err("path too long".into())
}

// Per-thread record of the save operations in progress, matched by hook slot.
type SaveRef = Rc<RefCell<SaveTxn>>;
type LocalRef = Rc<RefCell<LocalTxn>>;
type MoveRef = Rc<RefCell<MoveTxn>>;
struct SaveTxn {
    request: usize,
    world: (usize, i32, u8),
    data: BTreeMap<String, Value>,
    locals: Vec<LocalRef>,
    moves: Vec<MoveRef>,
}
struct LocalTxn {
    save: Weak<RefCell<SaveTxn>>,
    ironman: bool,
    success: bool,
    closed: bool,
}
struct MoveTxn {
    local: LocalRef,
    absolute: Option<PathBuf>,
    absolute_ok: bool,
    helper_ok: bool,
}
struct LoadTxn {
    world: usize,
    success: usize,
    locked: Option<LockedSave>,
    error: Option<String>,
}
enum Record {
    Ignored,
    Save(SaveRef),
    Local(LocalRef),
    Close(Option<LocalRef>),
    Move(MoveRef),
    Absolute(Option<(MoveRef, PathBuf)>),
    Load(LoadTxn),
}
thread_local! { static STACK: RefCell<Vec<(usize, Record)>> = const { RefCell::new(Vec::new()) }; }

fn ancestor<T>(pick: impl Fn(&Record) -> Option<T>) -> Option<T> {
    STACK.with(|s| s.borrow().iter().rev().find_map(|(_, r)| pick(r)))
}

fn context() -> Result<&'static Context> {
    CONTEXT.get().ok_or_else(|| "save data not initialized".to_string())
}

fn before(frame: *mut HookFrame, f: impl FnOnce(&HookFrame) -> Result<Record>) {
    crate::guarded("save observer", || {
        let h = unsafe { &*frame };
        STACK.with(|s| s.borrow_mut().push((h.slot, Record::Ignored)));
        match f(h) {
            Ok(record) => STACK.with(|s| {
                if let Some((slot, r)) = s.borrow_mut().last_mut() {
                    if *slot == h.slot {
                        *r = record;
                    }
                }
            }),
            Err(e) => log!("saves: {e}"),
        }
    });
}

fn after(frame: *mut HookFrame, f: impl FnOnce(&HookFrame, Record) -> Result<()>) {
    crate::guarded("save observer", || {
        let h = unsafe { &*frame };
        let record = STACK.with(|s| match s.borrow_mut().pop() {
            Some((slot, r)) if slot == h.slot => Ok(r),
            _ => Err("save observer nesting mismatch".to_string()),
        });
        let result = record.and_then(|r| if h.fault != 0 { Err("native call unwound; save evidence discarded".into()) } else { f(h, r) });
        if let Err(e) = result {
            log!("saves: {e}");
        }
    });
}

extern "C" fn save_before(frame: *mut HookFrame) {
    before(frame, |h| {
        let c = context()?;
        let world = world_date(&c.memory)?;
        let data = CURRENT.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Ok(Record::Save(Rc::new(RefCell::new(SaveTxn { request: h.args[0], world, data, locals: vec![], moves: vec![] }))))
    });
}

extern "C" fn save_after(frame: *mut HookFrame) {
    after(frame, |h, r| {
        let Record::Save(save) = r else { return Ok(()) };
        let save = save.borrow();
        let c = context()?;
        if save.data.is_empty() {
            return Ok(());
        }
        if h.result as u32 != 0 || world_date(&c.memory)? != save.world || save.locals.len() != 1 || save.moves.len() != 1 {
            return Err("incomplete evidence that the save was written".into());
        }
        let local = save.locals[0].borrow();
        let moved = save.moves[0].borrow();
        if !Rc::ptr_eq(&moved.local, &save.locals[0]) || !local.success || !local.closed || !moved.absolute_ok || !moved.helper_ok {
            return Err("save writer, close or final move failed".into());
        }
        let path = moved.absolute.as_ref().ok_or("no absolute save path")?;
        let locked = LockedSave::open(path)?;
        let value = SaveDataFile { schema: 1, identity: locked.identity.clone(), date: save.world.1, extensions: save.data.clone() };
        let dest = c.store.write(&value)?;
        log!("saves: wrote extension data for {} to {}", locked.identity.absolute_path, dest.display());
        Ok(())
    });
}

fn local_before(frame: *mut HookFrame, ironman: bool) {
    before(frame, |h| {
        let Some(save) = ancestor(|r| if let Record::Save(x) = r { Some(x.clone()) } else { None }) else {
            return Ok(Record::Ignored);
        };
        if save.borrow().request != h.args[0] {
            return Err("save request pointer mismatch".into());
        }
        let local = Rc::new(RefCell::new(LocalTxn { save: Rc::downgrade(&save), ironman, success: false, closed: false }));
        save.borrow_mut().locals.push(local.clone());
        Ok(Record::Local(local))
    });
}
extern "C" fn normal_before(f: *mut HookFrame) {
    local_before(f, false)
}
extern "C" fn ironman_before(f: *mut HookFrame) {
    local_before(f, true)
}
extern "C" fn local_after(f: *mut HookFrame) {
    after(f, |h, r| {
        if let Record::Local(x) = r {
            x.borrow_mut().success = h.result as u32 == 0;
        }
        Ok(())
    });
}

/// `PHYSFS_close`; shared with the PhysFS splicer, which calls these.
pub fn close_before(f: *mut HookFrame) {
    before(f, |h| {
        let c = context()?;
        let local = ancestor(|r| if let Record::Local(x) = r { Some(x.clone()) } else { None }).filter(|l| {
            h.return_address == c.memory.at(if l.borrow().ironman { CLOSE_FROM_IRONMAN } else { CLOSE_FROM_NORMAL })
        });
        Ok(Record::Close(local))
    });
}
pub fn close_after(f: *mut HookFrame) {
    after(f, |h, r| {
        if let Record::Close(Some(x)) = r {
            x.borrow_mut().closed = h.result as u32 == 1;
        }
        Ok(())
    });
}

extern "C" fn move_before(f: *mut HookFrame) {
    before(f, |h| {
        let c = context()?;
        let Some(local) = ancestor(|r| if let Record::Local(x) = r { Some(x.clone()) } else { None }) else {
            return Ok(Record::Ignored);
        };
        let expected = if local.borrow().ironman { MOVE_FROM_IRONMAN } else { MOVE_FROM_NORMAL };
        if h.return_address != c.memory.at(expected) {
            return Ok(Record::Ignored);
        }
        let save = local.borrow().save.upgrade().ok_or("save record expired")?;
        let moved = Rc::new(RefCell::new(MoveTxn { local, absolute: None, absolute_ok: false, helper_ok: false }));
        save.borrow_mut().moves.push(moved.clone());
        Ok(Record::Move(moved))
    });
}
extern "C" fn move_after(f: *mut HookFrame) {
    after(f, |h, r| {
        if let Record::Move(x) = r {
            x.borrow_mut().helper_ok = h.result & 255 == 1;
        }
        Ok(())
    });
}

extern "C" fn absolute_before(f: *mut HookFrame) {
    before(f, |h| {
        let c = context()?;
        if h.return_address != c.memory.at(ABSOLUTE_FROM_MOVE) {
            return Ok(Record::Absolute(None));
        }
        let Some(m) = ancestor(|r| if let Record::Move(x) = r { Some(x.clone()) } else { None }) else {
            return Ok(Record::Absolute(None));
        };
        Ok(Record::Absolute(Some((m, PathBuf::from(wide_cstring(&c.memory, h.args[1])?)))))
    });
}
extern "C" fn absolute_after(f: *mut HookFrame) {
    after(f, |h, r| {
        if let Record::Absolute(Some((m, path))) = r {
            let mut m = m.borrow_mut();
            m.absolute = Some(path);
            m.absolute_ok = h.result as u32 == 0;
        }
        Ok(())
    });
}

extern "C" fn load_before(f: *mut HookFrame) {
    LOAD_DEPTH.fetch_add(1, Ordering::SeqCst);
    *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    crate::game::events::load_begin();
    crate::remote::campaign_changes();
    before(f, |h| {
        let c = context()?;
        let mut txn = LoadTxn { world: h.args[2], success: h.args[1], locked: None, error: None };
        let binding = (|| {
            let file = c.memory.ptr(h.args[0])?;
            let (path, data) = engine_path(&c.memory, file + 0x18)?;
            let root = c.memory.call(crate::physfs::real_dir(), &[data])?;
            if root == 0 {
                return Err("loaded path has no real directory".to_string());
            }
            LockedSave::open(&PathBuf::from(cstring(&c.memory, root)?).join(path))
        })();
        match binding {
            Ok(locked) => txn.locked = Some(locked),
            Err(e) => txn.error = Some(e),
        }
        Ok(Record::Load(txn))
    });
}

extern "C" fn load_after(f: *mut HookFrame) {
    struct Exit;
    impl Drop for Exit {
        fn drop(&mut self) {
            let _ = LOAD_DEPTH.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| Some(n.saturating_sub(1)));
        }
    }
    let _exit = Exit;
    after(f, |h, r| {
        let Record::Load(txn) = r else { return Ok(()) };
        let c = context()?;
        if let Some(e) = txn.error {
            return Err(format!("loaded save not identified: {e}"));
        }
        let (world, date, daily) = world_date(&c.memory)?;
        if h.result != txn.success || c.memory.u8(txn.success)? != 1 || world != txn.world || daily != 0 {
            return Err("load did not establish the current world".into());
        }
        let locked = txn.locked.ok_or("no loaded file")?;
        let data = c.store.read(&locked.identity, date)?;
        log!("saves: loaded {} with data for {} extension(s)", locked.identity.absolute_path, data.as_ref().map_or(0, |d| d.len()));
        *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = Some(data.unwrap_or_default());
        Ok(())
    });
}

fn engine_path(m: &Memory, p: usize) -> Result<(String, usize)> {
    let len = m.u64(p + 16)? as usize;
    let cap = m.u64(p + 24)? as usize;
    if len == 0 || len > 4095 || cap < len || cap > 1_048_576 {
        return Err("engine path bounds".into());
    }
    let data = if cap <= 15 { p } else { m.ptr(p)? };
    if m.u8(data + len)? != 0 {
        return Err("engine path terminator".into());
    }
    let s = String::from_utf8(m.bytes(data, len)?).map_err(|e| e.to_string())?;
    Ok((s, data))
}

/// Sets where data files go; the user directory is only known after bootstrap.
pub fn initialize(memory: Memory, directory: PathBuf) -> Result<()> {
    CONTEXT.set(Context { memory, store: Store { directory } }).map_err(|_| "save data already initialized".to_string())
}

/// The save and load functions the tracker observes, and the calls whose
/// return addresses tie a file close or move to the writer that made it.
pub fn points() -> Vec<Point> {
    vec![
        Point::observe(SAVE, "48 89 5c 24 10 57 48 83 ec 20 48 8b f9 e8 be 49", "save request", 1, Some(save_before), Some(save_after)),
        Point::observe(LOCAL_NORMAL, "48 89 5c 24 10 48 89 74 24 18 57 41 54 41 55 41", "normal save writer", 1, Some(normal_before), Some(local_after)),
        Point::observe(LOCAL_IRONMAN, "48 89 5c 24 18 48 89 74 24 20 57 41 54 41 55 41", "ironman save writer", 1, Some(ironman_before), Some(local_after)),
        Point::observe(FINAL_MOVE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20 55", "save final move", 2, Some(move_before), Some(move_after)),
        Point::observe(ABSOLUTE_PATH, "48 83 ec 28 41 b8 02 00 00 00 ff 15 90 76 10 00", "save absolute move", 2, Some(absolute_before), Some(absolute_after)),
        Point::observe(LOAD, "48 89 5c 24 18 55 56 57 48 8b ec 48 81 ec 80 00", "save loader", 4, Some(load_before), Some(load_after)),
        Point::keep(CLOSE_FROM_NORMAL - 5, "e8 6f fa 42 01", "normal save writer closes its file"),
        Point::keep(CLOSE_FROM_IRONMAN - 5, "e8 b1 e4 42 01", "ironman save writer closes its file"),
        Point::keep(MOVE_FROM_NORMAL - 5, "e8 23 e3 14 01", "normal save writer moves its file"),
        Point::keep(MOVE_FROM_IRONMAN - 5, "e8 bc d2 14 01", "ironman save writer moves its file"),
        Point::keep(ABSOLUTE_FROM_MOVE - 5, "e8 f1 8b 34 00", "final move makes the path absolute"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_round_trip_requires_identity_and_date() {
        let dir = std::env::temp_dir().join(format!("eu4h-saves-test-{}", std::process::id()));
        let store = Store { directory: dir.clone() };
        let id = Identity { absolute_path: r"c:\saves\a.eu4".into(), sha256: "ab".repeat(32), size: 10 };
        let mut ext = BTreeMap::new();
        ext.insert("regional-combat".to_string(), serde_json::json!({"armies": [1, 2]}));
        store.write(&SaveDataFile { schema: 1, identity: id.clone(), date: 7, extensions: ext.clone() }).unwrap();
        assert_eq!(store.read(&id, 7).unwrap(), Some(ext));
        assert!(store.read(&id, 8).is_err());
        let other = Identity { sha256: "cd".repeat(32), ..id };
        assert_eq!(store.read(&other, 7).unwrap(), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn slot_normalizes_verbatim_prefix_and_case() {
        assert_eq!(slot(Path::new(r"\\?\C:\Saves\A.eu4")).unwrap(), r"c:\saves\a.eu4");
    }
}
