//! Hooks on the PhysFS functions that `eu4.exe` exports by name, plus the
//! framework's own in-memory archive (ADR framework-files-in-the-game-file-system).
//!
//! Script files (`.gui`, `.gfx`) are read whole into a buffer before the game's
//! lexer sees them, so the hooks serve a spliced or framework-served copy of
//! chosen files from a buffer of our own. Framework files are packed once into
//! an in-memory zip archive and mounted on every successful `PHYSFS_init` (the
//! game reinitializes PhysFS during startup, dropping earlier mounts).
//!
//! The detours run on whichever thread the game reads files from (GUI
//! definitions load on the game's LoadingThread) and never call extension code.
use crate::base::called_by_game;
use crate::{
    base::Result,
    native::hooks::Call,
    native::memory::Memory,
    native::points::{Function, GameFunction, Point},
    native::splice::{self, Addition, Plan},
};
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr},
    sync::{LazyLock, Mutex, Once, OnceLock},
};

/// One framework file: its path in the game's file system and its bytes.
pub struct File {
    pub path: String,
    pub data: Vec<u8>,
}

type Handle = *mut c_void;

type InitFn = unsafe extern "C" fn(*const c_char) -> i32;
type OpenReadFn = unsafe extern "C" fn(*const c_char) -> Handle;
type FileLengthFn = unsafe extern "C" fn(Handle) -> i64;
type ReadBytesFn = unsafe extern "C" fn(Handle, *mut c_void, u64) -> i64;
type SeekFn = unsafe extern "C" fn(Handle, u64) -> i32;
type TellFn = unsafe extern "C" fn(Handle) -> i64;
type EofFn = unsafe extern "C" fn(Handle) -> i32;
type GetRealDirFn = unsafe extern "C" fn(*const c_char) -> *const c_char;
type CloseFn = unsafe extern "C" fn(Handle) -> i32;
type MountMemoryFn = unsafe extern "C" fn(*const c_void, u64, Option<unsafe extern "C" fn(*mut c_void)>, *const c_char, *const c_char, i32) -> i32;

static ORIG_INIT: Function<InitFn> = Function::new();
static ORIG_OPEN_READ: Function<OpenReadFn> = Function::new();
static ORIG_FILE_LENGTH: Function<FileLengthFn> = Function::new();
static ORIG_READ_BYTES: Function<ReadBytesFn> = Function::new();
static ORIG_SEEK: Function<SeekFn> = Function::new();
static ORIG_TELL: Function<TellFn> = Function::new();
static ORIG_EOF: Function<EofFn> = Function::new();
static GET_REAL_DIR: Function<GetRealDirFn> = Function::new();
static CLOSE: Function<CloseFn> = Function::new();
static MOUNT_MEMORY: Function<MountMemoryFn> = Function::new();

struct Spliced {
    data: Vec<u8>,
    cursor: usize,
}

/// The packed in-memory `eu4harmony.zip` archive, kept alive for the whole
/// process so repeated `PHYSFS_init` calls remount the same slice without
/// allocating or leaking duplicates.
struct Archive {
    bytes: Box<[u8]>,
    paths: Vec<String>,
}

impl Archive {
    fn pack(files: &[File]) -> Self {
        let bytes = zip(files).into_boxed_slice();
        let paths = files.iter().map(|f| f.path.clone()).collect();
        Self { bytes, paths }
    }
}

/// Decision made on `PHYSFS_openRead` before reading the original file.
enum OpenCheck {
    Served(usize),
    NeedsSplice,
    Passthrough,
}

/// Outcome of inspecting or splicing an opened handle.
#[derive(Debug, PartialEq, Eq)]
enum OpenOutcome {
    Passthrough,
    Served(usize),
    Unchanged { rewound: bool },
    Spliced { before: usize, after: usize },
    SpliceFailed { error: String, rewound: bool },
}

/// In-process state machine for virtual PhysFS file overrides, splicing, and
/// open virtual handles.
#[derive(Default)]
struct VirtualFs {
    plan: Option<Plan>,
    open: HashMap<usize, Spliced>,
    /// Files the framework serves with bytes of its own, by normalized path,
    /// whatever the file found holds.
    served: HashMap<String, Vec<u8>>,
}

