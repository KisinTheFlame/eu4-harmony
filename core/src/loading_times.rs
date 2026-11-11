//! When each stretch of the game's startup and of a save's load begins and
//! ends, in the framework log, so that a player's logs say where a slow
//! start spent its time (docs/research/loading.md, "The startup after L5,
//! broken down"). The game's own `setup.log` and `game.log` carry no times.
//!
//! Each stretch is one function of the game's, observed at its start: its
//! first call's start and end are logged as it returns, in seconds from the
//! framework's bootstrap, and once more together at the main menu, with the
//! stretches that lie between them (`main` up to the engine's Init, the rest
//! of InitGame after the flags). A save's load is logged at every load. The
//! observers only read the clock.
use crate::hooks::Call;
use crate::points::Point;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// One function timed: where, its bytes, how many arguments it reads, what it
/// is in the log, and whether every call is logged or the first only.
struct Timed {
    rva: usize,
    bytes: &'static str,
    argc: u32,
    what: &'static str,
    every_call: bool,
}

const ENGINE_INIT: usize = 0x14c02b0;
const GFX: usize = 0x202850;
const MESHES: usize = 0x14dc5d0;
const DATABASES: usize = 0x208620;
const EVENTS: usize = 0x6f3250;
const FLAGS: usize = 0x10b2b70;
const INIT_GAME: usize = 0x2043a0;
const MAIN_MENU_PART: usize = 0x206260;
const LOAD: usize = 0xd84b80;
const SAVE_READ: usize = 0x5cf740;

#[rustfmt::skip]
const TIMED: [Timed; 10] = [
    Timed { rva: ENGINE_INIT, bytes: "48 89 5c 24 18 55 56 57 48 83 ec 50 48 8b d9", argc: 1, what: "the engine's Init", every_call: false },
    Timed { rva: GFX, bytes: "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57", argc: 2, what: "the gfx stage", every_call: false },
    Timed { rva: MESHES, bytes: "48 89 5c 24 18 88 54 24 10 55 56 57 48 83 ec 30", argc: 2, what: "meshes and particles", every_call: false },
    Timed { rva: DATABASES, bytes: "40 53 56 57 41 54 41 55 41 56 41 57 b8 f0 f5 00 00", argc: 1, what: "the databases", every_call: false },
    Timed { rva: EVENTS, bytes: "48 89 54 24 10 48 89 4c 24 08 55 53 56 57 41 54", argc: 2, what: "the events", every_call: false },
    Timed { rva: FLAGS, bytes: "48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57", argc: 2, what: "flags and loading screens", every_call: false },
    Timed { rva: INIT_GAME, bytes: "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57", argc: 1, what: "InitGame", every_call: false },
    Timed { rva: MAIN_MENU_PART, bytes: "48 8b c4 48 89 58 20 55 56 57 41 54 41 55 41 56", argc: 1, what: "the main menu's part", every_call: false },
    Timed { rva: LOAD, bytes: "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 4c 89 4c 24 20", argc: 5, what: "a save's load", every_call: true },
    Timed { rva: SAVE_READ, bytes: "48 89 4c 24 08 53 56 57 41 54 41 55 41 56 41 57", argc: 1, what: "the save read and the world set up", every_call: true },
];

/// Each startup function's first call, its start and end in microseconds
/// from the bootstrap, each one more so that 0 is not yet.
static FIRST: [(AtomicU64, AtomicU64); TIMED.len()] = [const { (AtomicU64::new(0), AtomicU64::new(0)) }; TIMED.len()];

thread_local! {
    /// This thread's timed calls still running, innermost last: which, and
    /// when it began.
    static RUNNING: RefCell<Vec<(usize, Duration)>> = const { RefCell::new(Vec::new()) };
}

pub fn points() -> Vec<Point> {
    TIMED.iter().map(|t| Point::observe(t.rva, t.bytes, t.what, t.argc, Some(began), Some(ended))).collect()
}

fn index(call: &Call) -> Option<usize> {
    let rva = call.observed()?.wrapping_sub(call.memory().base);
    TIMED.iter().position(|t| t.rva == rva)
}

fn began(call: &mut Call) {
    let Some(i) = index(call) else { return };
    let now = crate::since_bootstrap();
    RUNNING.with(|r| r.borrow_mut().push((i, now)));
}

