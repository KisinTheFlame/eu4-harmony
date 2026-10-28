//! Hooks on the PhysFS functions that `eu4.exe` exports by name. Script files
//! (`.gui`, `.gfx`) are read whole into a buffer before the game's lexer sees
//! them, so the hooks serve a spliced copy of the chosen files from a buffer
//! of our own.
//!
//! The detours run on whichever thread the game reads files from (GUI
//! definitions load on the game's LoadingThread) and never call extension code.
use crate::{
    hooks::HookFrame,
    points::{Function, Point},
    splice::{self, Addition, Plan},
    Result,
};
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr},
    sync::{
        LazyLock, Mutex, Once,
    },
};

type File = *mut c_void;

type InitFn = unsafe extern "C" fn(*const c_char) -> i32;
type OpenReadFn = unsafe extern "C" fn(*const c_char) -> File;
type FileLengthFn = unsafe extern "C" fn(File) -> i64;
type ReadBytesFn = unsafe extern "C" fn(File, *mut c_void, u64) -> i64;
type SeekFn = unsafe extern "C" fn(File, u64) -> i32;
type TellFn = unsafe extern "C" fn(File) -> i64;
type EofFn = unsafe extern "C" fn(File) -> i32;
type GetRealDirFn = unsafe extern "C" fn(*const c_char) -> *const c_char;

static ORIG_INIT: Function<InitFn> = Function::new();
static ORIG_OPEN_READ: Function<OpenReadFn> = Function::new();
static ORIG_FILE_LENGTH: Function<FileLengthFn> = Function::new();
static ORIG_READ_BYTES: Function<ReadBytesFn> = Function::new();
static ORIG_SEEK: Function<SeekFn> = Function::new();
static ORIG_TELL: Function<TellFn> = Function::new();
static ORIG_EOF: Function<EofFn> = Function::new();
static GET_REAL_DIR: Function<GetRealDirFn> = Function::new();

struct Spliced {
    data: Vec<u8>,
    cursor: usize,
}

struct State {
    plan: Option<Plan>,
    open: HashMap<usize, Spliced>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State { plan: None, open: HashMap::new() }));
