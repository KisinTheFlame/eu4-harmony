//! The drawing thread's watch (dev.toml's `[write-watch]` `drawing`): which
//! writes land in what the drawing thread reads, and whose
//! (docs/plans/drawing-thread.md; docs/dev-tools.md, "Write watch").
//!
//! 1. **What the drawing reads.** Once `after-days` game days have passed, the
//!    drawing thread's next `drawing-frames` frames are watched as a function
//!    is: every other thread held but `let-run`'s (the graphics driver's,
//!    whose own threads the drawing waits on), the heap's pages and
//!    `eu4.exe`'s data inaccessible. What `eu4.exe`'s own code reads is kept
//!    (the blocks by their start, the data by its eight bytes); what the
//!    graphics runtime and the driver read of their own is not, so that the
//!    second part does not make their memory read-only under the kernel.
//! 2. **Who writes it.** From the next day on, for `drawing-days` days, the
//!    pages of what was read are read-only while every thread runs, and each
//!    write to a block or bytes read is recorded by who wrote it: the main
//!    thread without the presentation lock (a race with the drawing thread,
//!    unless the drawing does not read it while the main thread may write
//!    it), another thread (the AI's workers and the rest: a race as well), the
//!    main thread with the lock, or the drawing thread itself. The report
//!    lists the first two first.
//!
//! The game runs many times slower while watched: the watch is for finding
//! out, not for play. A block freed and made again at the same start between
//! the two parts passes for the one read.
use super::{report, start_on, stop, Watch, GAME_READS_ONLY, STARTED, WHO_WROTE};
use crate::memory::Memory;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

const OFF: u8 = 0;
const WAITING: u8 = 1;
const READING: u8 = 2;
const READ: u8 = 3;
const WATCHING: u8 = 4;
const DONE: u8 = 5;

static PHASE: AtomicU8 = AtomicU8::new(OFF);
static DAYS_PASSED: AtomicUsize = AtomicUsize::new(0);
static FRAMES_READ: AtomicUsize = AtomicUsize::new(0);
static DAYS_WATCHED: AtomicUsize = AtomicUsize::new(0);

/// The watch is asked for: it waits for its days.
pub(super) fn on() {
    PHASE.store(WAITING, Ordering::Release);
}

/// A day begins, on the main thread, holding the presentation lock.
pub fn day_begins(m: &Memory) {
    let options = &crate::dev::options().write_watch;
    match PHASE.load(Ordering::Acquire) {
        WAITING if DAYS_PASSED.fetch_add(1, Ordering::Relaxed) + 1 >= options.after_days => {
            PHASE.store(READING, Ordering::Release);
            log!("write watch: the drawing thread's next {} frame(s) watched for what they read", options.drawing_frames);
        }
        READ => {
            let watch = Watch { reads: false, first_read: false, thread: crate::main_thread(), hold: false, only_seen: true };
            WHO_WROTE.store(true, Ordering::Release);
            match start_on(m, watch) {
                Ok(what) => {
                    *STARTED.lock().unwrap_or_else(|e| e.into_inner()) = what;
                    PHASE.store(WATCHING, Ordering::Release);
                }
                Err(e) => {
                    stop();
                    WHO_WROTE.store(false, Ordering::Release);
                    PHASE.store(DONE, Ordering::Release);
                    log!("write watch: what the drawing read not watched: {e}");
                }
            }
        }
        WATCHING if DAYS_WATCHED.fetch_add(1, Ordering::Relaxed) + 1 >= options.drawing_days => {
            stop();
            report(crate::drawing::drawing_rva(), 0, false);
            WHO_WROTE.store(false, Ordering::Release);
            PHASE.store(DONE, Ordering::Release);
            log!("write watch: done, {} day(s) watched", options.drawing_days);
        }
        _ => {}
    }
}

/// The drawing thread is about to draw a frame: watched for what it reads,
/// while the first part wants frames.
pub fn frame_begins(m: &Memory) -> bool {
    if PHASE.load(Ordering::Acquire) != READING {
        return false;
    }
    let first = FRAMES_READ.load(Ordering::Relaxed) == 0;
    GAME_READS_ONLY.store(true, Ordering::Release);
    let watch = Watch { reads: true, first_read: first, thread: crate::thread_id(), hold: true, only_seen: false };
    match start_on(m, watch) {
        Ok(what) => {
            *STARTED.lock().unwrap_or_else(|e| e.into_inner()) = what;
            true
        }
        Err(e) => {
            stop();
            GAME_READS_ONLY.store(false, Ordering::Release);
            PHASE.store(DONE, Ordering::Release);
            log!("write watch: the drawing thread's frame not watched: {e}");
            false
        }
    }
}

/// The frame [`frame_begins`] watched is drawn.
pub fn frame_ends(watched: bool) {
    if !watched {
        return;
    }
    stop();
    GAME_READS_ONLY.store(false, Ordering::Release);
    let n = FRAMES_READ.fetch_add(1, Ordering::Relaxed) + 1;
    report(crate::drawing::drawing_rva(), n, true);
    if n >= crate::dev::options().write_watch.drawing_frames {
        PHASE.store(READ, Ordering::Release);
    }
}
