//! The quarantine of what the framework's repairs let go
//! (docs/plans/leak-finding.md, M4): while it is on, a block freed on a thread
//! where a repair is letting memory go ([`begin`] to [`end`]) is kept
//! instead of freed and filled with [`PATTERN`]; every [`EVERY`] what it
//! keeps is compared with the pattern, and the first write to each block is
//! kept with the repair that let it go. A write is memory the game still
//! used after a repair let it go; a read of it gives the pattern's bytes,
//! and a call through them faults at once, where the code reads it.
//!
//! The census's `free` calls [`keep`], and frees what it does not keep.
use super::SiteKey;
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// What a block kept is filled with.
pub const PATTERN: u8 = 0xdd;
/// How often what it keeps is compared.
pub const EVERY: Duration = Duration::from_secs(2);
/// The most bytes it keeps: past it, the oldest blocks are freed.
const MOST_BYTES: usize = 512 << 20;
/// The most writes it keeps.
const MOST_WRITTEN: usize = 100;
/// How many bytes of a write are kept.
const WRITE_BYTES: usize = 16;

static ON: AtomicBool = AtomicBool::new(false);
static CHECKING: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<State>> = Mutex::new(None);

thread_local! {
    /// The repair letting memory go on this thread, and how deep.
    static RELEASING: Cell<(Option<&'static str>, u32)> = const { Cell::new((None, 0)) };
}

struct Block {
    start: usize,
    size: usize,
    site: SiteKey,
    repair: &'static str,
    generation: u32,
}

/// A block written to after a repair let it go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub repair: &'static str,
    pub site: SiteKey,
    pub size: usize,
    pub offset: usize,
    pub bytes: Vec<u8>,
    pub generation: u32,
    pub found: u32,
}

#[derive(Default)]
struct State {
    kept: VecDeque<Block>,
    bytes: usize,
    freed: u64,
    checks: u64,
    /// By repair: the blocks and bytes kept since it was turned on.
    repairs: HashMap<&'static str, (u64, u64)>,
    written: Vec<Found>,
    /// The blocks kept now that were written to, by start.
    written_to: HashSet<usize>,
}

/// What the quarantine holds and found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub on: bool,
    pub blocks: u64,
    pub bytes: u64,
    pub freed: u64,
    pub checks: u64,
    /// By repair, the most bytes first.
    pub repairs: Vec<(&'static str, u64, u64)>,
    pub written: Vec<Found>,
}

fn state() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A repair begins to let memory go on this thread, until [`end`]; one
/// inside another is the outer one's.
pub fn begin(repair: &'static str) {
    RELEASING.with(|r| {
        let (outer, depth) = r.get();
        r.set((outer.or(Some(repair)), depth + 1));
    });
}

/// The repair [`begin`] began is done.
pub fn end() {
    RELEASING.with(|r| {
        let (outer, depth) = r.get();
        r.set(if depth <= 1 { (None, 0) } else { (outer, depth - 1) });
    });
}

/// The repair letting memory go on this thread, while the quarantine is on.
pub fn releasing() -> Option<&'static str> {
    if !ON.load(Ordering::Relaxed) {
        return None;
    }
    RELEASING.with(|r| r.get().0)
}

/// The first byte of `bytes` that is not the pattern, and the bytes from
/// it, at most [`WRITE_BYTES`].
pub fn first_change(bytes: &[u8]) -> Option<(usize, Vec<u8>)> {
    let at = bytes.iter().position(|b| *b != PATTERN)?;
    Some((at, bytes[at..(at + WRITE_BYTES).min(bytes.len())].to_vec()))
}

/// Keeps the block at `start`, `size` bytes, that `repair` lets go, filled
/// with the pattern, unless the quarantine is off; true when it is kept, and
/// then it must not be freed. Past the most it keeps, the oldest blocks go
/// to `free`.
///
/// `start` must be a live block of `size` bytes no one else frees, as a
/// block given to `free` is.
pub fn keep(start: usize, size: usize, site: SiteKey, repair: &'static str, generation: u32, mut free: impl FnMut(usize)) -> bool {
    let mut state = state();
    let Some(state) = state.as_mut() else { return false };
    unsafe { std::ptr::write_bytes(start as *mut u8, PATTERN, size) };
    state.kept.push_back(Block { start, size, site, repair, generation });
    state.bytes += size;
    let kept = state.repairs.entry(repair).or_default();
    kept.0 += 1;
    kept.1 += size as u64;
    while state.bytes > MOST_BYTES {
        let Some(oldest) = state.kept.pop_front() else { break };
        state.bytes -= oldest.size;
        state.written_to.remove(&oldest.start);
        state.freed += 1;
        free(oldest.start);
    }
    true
}

