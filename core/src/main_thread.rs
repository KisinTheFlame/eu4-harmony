//! The game's main thread runs on any processor the process may use. The
//! game's run pins it to one, the one in the middle of those it may run on,
//! the same in every game on a computer, so several games side by side share
//! one processor for their main threads. Nothing in the game says why it pins
//! the thread (docs/research/headless.md, "One game per machine"); the call
//! is left out, in every game.
use crate::points::Point;

/// The application's run pins the main thread:
/// `SetThreadAffinityMask(GetCurrentThread(), mask)`, whose result nothing
/// reads.
const MAIN_THREAD_PINNED: usize = 0x14c2db4;
/// `SetThreadAffinityMask`'s import slot.
const SET_THREAD_AFFINITY_IMPORT: usize = 0x1b66850;

pub fn points() -> Vec<Point> {
    vec![Point::write(MAIN_THREAD_PINNED, "ff 15 96 3a 6a 00", "main thread pinned", "66 0f 1f 44 00 00").uses(&[SET_THREAD_AFFINITY_IMPORT])]
}
