//! What the game's AI changes on several worker threads at once without a
//! lock, in vanilla too: a country's coalition, which the AI of the
//! countries in it leaves in parallel. Leaving and joining a coalition take
//! turns.
use crate::{hooks::Call, objects::campaign::country, points::Point};
use std::sync::atomic::Ordering;

/// `(country, …, other)`: a country's AI weighs leaving the coalition
/// against `other` (a tag) and the coalitions its members are in, and takes
/// countries off them; on worker threads. `(country)`: a country joins the
/// coalition against the country it targets, on the main thread.
const COALITION_LEAVE: usize = 0x3b0220;
const COALITION_JOIN: usize = 0x3e3940;

/// The turns leaving and joining a coalition take: a lock, and the thread
/// that holds it and how many times, so that it may take its turn again.
struct Turns {
    lock: std::cell::UnsafeCell<windows_sys::Win32::System::Threading::SRWLOCK>,
    holder: std::sync::atomic::AtomicU32,
    depth: std::cell::Cell<u32>,
}
// The lock is the SRW lock's; `depth` is touched only by its holder.
unsafe impl Sync for Turns {}

static COALITION_TURNS: Turns = Turns {
    lock: std::cell::UnsafeCell::new(windows_sys::Win32::System::Threading::SRWLOCK_INIT),
    holder: std::sync::atomic::AtomicU32::new(0),
    depth: std::cell::Cell::new(0),
};

/// A country's coalition is about to change: wait for the turn. Several
/// countries' AI take countries off the same coalition at once, on worker
/// threads, and its list is not made for it.
fn coalition_changing(_: &mut Call) {
    let turns = &COALITION_TURNS;
    let me = crate::thread_id();
    if turns.holder.load(Ordering::Acquire) != me {
        unsafe { windows_sys::Win32::System::Threading::AcquireSRWLockExclusive(turns.lock.get()) };
        turns.holder.store(me, Ordering::Release);
    }
    turns.depth.set(turns.depth.get() + 1);
}

/// The change is made, or the function faulted: the turn is over.
fn coalition_changed(_: &mut Call) {
    let turns = &COALITION_TURNS;
    if turns.holder.load(Ordering::Acquire) != crate::thread_id() {
        return;
    }
    let depth = turns.depth.get() - 1;
    turns.depth.set(depth);
    if depth == 0 {
        turns.holder.store(0, Ordering::Release);
        unsafe { windows_sys::Win32::System::Threading::ReleaseSRWLockExclusive(turns.lock.get()) };
    }
}

/// The repair as the world reset sees it ([`super::REPAIRS`]): it counts
/// nothing.
pub(super) const REPAIR: super::Repair = super::Repair { points, ..super::Repair::NONE };

pub(super) fn points() -> Vec<Point> {
    vec![
        // Leaving and joining a coalition change its list; the AI leaves on
        // worker threads.
        Point::observe(
            COALITION_LEAVE,
            "4c 89 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55 53 56 57 41 54 41 55 41 56 41 57",
            "AI leaving a coalition",
            4,
            Some(coalition_changing),
            Some(coalition_changed),
        ),
        Point::keep(0x3b0c33usize, "48 8b 8b 28 19 00 00", "coalition taken off by the AI").uses(&[country::COALITION]),
        Point::observe(
            COALITION_JOIN,
            "48 89 5c 24 20 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 20 48 8b e9",
            "joining a coalition",
            1,
            Some(coalition_changing),
            Some(coalition_changed),
        ),
        Point::keep(0x3e3c31usize, "49 8b 81 28 19 00 00", "coalition joined").uses(&[country::COALITION]),
    ]
}
