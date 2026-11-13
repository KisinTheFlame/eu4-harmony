//! In-process coordination of prepared frames, main-thread fallback, frame
//! pacing, crossing hold-back, game lock ordering, deferred-call reentry, and
//! late-frame gap accounting, tested across multi-frame courses without the
//! game's threads or Direct3D device.
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

/// Coordinates publication, invalidation, pacing, and drawing eligibility of
/// prepared frames between the main thread and the drawing thread.
pub(super) struct FrameCoordinator {
    prepared_idler: AtomicUsize,
    prepared_seq: AtomicU64,
    drawn_seq: AtomicU64,
    day_ran: AtomicBool,
    invalidated_by_main: AtomicBool,
}

impl FrameCoordinator {
    pub(super) const fn new() -> Self {
        Self {
            prepared_idler: AtomicUsize::new(0),
            prepared_seq: AtomicU64::new(0),
            drawn_seq: AtomicU64::new(0),
            day_ran: AtomicBool::new(false),
            invalidated_by_main: AtomicBool::new(false),
        }
    }

    pub(super) fn note_day_ran(&self) {
        self.day_ran.store(true, Ordering::Release);
    }

    pub(super) fn take_day_ran(&self) -> bool {
        self.day_ran.swap(false, Ordering::AcqRel)
    }

    pub(super) fn prepared_idler(&self) -> usize {
        self.prepared_idler.load(Ordering::Acquire)
    }

    pub(super) fn drawn_caught_up(&self, wanted: u64) -> bool {
        self.drawn_seq.load(Ordering::Acquire) >= wanted
    }

    /// Returns the sequence number a paused/non-day main-thread frame should
    /// wait for the drawing thread to draw, or `None` when pacing must skip
    /// (because no newer frame is prepared, a crossing or main-drawn frame
    /// invalidated the prepared frame, or the main thread cannot pace).
    pub(super) fn pace_target(&self, drawing_on: bool, holding_back: bool, may_pace: bool) -> Option<u64> {
        let wanted = self.prepared_seq.load(Ordering::Acquire);
        if !drawing_on || holding_back || !may_pace || self.invalidated_by_main.load(Ordering::Acquire) || self.drawn_caught_up(wanted) {
            return None;
        }
        Some(wanted)
    }

    /// Marks the previous prepared frame stale because the main thread is
    /// drawing this frame itself, without recording a fake crossing.
    pub(super) fn main_draws_frame(&self) {
        self.invalidated_by_main.store(true, Ordering::Release);
    }

    /// Publishes a freshly prepared frame for `idler` and returns its sequence.
    pub(super) fn publish_prepared(&self, idler: usize) -> u64 {
        self.invalidated_by_main.store(false, Ordering::Release);
        self.prepared_idler.store(idler, Ordering::Release);
        self.prepared_seq.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Clears the prepared idler when the game switches idlers or quits.
    pub(super) fn clear_idler(&self) {
        self.prepared_idler.store(0, Ordering::Release);
        self.invalidated_by_main.store(false, Ordering::Release);
    }

    /// Returns `(idler, seq)` when the drawing thread may draw the prepared
    /// frame right now.
    pub(super) fn drawable(&self, current_idler: usize, holding_back: Option<usize>, must_draw_here: bool) -> Option<(usize, u64)> {
        let idler = self.prepared_idler.load(Ordering::Acquire);
        let seq = self.prepared_seq.load(Ordering::Acquire);
        if idler == 0 || current_idler != idler || holding_back.is_some() || must_draw_here || self.invalidated_by_main.load(Ordering::Acquire) {
            return None;
        }
        Some((idler, seq))
    }

    pub(super) fn note_drawn(&self, seq: u64) {
        self.drawn_seq.store(seq, Ordering::Release);
    }
}

/// Tracks crossings that invalidate the prepared frame until the next update
/// and aggregates crossing counts and hold-back durations for the log.
pub(super) struct CrossingsTracker {
    changed: AtomicBool,
    held_back_by: Mutex<Option<(usize, Instant)>>,
    taken: AtomicU64,
    by_crossing: Mutex<Vec<(usize, u64, u64)>>,
}

impl CrossingsTracker {
    pub(super) const fn new() -> Self {
        Self { changed: AtomicBool::new(false), held_back_by: Mutex::new(None), taken: AtomicU64::new(0), by_crossing: Mutex::new(Vec::new()) }
    }