impl VirtualFs {
    fn set_plan(&mut self, plan: Plan) {
        self.plan = Some(plan);
    }

    fn serve(&mut self, path: &str, data: Vec<u8>) {
        self.served.insert(normalize(path), data);
    }

    fn check_open(&mut self, handle: usize, normalized: &str) -> OpenCheck {
        if let Some(data) = self.served.get(normalized).cloned() {
            let len = data.len();
            self.open.insert(handle, Spliced { data, cursor: 0 });
            return OpenCheck::Served(len);
        }
        if self.plan.as_ref().is_some_and(|p| p.may_add_to(normalized)) {
            OpenCheck::NeedsSplice
        } else {
            OpenCheck::Passthrough
        }
    }

    fn additions_for(&self, normalized: &str, data: &[u8]) -> Vec<Addition> {
        self.plan.as_ref().map(|p| p.for_file(normalized, data)).unwrap_or_default()
    }

    fn insert_open(&mut self, handle: usize, data: Vec<u8>) {
        self.open.insert(handle, Spliced { data, cursor: 0 });
    }

    fn file_length(&self, handle: usize) -> Option<i64> {
        self.open.get(&handle).map(|s| s.data.len() as i64)
    }

    fn read_bytes(&mut self, handle: usize, out: &mut [u8]) -> Option<usize> {
        let s = self.open.get_mut(&handle)?;
        let n = out.len().min(s.data.len().saturating_sub(s.cursor));
        out[..n].copy_from_slice(&s.data[s.cursor..s.cursor + n]);
        s.cursor += n;
        Some(n)
    }

    fn seek(&mut self, handle: usize, pos: u64) -> Option<bool> {
        let s = self.open.get_mut(&handle)?;
        let pos = usize::try_from(pos).ok().filter(|&p| p <= s.data.len());
        match pos {
            Some(p) => {
                s.cursor = p;
                Some(true)
            }
            None => Some(false),
        }
    }

    fn tell(&self, handle: usize) -> Option<i64> {
        self.open.get(&handle).map(|s| s.cursor as i64)
    }

    fn eof(&self, handle: usize) -> Option<bool> {
        self.open.get(&handle).map(|s| s.cursor >= s.data.len())
    }

    fn close(&mut self, handle: usize) {
        self.open.remove(&handle);
    }
}

/// Runs the `PHYSFS_openRead` override and splice protocol against `fs` without
/// holding the mutex while reading or splicing the underlying file.
fn open_handle(
    fs: &Mutex<VirtualFs>,
    handle: usize,
    raw_path: &str,
    read_original: impl FnOnce() -> Result<Vec<u8>>,
    rewind: impl FnOnce() -> bool,
) -> OpenOutcome {
    let normalized = normalize(raw_path);
    match fs.lock().unwrap_or_else(|e| e.into_inner()).check_open(handle, &normalized) {
        OpenCheck::Served(len) => return OpenOutcome::Served(len),
        OpenCheck::Passthrough => return OpenOutcome::Passthrough,
        OpenCheck::NeedsSplice => {}
    }
    let outcome = apply_plan(read_original, |data| fs.lock().unwrap_or_else(|e| e.into_inner()).additions_for(&normalized, data));
    match outcome {
        Ok((_, None)) => OpenOutcome::Unchanged { rewound: rewind() },
        Ok((before, Some(data))) => {
            let after = data.len();
            fs.lock().unwrap_or_else(|e| e.into_inner()).insert_open(handle, data);
            OpenOutcome::Spliced { before, after }
        }
        Err(error) => OpenOutcome::SpliceFailed { error, rewound: rewind() },
    }
}

/// Private-use range used to round-trip raw high bytes (`0x80..=0xFF`) from
/// legacy Windows-1252 `.gui`/`.gfx` files through UTF-8 string splices
/// without altering original bytes or corrupting UTF-8 additions.
const RAW_BYTE_BASE: u32 = 0xE000;

fn decode_splice_source(data: Vec<u8>) -> (String, bool) {
    match String::from_utf8(data) {
        Ok(text) => (text, false),
        Err(err) => {
            let raw = err.into_bytes();
            let mut text = String::with_capacity(raw.len());
            for b in raw {
                if b < 0x80 {
                    text.push(b as char);
                } else {
                    text.push(char::from_u32(RAW_BYTE_BASE + u32::from(b)).expect("valid PUA scalar"));
                }
            }
            (text, true)
        }
    }
}

