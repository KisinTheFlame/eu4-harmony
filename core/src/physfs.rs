//! Hooks on the PhysFS functions that `eu4.exe` exports by name. Script files
//! (`.gui`, `.gfx`) are read whole into a buffer before the game's lexer sees
//! them, so the hooks serve a spliced copy of the chosen files from a buffer
//! of our own.
//!
//! The detours run on whichever thread the game reads files from (GUI
//! definitions load on the game's LoadingThread) and never call extension code.
use crate::{
    hooks::{self, HookFrame},
    splice::{self, Addition, Plan},
    Result,
};
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        LazyLock, Mutex, Once,
    },
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

type File = *mut c_void;

type InitFn = unsafe extern "C" fn(*const c_char) -> i32;
type OpenReadFn = unsafe extern "C" fn(*const c_char) -> File;
type FileLengthFn = unsafe extern "C" fn(File) -> i64;
type ReadBytesFn = unsafe extern "C" fn(File, *mut c_void, u64) -> i64;
type SeekFn = unsafe extern "C" fn(File, u64) -> i32;
type TellFn = unsafe extern "C" fn(File) -> i64;
type EofFn = unsafe extern "C" fn(File) -> i32;
type GetRealDirFn = unsafe extern "C" fn(*const c_char) -> *const c_char;

static ORIG_INIT: AtomicUsize = AtomicUsize::new(0);
static ORIG_OPEN_READ: AtomicUsize = AtomicUsize::new(0);
static ORIG_FILE_LENGTH: AtomicUsize = AtomicUsize::new(0);
static ORIG_READ_BYTES: AtomicUsize = AtomicUsize::new(0);
static ORIG_SEEK: AtomicUsize = AtomicUsize::new(0);
static ORIG_TELL: AtomicUsize = AtomicUsize::new(0);
static ORIG_EOF: AtomicUsize = AtomicUsize::new(0);
static GET_REAL_DIR: AtomicUsize = AtomicUsize::new(0);

fn load<F: Copy>(slot: &AtomicUsize) -> F {
    let p = slot.load(Ordering::Acquire);
    assert!(p != 0, "original not resolved");
    assert_eq!(std::mem::size_of::<F>(), std::mem::size_of::<usize>());
    unsafe { std::mem::transmute_copy(&p) }
}

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
    let result = load::<InitFn>(&ORIG_INIT)(argv0);
    crate::guarded("PHYSFS_init", || INIT_ONCE.call_once(crate::deferred_init));
    result
}

unsafe extern "C" fn open_read_detour(path: *const c_char) -> File {
    let handle = load::<OpenReadFn>(&ORIG_OPEN_READ)(path);
    crate::guarded("PHYSFS_openRead", || {
        INIT_ONCE.call_once(crate::deferred_init);
        if handle.is_null() {
            return;
        }
        let raw = c_str(path);
        let normalized = normalize(&raw);
        let Some(additions) = state().plan.as_ref().and_then(|p| p.for_file(&normalized)) else { return };
        let real_dir = match GET_REAL_DIR.load(Ordering::Acquire) {
            0 => "<n/a>".into(),
            _ => c_str(load::<GetRealDirFn>(&GET_REAL_DIR)(path)),
        };
        match splice_handle(handle, &additions) {
            Ok((before, data)) => {
                log!("spliced {raw:?} from {real_dir:?}: {before} -> {} bytes", data.len());
                state().open.insert(handle as usize, Spliced { data, cursor: 0 });
            }
            Err(e) => {
                log!("splice of {raw:?} failed, serving the original: {e}");
                if load::<SeekFn>(&ORIG_SEEK)(handle, 0) == 0 {
                    log!("rewind after a failed splice also failed");
                }
            }
        }
    });
    handle
}

/// Reads the whole original file through the trampolines and applies the plan.
fn splice_handle(handle: File, additions: &[Addition]) -> Result<(usize, Vec<u8>)> {
    let len = unsafe { load::<FileLengthFn>(&ORIG_FILE_LENGTH)(handle) };
    if !(0..=64 * 1024 * 1024).contains(&len) {
        return Err(format!("unexpected file length {len}"));
    }
    let mut data = vec![0u8; len as usize];
    let read = unsafe { load::<ReadBytesFn>(&ORIG_READ_BYTES)(handle, data.as_mut_ptr() as _, len as u64) };
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
    load::<FileLengthFn>(&ORIG_FILE_LENGTH)(handle)
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
    load::<ReadBytesFn>(&ORIG_READ_BYTES)(handle, buf, len)
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
    load::<SeekFn>(&ORIG_SEEK)(handle, pos)
}

unsafe extern "C" fn tell_detour(handle: File) -> i64 {
    if let Some(s) = state().open.get(&(handle as usize)) {
        return s.cursor as i64;
    }
    load::<TellFn>(&ORIG_TELL)(handle)
}

unsafe extern "C" fn eof_detour(handle: File) -> i32 {
    if let Some(s) = state().open.get(&(handle as usize)) {
        return (s.cursor >= s.data.len()) as i32;
    }
    load::<EofFn>(&ORIG_EOF)(handle)
}

/// `PHYSFS_close` is observed rather than detoured because the save tracker
/// watches it too.
extern "C" fn close_before(frame: *mut HookFrame) {
    crate::guarded("PHYSFS_close", || {
        let handle = unsafe { (*frame).args[0] };
        state().open.remove(&handle);
    });
    if crate::BASELINE_OK.load(Ordering::Acquire) {
        crate::saves::close_before(frame);
    }
}

extern "C" fn close_after(frame: *mut HookFrame) {
    if crate::BASELINE_OK.load(Ordering::Acquire) {
        crate::saves::close_after(frame);
    }
}

fn export(name: &CStr) -> Result<usize> {
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    unsafe { GetProcAddress(module, name.as_ptr() as *const u8) }
        .map(|f| f as usize)
        .ok_or_else(|| format!("eu4.exe does not export {}", name.to_string_lossy()))
}

/// Creates (but does not enable) every PhysFS hook.
pub fn install() -> Result<()> {
    GET_REAL_DIR.store(export(c"PHYSFS_getRealDir")?, Ordering::Release);
    let detours: [(&CStr, usize, &AtomicUsize); 7] = [
        (c"PHYSFS_init", init_detour as *const () as usize, &ORIG_INIT),
        (c"PHYSFS_openRead", open_read_detour as *const () as usize, &ORIG_OPEN_READ),
        (c"PHYSFS_fileLength", file_length_detour as *const () as usize, &ORIG_FILE_LENGTH),
        (c"PHYSFS_readBytes", read_bytes_detour as *const () as usize, &ORIG_READ_BYTES),
        (c"PHYSFS_seek", seek_detour as *const () as usize, &ORIG_SEEK),
        (c"PHYSFS_tell", tell_detour as *const () as usize, &ORIG_TELL),
        (c"PHYSFS_eof", eof_detour as *const () as usize, &ORIG_EOF),
    ];
    for (name, detour, slot) in detours {
        slot.store(hooks::create(export(name)?, detour)?, Ordering::Release);
    }
    hooks::observe(export(c"PHYSFS_close")?, 1, Some(close_before), Some(close_after))?;
    log!("PhysFS hooks created");
    Ok(())
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