static INIT_ONCE: Once = Once::new();

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Installs the splice plan; files opened from now on are served spliced.
pub fn set_plan(plan: Plan) {
    state().plan = Some(plan);
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

unsafe extern "C" fn init_detour(argv0: *const c_char) -> i32 {
    let result = ORIG_INIT.get()(argv0);
    crate::guarded("PHYSFS_init", || INIT_ONCE.call_once(crate::deferred_init));
    result
}

unsafe extern "C" fn open_read_detour(path: *const c_char) -> File {
    let handle = ORIG_OPEN_READ.get()(path);
    crate::guarded("PHYSFS_openRead", || {
        INIT_ONCE.call_once(crate::deferred_init);
        if handle.is_null() {
            return;
        }
        let raw = c_str(path);
        let normalized = normalize(&raw);
        let Some(additions) = state().plan.as_ref().and_then(|p| p.for_file(&normalized)) else { return };
        let real_dir = match GET_REAL_DIR.address() {
            0 => "<n/a>".into(),
            _ => c_str(GET_REAL_DIR.get()(path)),
        };
        match splice_handle(handle, &additions) {
            Ok((before, data)) => {
                log!("spliced {raw:?} from {real_dir:?}: {before} -> {} bytes", data.len());
                state().open.insert(handle as usize, Spliced { data, cursor: 0 });
            }
            Err(e) => {
                log!("splice of {raw:?} failed, serving the original: {e}");
                if ORIG_SEEK.get()(handle, 0) == 0 {
                    log!("rewind after a failed splice also failed");
                }
            }
        }
    });
    handle
}

/// Reads the whole original file through the trampolines and applies the plan.
fn splice_handle(handle: File, additions: &[Addition]) -> Result<(usize, Vec<u8>)> {
    let len = unsafe { ORIG_FILE_LENGTH.get()(handle) };
    if !(0..=64 * 1024 * 1024).contains(&len) {
        return Err(format!("unexpected file length {len}"));
    }
    let mut data = vec![0u8; len as usize];
    let read = unsafe { ORIG_READ_BYTES.get()(handle, data.as_mut_ptr() as _, len as u64) };
    if read != len {
        return Err(format!("short read {read} of {len}"));
    }
    let text = String::from_utf8(data).map_err(|e| format!("file is not UTF-8: {e}"))?;
    Ok((len as usize, splice::apply(&text, additions)?.into_bytes()))
}

unsafe extern "C" fn file_length_detour(handle: File) -> i64 {
    if let Some(s) = state().open.get(&(handle as usize)) {
        return s.data.len() as i64;
    }
    ORIG_FILE_LENGTH.get()(handle)
}

unsafe extern "C" fn read_bytes_detour(handle: File, buf: *mut c_void, len: u64) -> i64 {
    {
        let mut st = state();
        if let Some(s) = st.open.get_mut(&(handle as usize)) {
            if buf.is_null() {
                return -1;
            }
            let n = (len as usize).min(s.data.len().saturating_sub(s.cursor));
            std::ptr::copy_nonoverlapping(s.data.as_ptr().add(s.cursor), buf as *mut u8, n);
            s.cursor += n;
            return n as i64;
        }
    }
    ORIG_READ_BYTES.get()(handle, buf, len)
}

unsafe extern "C" fn seek_detour(handle: File, pos: u64) -> i32 {
    {
        let mut st = state();
        if let Some(s) = st.open.get_mut(&(handle as usize)) {
            if pos as usize > s.data.len() {
                return 0;
            }
            s.cursor = pos as usize;
            return 1;
        }
    }
    ORIG_SEEK.get()(handle, pos)
}

unsafe extern "C" fn tell_detour(handle: File) -> i64 {
    if let Some(s) = state().open.get(&(handle as usize)) {
        return s.cursor as i64;
    }
    ORIG_TELL.get()(handle)
}

unsafe extern "C" fn eof_detour(handle: File) -> i32 {
    if let Some(s) = state().open.get(&(handle as usize)) {
        return (s.cursor >= s.data.len()) as i32;
    }
    ORIG_EOF.get()(handle)
}

/// `PHYSFS_close` is observed rather than detoured because the save tracker
/// watches it too.
extern "C" fn close_before(frame: *mut HookFrame) {
    crate::guarded("PHYSFS_close", || {
        let handle = unsafe { (*frame).args[0] };
        state().open.remove(&handle);
    });
    crate::saves::close_before(frame);
}

extern "C" fn close_after(frame: *mut HookFrame) {
    crate::saves::close_after(frame);
}

/// `PHYSFS_getRealDir`, which the save tracker calls too.
pub fn real_dir() -> usize {
    GET_REAL_DIR.address()
}

/// Every PhysFS function the core detours, observes or calls, by export name.
pub fn points() -> Vec<Point> {
    vec![
        Point::replace(c"PHYSFS_init", "40 56 48 83 ec 20 83 3d 33 1a 94 00 00 48 8b f1", "PhysFS init", init_detour as InitFn, &ORIG_INIT),
        Point::replace(c"PHYSFS_openRead", "40 55 48 83 ec 30 48 8d 6c 24 20 48 89 5d 20 48", "PhysFS open", open_read_detour as OpenReadFn, &ORIG_OPEN_READ),
        Point::replace(c"PHYSFS_fileLength", "48 8b 01 48 8b c8 48 ff 60 30", "PhysFS file length", file_length_detour as FileLengthFn, &ORIG_FILE_LENGTH),
        Point::replace(c"PHYSFS_readBytes", "40 53 56 41 56 48 83 ec 20 48 b8 ff ff ff ff ff", "PhysFS read", read_bytes_detour as ReadBytesFn, &ORIG_READ_BYTES),
        Point::replace(c"PHYSFS_seek", "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", "PhysFS seek", seek_detour as SeekFn, &ORIG_SEEK),
        Point::replace(c"PHYSFS_tell", "40 53 48 83 ec 20 48 8b 01 48 8b d9 48 8b c8 ff", "PhysFS tell", tell_detour as TellFn, &ORIG_TELL),
        Point::replace(c"PHYSFS_eof", "40 57 48 83 ec 20 80 79 08 00 74 41 48 8b 41 28", "PhysFS end of file", eof_detour as EofFn, &ORIG_EOF),
        Point::observe(c"PHYSFS_close", "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8b 0d", "PhysFS close", 1, Some(close_before), Some(close_after)),
        Point::keep(c"PHYSFS_getRealDir", "48 83 ec 28 e8 37 38 00 00 48 85 c0 74 09 48 8b", "PhysFS real directory").recorded(GET_REAL_DIR.slot()),
    ]
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalizes_separators_case_and_leading_dots() {
        assert_eq!(normalize("Interface\\UnitPanel_Main.GUI"), "interface/unitpanel_main.gui");
        assert_eq!(normalize("./interface/x.gui"), "interface/x.gui");
        assert_eq!(normalize("/interface/x.gui"), "interface/x.gui");
    }
}