fn encode_splice_output(text: String, had_raw_bytes: bool) -> Vec<u8> {
    if !had_raw_bytes {
        return text.into_bytes();
    }
    let mut out = Vec::with_capacity(text.len());
    let mut buf = [0u8; 4];
    for ch in text.chars() {
        let cp = ch as u32;
        if (RAW_BYTE_BASE + 0x80..=RAW_BYTE_BASE + 0xFF).contains(&cp) {
            out.push((cp - RAW_BYTE_BASE) as u8);
        } else {
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }
    out
}

fn apply_plan(read_original: impl FnOnce() -> Result<Vec<u8>>, additions_for: impl FnOnce(&[u8]) -> Vec<Addition>) -> Result<(usize, Option<Vec<u8>>)> {
    let data = read_original()?;
    let len = data.len();
    let additions = additions_for(&data);
    if additions.is_empty() {
        return Ok((len, None));
    }
    let (text, had_raw_bytes) = decode_splice_source(data);
    let (spliced, errors) = splice::apply_best_effort(&text, &additions);
    if spliced == text {
        if let Some(first) = errors.into_iter().next() {
            return Err(first);
        }
        return Ok((len, None));
    }
    for error in errors {
        log!("physfs: partial splice error: {error}");
    }
    Ok((len, Some(encode_splice_output(spliced, had_raw_bytes))))
}

static STATE: LazyLock<Mutex<VirtualFs>> = LazyLock::new(|| Mutex::new(VirtualFs::default()));
static ARCHIVE: OnceLock<Archive> = OnceLock::new();
static INIT_ONCE: Once = Once::new();
static ON_FIRST_INIT: OnceLock<fn()> = OnceLock::new();

/// Registers the startup callback invoked once on the first PhysFS call
/// (before mounting the framework's in-memory archive).
pub(crate) fn set_init_hook(on_first_init: fn()) {
    let _ = ON_FIRST_INIT.set(on_first_init);
}

fn run_first_init() {
    if let Some(hook) = ON_FIRST_INIT.get() {
        hook();
    }
}

fn state() -> std::sync::MutexGuard<'static, VirtualFs> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Installs the splice plan; files opened from now on are served spliced.
pub fn set_plan(plan: Plan) {
    state().set_plan(plan);
}

/// Packs and caches the framework's in-memory files once. Every successful
/// `PHYSFS_init` mounts this same archive slice into PhysFS.
pub(crate) fn set_framework_files(files: &[File]) {
    let _ = ARCHIVE.set(Archive::pack(files));
}

/// Mounts the cached framework-files archive into PhysFS when present.
fn mount_cached_archive() {
    let Some(archive) = ARCHIVE.get() else {
        return;
    };
    let mounted =
        unsafe { MOUNT_MEMORY.get()(archive.bytes.as_ptr().cast(), archive.bytes.len() as u64, None, c"eu4harmony.zip".as_ptr(), std::ptr::null(), 1) };
    if mounted == 0 {
        log!("framework files: not mounted; the game reads none of {:?}", archive.paths);
    } else {
        log!("framework files: mounted {} bytes: {:?}", archive.bytes.len(), archive.paths);
    }
}

/// The files packed as a zip archive, each stored as it is.
fn zip(files: &[File]) -> Vec<u8> {
    let entries: Vec<_> = files.iter().map(|f| crate::base::zip::Entry::stored(f.path.as_bytes(), &f.data)).collect();
    crate::base::zip::write(entries.iter().zip(files).map(|(e, f)| (e, f.data.as_slice())))
}

/// Serves the file at `path`, one PhysFS finds (a framework file), with
/// `data` from now on, whatever it holds: what the framework writes after
/// startup for the game to read again (`crate::interface::windows`).
pub fn serve(path: &str, data: Vec<u8>) {
    state().serve(path, data);
}

/// The archive or folder PhysFS reads `path` from, as it names it.
pub(crate) fn real_dir(path: *const c_char) -> Option<String> {
    if GET_REAL_DIR.address() == 0 {
        return None;
    }
    let dir = unsafe { GET_REAL_DIR.get()(path) };
    (!dir.is_null()).then(|| c_str(dir))
}

fn c_str(p: *const c_char) -> String {
    if p.is_null() {
        return "<null>".into();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Lower-cased, forward-slashed, without a leading `/` or `./`.
pub fn normalize(path: &str) -> String {
    let mut s = path.replace('\\', "/").to_ascii_lowercase();
    while let Some(rest) = s.strip_prefix("./").or_else(|| s.strip_prefix('/')) {
        s = rest.to_string();
    }
    s
}

fn dispatch_init(once: &Once, succeeded: bool, on_first_init: impl FnOnce(), mount: impl FnOnce()) {
    once.call_once(on_first_init);
    if succeeded {
        mount();
    }
}

#[called_by_game]
unsafe extern "C" fn init_detour(argv0: *const c_char) -> i32 {
    let result = ORIG_INIT.get()(argv0);
    crate::base::guarded("PHYSFS_init", || dispatch_init(&INIT_ONCE, result != 0, run_first_init, mount_cached_archive));
    result
}

#[called_by_game]
unsafe extern "C" fn open_read_detour(path: *const c_char) -> Handle {
    let handle = ORIG_OPEN_READ.get()(path);
    crate::base::guarded("PHYSFS_openRead", || {
        INIT_ONCE.call_once(run_first_init);
        if handle.is_null() {
            return;
        }
        let raw = c_str(path);
        match open_handle(&STATE, handle as usize, &raw, || read_handle_bytes(handle), || ORIG_SEEK.get()(handle, 0) != 0) {
            OpenOutcome::Passthrough | OpenOutcome::Unchanged { rewound: true } => {}
            OpenOutcome::Served(len) => log!("served {raw:?}: {len} bytes"),
            OpenOutcome::Unchanged { rewound: false } => log!("rewind of the unchanged {raw:?} failed"),
            OpenOutcome::Spliced { before, after } => {
                let real_dir = match GET_REAL_DIR.address() {
                    0 => "<n/a>".into(),
                    _ => c_str(GET_REAL_DIR.get()(path)),
                };
                log!("spliced {raw:?} from {real_dir:?}: {before} -> {after} bytes");
            }
            OpenOutcome::SpliceFailed { error, rewound } => {
                log!("splice of {raw:?} failed, serving the original: {error}");
                if !rewound {
                    log!("rewind after a failed splice also failed");
                }
            }
        }
    });
    handle
}

/// Reads the whole original file through the raw PhysFS trampolines.
fn read_handle_bytes(handle: Handle) -> Result<Vec<u8>> {
    let len = unsafe { ORIG_FILE_LENGTH.get()(handle) };
    if !(0..=64 * 1024 * 1024).contains(&len) {
        return Err(format!("unexpected file length {len}"));
    }
    let mut data = vec![0u8; len as usize];
    let read = unsafe { ORIG_READ_BYTES.get()(handle, data.as_mut_ptr() as _, len as u64) };
    if read != len {
        return Err(format!("short read {read} of {len}"));
    }
    Ok(data)
}

/// Reads a file the game opened, `handle`, from its start, as the game would
/// read it: `count` bytes, or all of them with None, handed to `chunk` a piece
/// at a time. The file is left at the end of what was read.
pub(crate) fn read_from_start(handle: usize, count: Option<u64>, mut chunk: impl FnMut(&[u8])) -> Result<()> {
    let handle = handle as Handle;
    if unsafe { seek_detour(handle, 0) } == 0 {
        return Err("the file could not be rewound".into());
    }
    let mut left = count.unwrap_or(u64::MAX);
    let mut buffer = vec![0u8; 0x40000];
    while left > 0 {
        let want = left.min(buffer.len() as u64);
        let read = unsafe { read_bytes_detour(handle, buffer.as_mut_ptr().cast(), want) };
        if read < 0 {
            return Err("the file could not be read".into());
        }
        if read == 0 {
            break;
        }
        chunk(&buffer[..read as usize]);
        left -= read as u64;
    }
    Ok(())
}

/// A snapshot of a text save through the already open file, leaving its
/// position unchanged even after a failed read. The file stays open.
pub(crate) fn text_snapshot(handle: usize) -> Result<Option<Vec<u8>>> {
    snapshot(&mut Input(handle as Handle)).map_err(|e| format!("the text save could not be buffered: {e}"))
}

struct Input(Handle);

impl std::io::Read for Input {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let read = unsafe { read_bytes_detour(self.0, out.as_mut_ptr().cast(), out.len() as u64) };
        usize::try_from(read).map_err(|_| std::io::Error::other("PhysFS read failed"))
    }
}

impl std::io::Seek for Input {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        use std::io::{Error, SeekFrom};
        let position = match from {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::Current(by) => u64::try_from(unsafe { tell_detour(self.0) }).ok().and_then(|at| at.checked_add_signed(by)),
            SeekFrom::End(by) => u64::try_from(unsafe { file_length_detour(self.0) }).ok().and_then(|at| at.checked_add_signed(by)),
        }
        .ok_or_else(|| Error::other("PhysFS position unavailable"))?;
        if unsafe { seek_detour(self.0, position) } == 0 {
            return Err(Error::other("PhysFS seek failed"));
        }
        Ok(position)
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        u64::try_from(unsafe { tell_detour(self.0) }).map_err(|_| std::io::Error::other("PhysFS position unavailable"))
    }
}

/// Probe before allocating: ZIP and binary saves keep their native paths.
/// The native memory stream's positions and lengths are signed 32-bit.
fn snapshot(input: &mut (impl std::io::Read + std::io::Seek)) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::{Error, ErrorKind, SeekFrom};
    let position = input.stream_position()?;
    let result = (|| {
        input.seek(SeekFrom::Start(0))?;
        let mut header = [0; 6];
        match input.read_exact(&mut header) {
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            other => other?,
        }
        if &header != b"EU4txt" {
            return Ok(None);
        }
        let length = input.seek(SeekFrom::End(0))?;
        let length = i32::try_from(length).map_err(|_| Error::other("save exceeds the memory stream's length range"))? as usize;
        input.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        data.try_reserve_exact(length).map_err(Error::other)?;
        data.resize(length, 0);
        input.read_exact(&mut data)?;
        Ok(Some(data))
    })();
    // Restoration is required on success, refusal and read failure alike.
    input.seek(SeekFrom::Start(position))?;
    result
}