fn ended(call: &mut Call) {
    let Some(i) = index(call) else { return };
    let end = crate::since_bootstrap();
    // The call returning is this thread's innermost timed one.
    let Some((running, start)) = RUNNING.with(|r| r.borrow_mut().pop()) else { return };
    if running != i {
        return;
    }
    let t = &TIMED[i];
    let fault = if call.faulted() { ", left through an exception" } else { "" };
    if t.every_call {
        log!("loading: {} ({:#x}) from {} to {} s, {} s{fault}", t.what, t.rva, secs(start), secs(end), secs(end - start));
        return;
    }
    let (first_start, first_end) = &FIRST[i];
    if first_start.compare_exchange(0, micros(start), Ordering::AcqRel, Ordering::Acquire).is_ok() {
        first_end.store(micros(end), Ordering::Release);
        log!("loading: {} ({:#x}) from {} to {} s, {} s{fault}", t.what, t.rva, secs(start), secs(end), secs(end - start));
    }
}

/// One more than the microseconds, so that 0 stays "not yet".
fn micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX - 1) + 1
}

fn secs(d: Duration) -> String {
    format!("{:.2}", d.as_secs_f64())
}

/// A startup function's first call, its start and end from the bootstrap,
/// when it has returned.
fn first(rva: usize) -> Option<(Duration, Duration)> {
    let i = TIMED.iter().position(|t| t.rva == rva)?;
    let (start, end) = (&FIRST[i].0, &FIRST[i].1);
    let (start, end) = (start.load(Ordering::Acquire), end.load(Ordering::Acquire));
    (start != 0 && end != 0).then(|| (Duration::from_micros(start - 1), Duration::from_micros(end - 1)))
}

/// The startup's stretches as one line, at the main menu `now` from the
/// bootstrap; `first` gives each function's first call.
fn summary(now: Duration, first: impl Fn(usize) -> Option<(Duration, Duration)>) -> String {
    let span = |what: &str, span: Option<(Duration, Duration)>| match span {
        Some((start, end)) => format!("{what} {} to {}", secs(start), secs(end)),
        None => format!("{what} not seen"),
    };
    let stretches = [
        span("main up to the engine's Init", first(ENGINE_INIT).map(|(start, _)| (Duration::ZERO, start))),
        span("the engine's Init", first(ENGINE_INIT)),
        span("the gfx stage", first(GFX)),
        span("meshes and particles", first(MESHES)),
        span("the databases", first(DATABASES)),
        span("the events", first(EVENTS)),
        span("flags and loading screens", first(FLAGS)),
        span("the rest of InitGame", first(FLAGS).zip(first(INIT_GAME)).map(|((_, flags), (_, init))| (flags, init))),
        span("the main menu's part", first(MAIN_MENU_PART)),
    ];
    format!("loading: the main menu at {} s; from, to in s: {}", secs(now), stretches.join("; "))
}

/// Logs the startup's stretches, once, from the main menu.
pub fn log_main_menu() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| log!("{}", summary(crate::since_bootstrap(), first)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_derives_the_stretches_between_the_functions() {
        let s = |a: f64, b: f64| Some((Duration::from_secs_f64(a), Duration::from_secs_f64(b)));
        let first = |rva| match rva {
            ENGINE_INIT => s(2.6, 2.9),
            GFX => s(3.0, 7.5),
            MESHES => s(5.3, 7.5),
            DATABASES => s(7.7, 13.7),
            EVENTS => s(13.7, 17.6),
            FLAGS => s(17.6, 18.2),
            INIT_GAME => s(7.6, 20.6),
            _ => None,
        };
        assert_eq!(
            summary(Duration::from_secs_f64(25.1), first),
            "loading: the main menu at 25.10 s; from, to in s: main up to the engine's Init 0.00 to 2.60; the engine's Init 2.60 to 2.90; \
             the gfx stage 3.00 to 7.50; meshes and particles 5.30 to 7.50; the databases 7.70 to 13.70; the events 13.70 to 17.60; \
             flags and loading screens 17.60 to 18.20; the rest of InitGame 18.20 to 20.60; the main menu's part not seen"
        );
    }

    #[test]
    fn a_time_of_zero_is_kept_apart_from_not_yet() {
        assert_eq!(micros(Duration::ZERO), 1);
        assert_eq!(micros(Duration::from_micros(41)), 42);
    }
}