    pub(super) fn record_crossing(&self, into: usize, now: Instant, took_lock: bool) {
        if !self.changed.swap(true, Ordering::AcqRel) {
            *self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()) = Some((into, now));
        }
        self.count(into, 1, 0);
        if took_lock {
            self.taken.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn record_lock_order(&self, into: usize, took_lock: bool) {
        self.count(into, 1, 0);
        if took_lock {
            self.taken.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn holding_back(&self) -> Option<usize> {
        self.changed.load(Ordering::Acquire).then(|| self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()).map_or(0, |(into, _)| into))
    }

    pub(super) fn frame_prepared(&self, now: Instant) {
        self.changed.store(false, Ordering::Release);
        if let Some((into, at)) = self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.count(into, 0, now.saturating_duration_since(at).as_micros() as u64);
        }
    }

    pub(super) fn clear(&self) {
        self.changed.store(false, Ordering::Release);
        self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    fn count(&self, into: usize, times: u64, held: u64) {
        let mut by = self.by_crossing.lock().unwrap_or_else(|e| e.into_inner());
        match by.iter_mut().find(|c| c.0 == into) {
            Some(c) => {
                c.1 += times;
                c.2 += held;
            }
            None => by.push((into, times, held)),
        }
    }

    pub(super) fn logged(&self, base: usize) -> (u64, String) {
        let mut by = std::mem::take(&mut *self.by_crossing.lock().unwrap_or_else(|e| e.into_inner()));
        by.sort_by_key(|c| std::cmp::Reverse(c.2));
        let longest = by
            .iter()
            .take(3)
            .map(|&(into, times, held)| format!("{:#x} {times}x {:.0} ms", into.wrapping_sub(base), held as f64 / 1000.0))
            .collect::<Vec<_>>()
            .join(", ");
        (self.taken.swap(0, Ordering::Relaxed), longest)
    }
}

/// Lock-free fixed-capacity set of game locks taken by the drawing thread,
/// consulted without taking a Rust mutex inside `EnterCriticalSection` or
/// `AcquireSRWLockExclusive`.
pub(super) struct LockOrder<const N: usize> {
    locks: [AtomicUsize; N],
    known: AtomicUsize,
    overflow_logged: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LockAction {
    None,
    WarnOverflow,
    TakePresentationFirst,
}

impl<const N: usize> LockOrder<N> {
    pub(super) const fn new() -> Self {
        Self { locks: [const { AtomicUsize::new(0) }; N], known: AtomicUsize::new(0), overflow_logged: AtomicBool::new(false) }
    }

    pub(super) fn observe(&self, lock: usize, on_drawing: bool, on_main_unheld: bool) -> LockAction {
        let count = self.known.load(Ordering::Acquire).min(N);
        let known = self.locks[..count].iter().any(|l| l.load(Ordering::Relaxed) == lock);
        if on_drawing {
            if !known && count < N {
                self.locks[count].store(lock, Ordering::Relaxed);
                self.known.store(count + 1, Ordering::Release);
            } else if !known && !self.overflow_logged.swap(true, Ordering::Relaxed) {
                return LockAction::WarnOverflow;
            }
            LockAction::None
        } else if known && on_main_unheld {
            LockAction::TakePresentationFirst
        } else {
            LockAction::None
        }
    }
}

/// Bounded queue and reentry guard for main-thread calls deferred to frame
/// preparation.
pub(super) struct DeferredQueue<T> {
    items: Mutex<Vec<T>>,
    count: AtomicU64,
    doing: AtomicBool,
}

pub(super) struct DoingGuard<'a, T>(&'a DeferredQueue<T>);

impl<T> Drop for DoingGuard<'_, T> {
    fn drop(&mut self) {
        self.0.doing.store(false, Ordering::Release);
    }
}

impl<T> DeferredQueue<T> {
    pub(super) const fn new() -> Self {
        Self { items: Mutex::new(Vec::new()), count: AtomicU64::new(0), doing: AtomicBool::new(false) }
    }

    pub(super) fn is_doing(&self) -> bool {
        self.doing.load(Ordering::Acquire)
    }