/// Reads the whole of a file in the game's file system, as the game sees it
/// (a mod's file over the game's, a DLC's archive among them), without the
/// framework's splicing. Any thread may call it.
pub(crate) fn read(path: &str) -> Result<Vec<u8>> {
    let name = std::ffi::CString::new(path).map_err(|_| format!("{path:?} holds a NUL"))?;
    let file = unsafe { ORIG_OPEN_READ.get()(name.as_ptr()) };
    if file.is_null() {
        return Err(format!("{path} could not be opened"));
    }
    let length = unsafe { ORIG_FILE_LENGTH.get()(file) };
    let mut data = vec![0u8; usize::try_from(length).unwrap_or(0)];
    let read = unsafe { ORIG_READ_BYTES.get()(file, data.as_mut_ptr().cast(), data.len() as u64) };
    unsafe { CLOSE.get()(file) };
    if length < 0 || read != length {
        return Err(format!("{path} could not be read"));
    }
    Ok(data)
}

#[called_by_game]
unsafe extern "C" fn file_length_detour(handle: Handle) -> i64 {
    if let Some(len) = state().file_length(handle as usize) {
        return len;
    }
    ORIG_FILE_LENGTH.get()(handle)
}

#[called_by_game]
unsafe extern "C" fn read_bytes_detour(handle: Handle, buf: *mut c_void, len: u64) -> i64 {
    {
        let mut st = state();
        if st.file_length(handle as usize).is_some() {
            if buf.is_null() {
                return -1;
            }
            let out = std::slice::from_raw_parts_mut(buf.cast::<u8>(), len as usize);
            if let Some(n) = st.read_bytes(handle as usize, out) {
                return n as i64;
            }
        }
    }
    ORIG_READ_BYTES.get()(handle, buf, len)
}

