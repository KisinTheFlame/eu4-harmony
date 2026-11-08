//! The thread the game keeps updating while it writes a save, woken when the
//! save is written (docs/game-speed.md, S8).
//!
//! While a save is written (`0x5cde60`), a thread of the game's,
//! "SaveSessionUpdateThread", calls `0x5c4460` over and over until it is told
//! to stop: an update of the object it was handed (its `+0x58`'s method
//! `+0x40`), then `Sleep(50)`. When the save is written the main thread tells
//! it to stop and waits for it, often for most of a sleep: 40 to 100 ms of a
//! save's stop. The framework's `0x5c4460` makes the same update, then waits
//! up to the same 50 ms for the main thread's word to stop, which it is given
//! where the main thread tells the game's thread.
use crate::called_by_game;
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// `(arg) -> keep going`: the update and the sleep, which the thread calls
/// while its closure's byte to stop is 0.
const UPDATE: usize = 0x5c4460;
/// The object the update is handed points to, its member updated and the
/// member's method.
const UPDATED: usize = 0x58;
const UPDATE_METHOD: usize = 0x40;
/// How long the game's update sleeps.
const SLEEP: Duration = Duration::from_millis(50);
/// In `0x5cde60`, after the save is written: `mov byte ptr [rsp + 0x30], 1`,
/// the closure's byte to stop, before the thread is waited for.
const STOP: usize = 0x5ce531;
const STOP_NEXT: usize = 0x5ce536;

/// Counts the words to stop; a thread's sleep ends at the next one.
static STOPS: AtomicU64 = AtomicU64::new(0);
static WAKE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(
            UPDATE,
            "48 83 ec 28 48 8b 01 33 d2 48 8b 48 58 48 8b 01 ff 50 40 b9 32 00 00 00 ff 15 d2 21 5a 01 b0 01 48 83 c4 28 c3",
            "save's update",
            update as Update,
            &ORIGINAL_UPDATE,
        )
        .uses(&[UPDATED, UPDATE_METHOD, 50]),
        // The loop that calls it: while the byte at `+0x10` is 0, and the
        // update says to go on.
        Point::keep(
            0x1720880,
            "40 53 48 83 ec 20 0f b6 41 10 48 8b d9 b2 01 84 c0 75 18 84 d2 74 14 48 8b 03 48 8b 4b 08 ff d0 0f b6 4b 10 0f b6 d0 84 c9 74 e8",
            "save's update thread's loop",
        ),
        Point::jump(STOP, "c6 44 24 30 01", "save's update thread told to stop", stop_stub as *const () as usize, &STOP_RESUME),
        // Where it is waited for: the thread's join.
        Point::keep(STOP_NEXT, "48 83 39 00 74 0b ff 15 de 92 59 01", "save's update thread waited for"),
    ]
}

type Update = extern "C" fn(usize) -> bool;
static ORIGINAL_UPDATE: Function<Update> = Function::new();

#[called_by_game]
extern "C" fn update(arg: usize) -> bool {
    if super::game_writes_saves() {
        return ORIGINAL_UPDATE.get()(arg);
    }
    crate::guarded("save's update thread", || {
        let stops = STOPS.load(Ordering::SeqCst);
        let m = crate::hooked_memory();
        let updated = (|| {
            let object = m.ptr(m.ptr(arg)? + UPDATED)?;
            m.call(m.vslot(object, UPDATE_METHOD)?, &[object, 0])
        })();
        if let Err(e) = updated {
            log!("saves: the save's update thread: {e}");
        }
        let (lock, wake) = &WAKE;
        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = wake.wait_timeout_while(guard, SLEEP, |_| STOPS.load(Ordering::SeqCst) == stops);
    });
    true
}

#[called_by_game]
extern "C" fn stopped() {
    let (lock, wake) = &WAKE;
    let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
    STOPS.fetch_add(1, Ordering::SeqCst);
    wake.notify_all();
}

crate::resume!(STOP_RESUME: STOP_AT = STOP_NEXT);

/// Replaces `mov byte ptr [rsp + 0x30], 1`: the thread is woken once it is
/// told to stop, so that its loop sees the byte.
#[unsafe(naked)]
extern "C" fn stop_stub() {
    std::arch::naked_asm!(
        "mov byte ptr [rsp + 0x30], 1",
        crate::call_rust!(call stopped),
        "jmp qword ptr [rip + {next}]",
        stopped = sym stopped,
        next = sym STOP_AT,
    );
}
