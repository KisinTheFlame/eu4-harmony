//! The time limit (ADR an-error-does-not-stop-an-extension): a stretch of Lua
//! that runs for more than [`LIMIT`] without waiting is stopped with an error.
//!
//! A count hook set for the whole stretch would slow Lua 5.4 by 50 to 80%
//! in tight loops, whatever its interval (measured 2026-11-03), so a thread
//! of its own watches instead, and only once the limit has passed sets a
//! hook on the running Lua thread, which raises the error at its next
//! instruction and at every one after until the stretch ends. `lua_sethook`
//! is the one Lua call made to be called from outside the thread running the
//! state. It sets the hook on that one thread, and Lua gives a thread the
//! hook only when it is made, so the sandbox's `coroutine.resume` and
//! `coroutine.wrap` tell the watcher which coroutine of the mod's own runs
//! ([`resume`]).
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

/// A stretch running: the Lua thread it runs on, and the one its error is
/// written on if that is another, since when, and whether the hook is set on
/// them. Or a coroutine of the mod's own it resumed, which runs within the
/// stretch and since its start.
struct Running {
    thread: std::thread::ThreadId,
    state: usize,
    also: Option<usize>,
    since: Instant,
    hooked: bool,
    coroutine: bool,
}

impl Running {
    fn hook(&self, on: bool) {
        hook(self.state, on);
        if let Some(also) = self.also {
            hook(also, on);
        }
    }
}

/// The stretches running and the coroutines they resumed, the innermost of
/// each thread last: a stretch may start another, as `spawn` does, which
/// runs until it first waits. In the game only the main thread runs Lua;
/// tests run several.
static RUNNING: Mutex<Vec<Running>> = Mutex::new(Vec::new());

fn running() -> std::sync::MutexGuard<'static, Vec<Running>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sets the hook on `state`, or takes it off.
fn hook(state: usize, on: bool) {
    let state = state as *mut ffi::lua_State;
    match on {
        true => unsafe { ffi::lua_sethook(state, Some(time_up), ffi::LUA_MASKCOUNT, 1) },
        false => unsafe { ffi::lua_sethook(state, None, 0, 0) },
    }
}

/// A stretch begins on the Lua thread `state` (a `lua_State*`), its error to
/// be written on `also`: mlua writes it as text on the thread that resumed
/// the stretch, which runs the error's `__tostring`, if it has one, there. A
/// thread Lua made while the hook was set has it too, from its maker; it is
/// taken off.
pub fn start(state: *const std::ffi::c_void, also: Option<*const std::ffi::c_void>) {
    static WATCHER: Once = Once::new();
    WATCHER.call_once(|| {
        let _ = std::thread::Builder::new().name("eu4-harmony lua watch".into()).spawn(watch);
    });
    let mut all = running();
    let r = Running {
        thread: std::thread::current().id(),
        state: state as usize,
        also: also.map(|also| also as usize),
        since: Instant::now(),
        hooked: false,
        coroutine: false,
    };
    r.hook(false);
    all.push(r);
}

/// The stretch has ended; whether the time limit stopped it. The hook, if
/// set, is taken off here, before the state runs anything else, and not
/// before; so is that of every coroutine it resumed that an error left
/// behind.
pub fn end() -> bool {
    let mut all = running();
    let here = std::thread::current().id();
    let mut stopped = false;
    while let Some(at) = all.iter().rposition(|r| r.thread == here) {
        let r = all.remove(at);
        if r.hooked {
            r.hook(false);
            stopped = true;
        }
        if !r.coroutine {
            break;
        }
    }
    stopped
}

/// The running stretch resumes `state`, a coroutine of the mod's own, which
/// is watched as the stretch, with the hook set on it when it is set on the
/// stretch.
pub fn resume(state: *const std::ffi::c_void) {
    let mut all = running();
    let here = std::thread::current().id();
    let Some(outer) = all.iter().rev().find(|r| r.thread == here) else { return };
    let (since, hooked) = (outer.since, outer.hooked);
    hook(state as usize, hooked);
    all.push(Running { thread: here, state: state as usize, also: None, since, hooked, coroutine: true });
}

/// The coroutine `state` resumed has given control back, by yielding,
/// returning or an error.
pub fn resumed(state: *const std::ffi::c_void) {
    let mut all = running();
    let here = std::thread::current().id();
    let Some(at) = all.iter().rposition(|r| r.thread == here) else { return };
    if all[at].coroutine && all[at].state == state as usize && all.remove(at).hooked {
        hook(state as usize, false);
    }
}

fn watch() {
    loop {
        std::thread::sleep(LOOK);
        // Every one past the limit, the stretches waiting for one they
        // started or a coroutine they resumed too, which raise the error as
        // soon as they run again.
        for r in running().iter_mut() {
            if !r.hooked && r.since.elapsed() > LIMIT {
                r.hooked = true;
                r.hook(true);
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