/// Compares every block kept with the pattern, and keeps the first write
/// to each; `generation` is the census's now.
pub fn check(generation: u32) {
    let mut state = state();
    let Some(state) = state.as_mut() else { return };
    state.checks += 1;
    for block in &state.kept {
        if state.written_to.contains(&block.start) {
            continue;
        }
        let bytes = unsafe { std::slice::from_raw_parts(block.start as *const u8, block.size) };
        if let Some((offset, bytes)) = first_change(bytes) {
            state.written_to.insert(block.start);
            if state.written.len() < MOST_WRITTEN {
                state.written.push(Found {
                    repair: block.repair,
                    site: block.site,
                    size: block.size,
                    offset,
                    bytes,
                    generation: block.generation,
                    found: generation,
                });
            }
        }
    }
}

/// Turns the quarantine on, and the comparing with `generation`, the
/// census's generation now, on its own thread.
pub fn start(generation: fn() -> u32) {
    let mut state = state();
    if state.is_none() {
        *state = Some(State::default());
    }
    ON.store(true, Ordering::Relaxed);
    if !CHECKING.swap(true, Ordering::Relaxed) {
        std::thread::spawn(move || loop {
            std::thread::sleep(EVERY);
            if ON.load(Ordering::Relaxed) {
                check(generation());
            }
        });
    }
}

/// Turns the quarantine off: what it keeps is compared once more, then
/// goes to `free`. Answers what it held and found.
pub fn stop(generation: u32, mut free: impl FnMut(usize)) -> Report {
    ON.store(false, Ordering::Relaxed);
    check(generation);
    let report = report();
    if let Some(state) = state().take() {
        state.kept.into_iter().for_each(|b| free(b.start));
    }
    Report { on: false, ..report }
}

/// What it holds and found.
pub fn report() -> Report {
    let state = state();
    let Some(state) = state.as_ref() else { return Report::default() };
    let mut repairs: Vec<(&'static str, u64, u64)> = state.repairs.iter().map(|(r, (blocks, bytes))| (*r, *blocks, *bytes)).collect();
    repairs.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
    Report {
        on: ON.load(Ordering::Relaxed),
        blocks: state.kept.len() as u64,
        bytes: state.bytes as u64,
        freed: state.freed,
        checks: state.checks,
        repairs,
        written: state.written.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_byte_changed_is_found_with_those_after_it() {
        let mut bytes = vec![PATTERN; 40];
        assert_eq!(first_change(&bytes), None);
        bytes[8..12].copy_from_slice(&[1, 2, 3, 4]);
        let (at, found) = first_change(&bytes).unwrap();
        assert_eq!(at, 8);
        assert_eq!(found.len(), WRITE_BYTES);
        assert_eq!(&found[..5], &[1, 2, 3, 4, PATTERN]);
        bytes[39] = 0;
        assert_eq!(first_change(&bytes[36..]), Some((3, vec![0])));
    }

    #[test]
    fn a_repair_inside_another_is_the_outer_one_s() {
        // The quarantine is off in tests: releasing() answers nothing.
        assert_eq!(releasing(), None);
        begin("outer");
        begin("inner");
        assert_eq!(RELEASING.with(|r| r.get()), (Some("outer"), 2));
        end();
        assert_eq!(RELEASING.with(|r| r.get()), (Some("outer"), 1));
        end();
        end();
        assert_eq!(RELEASING.with(|r| r.get()), (None, 0));
    }

    #[test]
    fn a_block_kept_is_filled_and_a_write_to_it_found_once() {
        // Static state: one test keeps blocks, with buffers of its own.
        let mut a = vec![7u8; 64];
        let mut b = vec![7u8; 32];
        let mut freed = Vec::new();
        assert!(!keep(a.as_mut_ptr() as usize, 64, 1, "first", 0, |s| freed.push(s)));
        *state() = Some(State::default());
        assert!(keep(a.as_mut_ptr() as usize, 64, 1, "first", 3, |s| freed.push(s)));
        assert!(keep(b.as_mut_ptr() as usize, 32, 2, "second", 3, |s| freed.push(s)));
        assert!(a.iter().chain(&b).all(|x| *x == PATTERN));
        check(4);
        a[20] = 9;
        check(5);
        check(6);
        let report = report();
        assert_eq!((report.blocks, report.bytes, report.checks, report.freed), (2, 96, 3, 0));
        assert_eq!(report.repairs, [("first", 1, 64), ("second", 1, 32)]);
        assert_eq!(report.written.len(), 1);
        let found = &report.written[0];
        assert_eq!((found.repair, found.site, found.size, found.offset, found.generation, found.found), ("first", 1, 64, 20, 3, 5));
        let report = stop(7, |s| freed.push(s));
        assert!(!report.on);
        assert_eq!(freed, [a.as_ptr() as usize, b.as_ptr() as usize]);
        assert_eq!(super::report(), Report::default());
    }
}
