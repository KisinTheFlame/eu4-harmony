//! The time limit (ADR an-error-does-not-stop-an-extension): a stretch of Lua
//! that runs for more than [`LIMIT`] without waiting is stopped with an error.
//!
//! A count hook set for the whole stretch would slow Lua 5.4 by 50 to 80%
//! in tight loops, whatever its interval (measured 2026-11-03), so a thread
//! of its own watches instead, and only once the limit has passed sets a
//! hook on the running Lua thread, which raises the error at its next
//! instructions. `lua_sethook` is the one Lua call made to be called from
//! outside the thread running the state.
use mlua::ffi;
use std::{
    sync::{Mutex, Once},
    time::{Duration, Instant},
};

/// The longest a stretch may run without waiting.
pub const LIMIT: Duration = Duration::from_secs(1);

/// How often the watcher looks.
const LOOK: Duration = Duration::from_millis(50);

/// The message of the error a stopped stretch raises.
pub const STOPPED: &str = "stopped: it ran for more than 1 s without waiting (the time limit)";

/// A stretch running: the Lua thread it runs on, since when, and whether the
/// hook is set on it.
struct Running {
    thread: std::thread::ThreadId,
    state: usize,
    since: Instant,
    hooked: bool,
}

/// The stretches running, the innermost of each thread last: a stretch may
/// start another, as `spawn` does, which runs until it first waits. In the
/// game only the main thread runs Lua; tests run several.
static RUNNING: Mutex<Vec<Running>> = Mutex::new(Vec::new());

fn running() -> std::sync::MutexGuard<'static, Vec<Running>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

/// A stretch begins on the Lua thread `state` (a `lua_State*`).
pub fn start(state: *const std::ffi::c_void) {
    static WATCHER: Once = Once::new();
    WATCHER.call_once(|| {
        let _ = std::thread::Builder::new().name("eu4-harmony lua watch".into()).spawn(watch);
    });
    running().push(Running { thread: std::thread::current().id(), state: state as usize, since: Instant::now(), hooked: false });
}

/// The stretch has ended; whether the time limit stopped it. The hook, if
/// set, is taken off here, before the state runs anything else, and not
/// before.
pub fn end() -> bool {
    let r = {
        let mut all = running();
        let here = std::thread::current().id();
        let Some(at) = all.iter().rposition(|r| r.thread == here) else { return false };
        all.remove(at)
    };
    if r.hooked {
        unsafe { ffi::lua_sethook(r.state as *mut ffi::lua_State, None, 0, 0) };
    }
    r.hooked
}

fn watch() {
    loop {
        std::thread::sleep(LOOK);
        let mut all = running();
        for at in 0..all.len() {
            let innermost = !all[at + 1..].iter().any(|later| later.thread == all[at].thread);
            let r = &mut all[at];
            if innermost && !r.hooked && r.since.elapsed() > LIMIT {
                r.hooked = true;
                unsafe { ffi::lua_sethook(r.state as *mut ffi::lua_State, Some(time_up), ffi::LUA_MASKCOUNT, 1) };
            }
        }
    }
}

/// The hook: the error, which unwinds the stretch to the host as any error
/// does. It stays set until the stretch ends, so an error caught by `pcall`
/// is raised again at the next instruction, and the stretch cannot go on.
unsafe extern "C-unwind" fn time_up(state: *mut ffi::lua_State, _: *mut ffi::lua_Debug) {
    ffi::lua_pushlstring(state, STOPPED.as_ptr().cast(), STOPPED.len());
    ffi::lua_error(state);
}