#[called_by_game]
unsafe extern "C" fn seek_detour(handle: Handle, pos: u64) -> i32 {
    if let Some(ok) = state().seek(handle as usize, pos) {
        return i32::from(ok);
    }
    ORIG_SEEK.get()(handle, pos)
}

#[called_by_game]
unsafe extern "C" fn tell_detour(handle: Handle) -> i64 {
    if let Some(pos) = state().tell(handle as usize) {
        return pos;
    }
    ORIG_TELL.get()(handle)
}

#[called_by_game]
unsafe extern "C" fn eof_detour(handle: Handle) -> i32 {
    if let Some(eof) = state().eof(handle as usize) {
        return i32::from(eof);
    }
    ORIG_EOF.get()(handle)
}

/// `PHYSFS_close` is only observed: a spliced copy is dropped as its file
/// closes.
fn close_before(call: &mut Call) {
    let handle = call.arg(0);
    state().close(handle);
}

/// `PHYSFS_setErrorCode(code)`: shared by loading speed-ups that answer a
/// missing file without calling Windows.
pub(crate) const SET_ERROR: GameFunction<1> = GameFunction::at(0x19f8510);
/// `PHYSFS_ERR_NOT_FOUND`.
pub(crate) const NOT_FOUND: usize = 0xb;