    pub(super) fn push(&self, item: T) {
        self.items.lock().unwrap_or_else(|e| e.into_inner()).push(item);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn push_if_absent(&self, item: T, mut same: impl FnMut(&T) -> bool) {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        if !items.iter().any(&mut same) {
            items.push(item);
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn take_for_execution(&self) -> Option<(Vec<T>, DoingGuard<'_, T>)> {
        let taken = std::mem::take(&mut *self.items.lock().unwrap_or_else(|e| e.into_inner()));
        if taken.is_empty() {
            return None;
        }
        self.doing.store(true, Ordering::Release);
        Some((taken, DoingGuard(self)))
    }

    pub(super) fn logged(&self) -> u64 {
        self.count.swap(0, Ordering::Relaxed)
    }
}

const LATE_BY: [&str; 6] = ["held back", "the lock before", "drawing", "Present", "the lock after Present", "other"];

/// Late-frame gap accounting across ten-second windows.
#[derive(Default)]
pub(super) struct Gaps {
    last: Option<Instant>,
    held: Option<usize>,
    parts: [Duration; 4],
    late: [u32; 6],
    by: Vec<(usize, u32)>,
    longest: Duration,
    drawing: (Duration, u32, Duration),
}

impl Gaps {
    pub(super) fn held_by(&mut self, into: usize) {
        self.held.get_or_insert(into);
    }

    pub(super) fn waited(&mut self, waited: Duration) {
        self.parts[0] += waited;
    }

    pub(super) fn drawn(&mut self, now: Instant, interval: Duration, drawing: Duration, present: Duration, lock_after: Duration) {
        let own = drawing.saturating_sub(present + lock_after);
        self.parts[1] += own;
        self.drawing = (self.drawing.0 + own, self.drawing.1 + 1, self.drawing.2.max(own));
        self.parts[2] += present;
        self.parts[3] += lock_after;
        if let Some(last) = self.last.replace(now) {
            let gap = now.saturating_duration_since(last);
            if gap > interval * 2 {
                let rest = gap.saturating_sub(self.parts.iter().sum());
                let why = match self.held {
                    Some(_) => 0,
                    None => {
                        let parts = [self.parts[0], self.parts[1], self.parts[2], self.parts[3], rest];
                        1 + (0..parts.len()).max_by_key(|&i| parts[i]).unwrap_or(4)
                    }
                };
                self.late[why] += 1;
                if let Some(into) = self.held {
                    match self.by.iter_mut().find(|b| b.0 == into) {
                        Some(b) => b.1 += 1,
                        None => self.by.push((into, 1)),
                    }
                }
            }
            self.longest = self.longest.max(gap);
        }
        self.held = None;
        self.parts = [Duration::ZERO; 4];
    }

    pub(super) fn told(&mut self, base: usize) -> String {
        self.by.sort_by_key(|b| std::cmp::Reverse(b.1));
        let by: Vec<String> = self.by.iter().take(3).map(|(into, n)| format!("{:#x} {n}", into.wrapping_sub(base))).collect();
        let others: Vec<String> = (1..LATE_BY.len()).map(|i| format!("{} {}", LATE_BY[i], self.late[i])).collect();
        let (all, frames, longest) = self.drawing;
        let told = format!(
            "{} late (held back {} by {}, {}), the longest gap {:.0} ms; a frame's drawing {:.1} ms, the longest {:.1} ms",
            self.late.iter().sum::<u32>(),
            self.late[0],
            by.join(", "),
            others.join(", "),
            self.longest.as_secs_f64() * 1000.0,
            all.as_secs_f64() * 1000.0 / frames.max(1) as f64,
            longest.as_secs_f64() * 1000.0
        );
        self.drawing = Default::default();
        self.late = [0; 6];
        self.by.clear();
        self.longest = Duration::ZERO;
        told
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_coordinator_handles_publication_pacing_crossings_main_fallback_and_idler_switches() {
        let frames = FrameCoordinator::new();
        let crossings = CrossingsTracker::new();
        let start = Instant::now();

        assert_eq!(frames.drawable(0x100, None, false), None);
        let seq1 = frames.publish_prepared(0x100);
        assert_eq!(seq1, 1);
        assert_eq!(frames.pace_target(true, false, true), Some(1));
        assert_eq!(frames.drawable(0x100, None, false), Some((0x100, 1)));

        // Once drawn, pacing stops waiting until a newer frame is prepared.
        frames.note_drawn(1);
        assert_eq!(frames.pace_target(true, false, true), None);

        // A crossing during the day holds the prepared frame back and prevents pacing spin/stall.
        let seq2 = frames.publish_prepared(0x100);
        assert_eq!(seq2, 2);
        crossings.record_crossing(0x1400_1234, start, true);
        assert_eq!(crossings.holding_back(), Some(0x1400_1234));
        assert_eq!(frames.drawable(0x100, crossings.holding_back(), false), None);
        assert_eq!(frames.pace_target(true, crossings.holding_back().is_some(), true), None);

        // Preparing the next frame clears the crossing hold-back and records its duration.
        crossings.frame_prepared(start + Duration::from_millis(12));
        assert_eq!(crossings.holding_back(), None);
        let (taken, summary) = crossings.logged(0x1400_0000);
        assert_eq!(taken, 1);
        assert!(summary.contains("0x1234 1x 12 ms"), "{summary}");

        // Main-thread fallback invalidates the old prepared frame without faking a crossing.
        frames.main_draws_frame();
        assert_eq!(frames.drawable(0x100, None, false), None);
        assert_eq!(frames.pace_target(true, false, true), None);
        assert_eq!(crossings.logged(0).0, 0);

        // Next prepared frame restores drawing; idler switch clears it immediately.
        let seq3 = frames.publish_prepared(0x200);
        assert_eq!(frames.drawable(0x200, None, false), Some((0x200, seq3)));
        frames.clear_idler();
        assert_eq!(frames.drawable(0x200, None, false), None);
    }

    #[test]
    fn lock_order_learns_drawing_locks_warns_once_on_overflow_and_orders_unheld_main_thread() {
        let order: LockOrder<2> = LockOrder::new();
        assert_eq!(order.observe(0xaa, false, true), LockAction::None);
        assert_eq!(order.observe(0xaa, true, false), LockAction::None);
        assert_eq!(order.observe(0xbb, true, false), LockAction::None);
        assert_eq!(order.observe(0xcc, true, false), LockAction::WarnOverflow);
        assert_eq!(order.observe(0xdd, true, false), LockAction::None, "overflow warns only once");
        assert_eq!(order.observe(0xaa, false, true), LockAction::TakePresentationFirst);
        assert_eq!(order.observe(0xaa, false, false), LockAction::None, "main already holds presentation lock");
        assert_eq!(order.observe(0xcc, false, true), LockAction::None, "unknown lock untouched");
    }

    #[test]
    fn deferred_queue_deduplicates_and_resets_doing_even_on_unwind() {
        let queue: DeferredQueue<(usize, u32)> = DeferredQueue::new();
        queue.push_if_absent((10, 1), |x| *x == (10, 1));
        queue.push_if_absent((10, 1), |x| *x == (10, 1));
        queue.push_if_absent((10, 2), |x| *x == (10, 2));
        assert_eq!(queue.logged(), 2);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (items, _guard) = queue.take_for_execution().unwrap();
            assert_eq!(items.len(), 2);
            assert!(queue.is_doing());
            panic!("simulated unwind during deferred execution");
        }));
        assert!(result.is_err());
        assert!(!queue.is_doing(), "RAII guard must clear `doing` on unwind");
        assert!(queue.take_for_execution().is_none());
    }

    #[test]
    fn gaps_attributes_late_frames_to_crossing_or_dominant_stage() {
        let mut gaps = Gaps::default();
        let t0 = Instant::now();
        let interval = Duration::from_millis(10);
        gaps.drawn(t0, interval, Duration::from_millis(2), Duration::from_millis(1), Duration::ZERO);

        // Late frame caused by a crossing hold-back.
        gaps.held_by(0x1400_0500);
        gaps.drawn(t0 + Duration::from_millis(30), interval, Duration::from_millis(3), Duration::from_millis(1), Duration::ZERO);

        // Late frame dominated by Present wait.
        gaps.drawn(t0 + Duration::from_millis(65), interval, Duration::from_millis(28), Duration::from_millis(25), Duration::from_millis(1));

        let line = gaps.told(0x1400_0000);
        assert!(line.starts_with("2 late (held back 1 by 0x500 1,"), "{line}");
        assert!(line.contains("Present 1"), "{line}");
    }
}