/// Sets PhysFS's thread-local error code to `PHYSFS_ERR_NOT_FOUND`.
pub(crate) fn set_not_found(m: &Memory) {
    let _ = SET_ERROR.call(m, [NOT_FOUND]);
}

/// Every PhysFS function the core detours, observes or calls, by export name.
pub fn points() -> Vec<Point> {
    vec![
        Point::replace(c"PHYSFS_init", "40 56 48 83 ec 20 83 3d 33 1a 94 00 00 48 8b f1", "PhysFS init", init_detour as InitFn, &ORIG_INIT),
        Point::replace(c"PHYSFS_openRead", "40 55 48 83 ec 30 48 8d 6c 24 20 48 89 5d 20 48", "PhysFS open", open_read_detour as OpenReadFn, &ORIG_OPEN_READ),
        Point::replace(c"PHYSFS_fileLength", "48 8b 01 48 8b c8 48 ff 60 30", "PhysFS file length", file_length_detour as FileLengthFn, &ORIG_FILE_LENGTH),
        Point::replace(
            c"PHYSFS_readBytes",
            "40 53 56 41 56 48 83 ec 20 48 b8 ff ff ff ff ff",
            "PhysFS read",
            read_bytes_detour as ReadBytesFn,
            &ORIG_READ_BYTES,
        ),
        Point::replace(c"PHYSFS_seek", "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", "PhysFS seek", seek_detour as SeekFn, &ORIG_SEEK),
        Point::replace(c"PHYSFS_tell", "40 53 48 83 ec 20 48 8b 01 48 8b d9 48 8b c8 ff", "PhysFS tell", tell_detour as TellFn, &ORIG_TELL),
        Point::replace(c"PHYSFS_eof", "40 57 48 83 ec 20 80 79 08 00 74 41 48 8b 41 28", "PhysFS end of file", eof_detour as EofFn, &ORIG_EOF),
        Point::observe(c"PHYSFS_close", "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8b 0d", "PhysFS close", 1, Some(close_before), None).recorded(CLOSE.slot()),
        Point::keep(c"PHYSFS_getRealDir", "48 83 ec 28 e8 37 38 00 00 48 85 c0 74 09 48 8b", "PhysFS real directory").recorded(GET_REAL_DIR.slot()),
        Point::keep(c"PHYSFS_mountMemory", "48 89 6c 24 18 56 57 41 56 48 83 ec 20 49 8b f9", "PhysFS mount from memory").recorded(MOUNT_MEMORY.slot()),
        Point::keep(SET_ERROR, "85 c9 74 7f 57 48 83 ec 20 8b f9 48 89 5c 24 30", "PhysFS sets its error"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{normalize, open_handle, Archive, File, OpenOutcome, VirtualFs};
    use crate::native::splice::{Addition, Plan};
    use std::io::Cursor;
    use std::sync::Mutex;

    #[test]
    fn normalizes_separators_case_and_leading_dots() {
        assert_eq!(normalize("Interface\\UnitPanel_Main.GUI"), "interface/unitpanel_main.gui");
        assert_eq!(normalize("./interface/x.gui"), "interface/x.gui");
        assert_eq!(normalize("/interface/x.gui"), "interface/x.gui");
    }

    #[test]
    fn files_come_back_out_of_the_packed_archive() {
        let files =
            vec![File { path: "interface/a.gfx".into(), data: b"bitmapfonts = {}".to_vec() }, File { path: "gfx/fonts/b.tga".into(), data: vec![0; 50] }];
        let archive = Archive::pack(&files);
        assert_eq!(archive.paths, vec!["interface/a.gfx", "gfx/fonts/b.tga"]);
        let find = |path: &str| crate::base::zip::find(&mut Cursor::new(&archive.bytes), path.as_bytes(), archive.bytes.len());
        assert_eq!(find("interface/a.gfx").unwrap(), b"bitmapfonts = {}");
        assert_eq!(find("gfx/fonts/b.tga").unwrap(), [0u8; 50]);
        assert!(find("c").is_err());
    }

    #[test]
    fn virtual_handle_supports_length_chunked_reads_seek_tell_eof_and_close() {
        let vfs = Mutex::new(VirtualFs::default());
        vfs.lock().unwrap().serve("Interface\\Example.gui", b"abcdef".to_vec());

        let outcome = open_handle(&vfs, 7, "./interface/example.gui", || unreachable!(), || unreachable!());
        assert_eq!(outcome, OpenOutcome::Served(6));
        let mut vfs = vfs.into_inner().unwrap();
        assert_eq!(vfs.file_length(7), Some(6));
        assert_eq!(vfs.tell(7), Some(0));
        assert_eq!(vfs.eof(7), Some(false));

        let mut buf = [0u8; 4];
        assert_eq!(vfs.read_bytes(7, &mut buf), Some(4));
        assert_eq!(&buf, b"abcd");
        assert_eq!(vfs.tell(7), Some(4));
        assert_eq!(vfs.eof(7), Some(false));

        assert_eq!(vfs.read_bytes(7, &mut buf), Some(2));
        assert_eq!(&buf[..2], b"ef");
        assert_eq!(vfs.tell(7), Some(6));
        assert_eq!(vfs.eof(7), Some(true));

        assert_eq!(vfs.seek(7, 99), Some(false));
        assert_eq!(vfs.tell(7), Some(6));
        assert_eq!(vfs.seek(7, 2), Some(true));
        assert_eq!(vfs.tell(7), Some(2));
        assert_eq!(vfs.eof(7), Some(false));

        vfs.close(7);
        assert_eq!(vfs.file_length(7), None);
        assert_eq!(vfs.tell(7), None);
        assert_eq!(vfs.eof(7), None);
    }

    #[test]
    fn served_overrides_take_precedence_over_splice_plan() {
        let vfs = Mutex::new(VirtualFs::default());
        let mut plan = Plan::default();
        plan.add("interface/win.gui", Addition::Append("\n# spliced".into()));
        {
            let mut g = vfs.lock().unwrap();
            g.set_plan(plan);
            g.serve("interface/win.gui", b"served-bytes".to_vec());
        }

        let outcome = open_handle(&vfs, 1, "interface/win.gui", || panic!("served file must not read original"), || panic!());
        assert_eq!(outcome, OpenOutcome::Served(12));
        let mut out = vec![0u8; 12];
        assert_eq!(vfs.lock().unwrap().read_bytes(1, &mut out), Some(12));
        assert_eq!(out, b"served-bytes");
    }

    #[test]
    fn splice_plan_handles_success_unchanged_rewind_and_failure_rewind() {
        let vfs = Mutex::new(VirtualFs::default());
        let mut plan = Plan::default();
        plan.add("interface/a.gui", Addition::Append("extra".into()));
        plan.add("interface/b.gui", Addition::Edit { what: "noop", edit: |s| Ok(s.to_string()) });
        vfs.lock().unwrap().set_plan(plan);

        // Unmatched path passes through without reading.
        assert_eq!(open_handle(&vfs, 1, "interface/other.gui", || panic!(), || panic!()), OpenOutcome::Passthrough);

        // Successful splice inserts a virtual handle with the modified bytes.
        assert_eq!(open_handle(&vfs, 2, "interface/a.gui", || Ok(b"base".to_vec()), || panic!()), OpenOutcome::Spliced { before: 4, after: 10 });
        let mut out = vec![0u8; 10];
        assert_eq!(vfs.lock().unwrap().read_bytes(2, &mut out), Some(10));
        assert_eq!(out, b"base\nextra");

        // Unchanged content rewinds the original handle and does not register a virtual handle.
        let mut rewound = false;
        let unchanged = open_handle(
            &vfs,
            3,
            "interface/b.gui",
            || Ok(b"same".to_vec()),
            || {
                rewound = true;
                true
            },
        );
        assert_eq!(unchanged, OpenOutcome::Unchanged { rewound: true });
        assert!(rewound);
        assert_eq!(vfs.lock().unwrap().file_length(3), None);

        // Non-UTF-8 (Windows-1252) .gui files preserve their raw bytes while still applying splices.
        let win1252 = b"# caf\xe9 \xa7Y\nbase".to_vec();
        let expected_len = win1252.len() + "\nextra".len();
        assert_eq!(
            open_handle(&vfs, 4, "interface/a.gui", || Ok(win1252.clone()), || panic!()),
            OpenOutcome::Spliced { before: win1252.len(), after: expected_len }
        );
        let mut out1252 = vec![0u8; expected_len];
        assert_eq!(vfs.lock().unwrap().read_bytes(4, &mut out1252), Some(expected_len));
        assert_eq!(out1252, b"# caf\xe9 \xa7Y\nbase\nextra");

        // Read failure rewinds the original handle and leaves it unintercepted.
        let mut failed_rewound = false;
        let failed = open_handle(
            &vfs,
            5,
            "interface/a.gui",
            || Err("disk read error".into()),
            || {
                failed_rewound = true;
                true
            },
        );
        assert!(matches!(failed, OpenOutcome::SpliceFailed { rewound: true, .. }));
        assert!(failed_rewound);
        assert_eq!(vfs.lock().unwrap().file_length(5), None);
    }

    #[test]
    fn a_text_snapshot_preserves_bytes_and_the_original_position() {
        let bytes = b"EU4txt\nquoted=\"a\\\"b\"\r\nchecksum=\"unchanged\"";
        let mut input = Cursor::new(bytes);
        input.set_position(17);
        assert_eq!(super::snapshot(&mut input).unwrap().unwrap(), bytes);
        assert_eq!(input.position(), 17);
        for bytes in [b"PK\x03\x04zip".as_slice(), b"EU4bin", b"EU4"] {
            let mut input = Cursor::new(bytes);
            input.set_position(2);
            assert!(super::snapshot(&mut input).unwrap().is_none());
            assert_eq!(input.position(), 2);
        }
    }

    #[test]
    fn a_failed_snapshot_restores_the_position() {
        use std::io::{Read, Seek, SeekFrom};
        struct Interrupted(Cursor<&'static [u8]>, usize);
        impl Read for Interrupted {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                self.1 += 1;
                if self.1 > 1 {
                    return Err(std::io::Error::other("read failure after the header"));
                }
                self.0.read(out)
            }
        }
        impl Seek for Interrupted {
            fn seek(&mut self, at: SeekFrom) -> std::io::Result<u64> {
                self.0.seek(at)
            }
        }
        let mut input = Interrupted(Cursor::new(b"EU4txt\ndata"), 0);
        input.0.set_position(4);
        assert!(super::snapshot(&mut input).is_err());
        assert_eq!(input.0.position(), 4);
    }

    #[test]
    fn reinit_runs_first_init_once_before_mounting_and_remounts_same_archive_on_each_success() {
        use std::{cell::RefCell, sync::Once};
        let once = Once::new();
        let archive: RefCell<Option<Archive>> = RefCell::new(None);
        let mounted_ptrs: RefCell<Vec<usize>> = RefCell::new(Vec::new());
        let first_runs = RefCell::new(0usize);

        let first = || {
            *first_runs.borrow_mut() += 1;
            *archive.borrow_mut() = Some(Archive::pack(&[File { path: "interface/a.gfx".into(), data: b"ok".to_vec() }]));
        };
        let mount = || {
            if let Some(a) = archive.borrow().as_ref() {
                mounted_ptrs.borrow_mut().push(a.bytes.as_ptr() as usize);
            }
        };

        super::dispatch_init(&once, true, first, mount);
        super::dispatch_init(&once, false, first, mount);
        super::dispatch_init(&once, true, first, mount);

        assert_eq!(*first_runs.borrow(), 1);
        let ptrs = mounted_ptrs.borrow();
        assert_eq!(ptrs.len(), 2);
        assert_eq!(ptrs[0], ptrs[1]);
    }
}
