//! The presentation lock's turns (GLOSSARY: presentation lock): which thread
//! holds the lock when, as the moments of the game's frames and days come.
//!
//! The main thread holds it through each application frame, but lets it go
//! while the daily update runs, taking it again for a frame inside the day,
//! at the day's end, and at each crossing the day reaches, which keeps it
//! until the next of the game's checks for a frame inside a day. Where the
//! drawing thread may be in a frame of its own, at an idler's switch and as
//! the game quits, the main thread lets the lock go until that frame has
//! ended. The drawing thread holds it for each frame it draws, but for
//! Direct3D's Present, unless the write watch holds every other thread for
//! that frame.
//!
//! The hooks tell [`MainSide`] and [`DrawingSide`] the moments; the lock
//! itself is reached through [`Lock`], the ticket lock in the game
//! (`super::MainLock`, `super::DrawingLock`) and a recorder in the tests.
use std::time::Duration;

/// Where a thread waited for the lock, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum At {
    /// The main thread, at an application frame, an idler's switch or the
    /// game quitting.
    Frame,
    /// The main thread, at a crossing in a day.
    Crossing,
    /// The main thread, at the day's end.
    DayEnd,
    /// The drawing thread, before a frame of its own.
    DrawingFrame,
    /// The drawing thread, after Present.
    AfterPresent,
}

/// The presentation lock, as the turns take and give it.
pub(super) trait Lock {
    /// Takes the lock, waiting for this thread's turn; how long it waited.
    fn take(&self, at: At) -> Duration;
    fn give(&self);
}

/// The main thread's side of the lock: what only the main thread changes.
pub(super) struct MainSide {
    holds: bool,
    /// For each application frame on the way, whether it took the lock.
    frames: Vec<bool>,
    /// The day let the lock go as it began.
    day_gave: bool,
    in_day: bool,
    /// A crossing took the lock, kept until the next check for a frame.
    crossing: bool,
}

impl MainSide {
    pub(super) const fn new() -> Self {
        Self { holds: false, frames: Vec::new(), day_gave: false, in_day: false, crossing: false }
    }

    /// Whether the main thread holds the lock.
    pub(super) fn holds(&self) -> bool {
        self.holds
    }

    /// Whether a frame that runs no day may wait for the drawing thread to
    /// draw the last one: not while the main thread holds the lock (a frame
    /// inside another) or is in a day.
    pub(super) fn may_pace(&self) -> bool {
        !self.holds && !self.in_day
    }

    fn take(&mut self, lock: &impl Lock, at: At) {
        if !self.holds {
            lock.take(at);
            self.holds = true;
        }
    }

    fn give(&mut self, lock: &impl Lock) {
        if self.holds {
            self.holds = false;
            lock.give();
        }
    }

    /// An application frame begins: it holds the lock, taking it unless a
    /// frame it is inside of holds it already.
    pub(super) fn frame_begins(&mut self, lock: &impl Lock) {
        let took = !self.holds;
        self.take(lock, At::Frame);
        self.frames.push(took);
    }

    /// An application frame has returned: the lock goes if that frame took
    /// it, with a crossing's.
    pub(super) fn frame_ends(&mut self, lock: &impl Lock) {
        if self.frames.pop() == Some(true) {
            self.crossing = false;
            self.give(lock);
        }
    }

    /// The daily update begins: it lets the lock go.
    pub(super) fn day_begins(&mut self, lock: &impl Lock) {
        self.in_day = true;
        self.day_gave = self.holds;
        self.give(lock);
    }

    /// The daily update has returned: the frame it ran in has the lock again,
    /// if it had it as the day began.
    pub(super) fn day_ends(&mut self, lock: &impl Lock) {
        if !self.in_day {
            return;
        }
        self.in_day = false;
        self.crossing = false;
        if self.day_gave {
            self.take(lock, At::DayEnd);
            self.day_gave = false;
        }
    }

    /// The day reaches a crossing: the lock first, kept until the next check
    /// for a frame. Whether it was taken here.
    pub(super) fn crossing(&mut self, lock: &impl Lock) -> bool {
        if self.in_day && !self.holds {
            self.take(lock, At::Crossing);
            self.crossing = true;
            return true;
        }
        false
    }

    /// The game checks for a frame inside the day: a crossing's lock goes.
    pub(super) fn frame_check(&mut self, lock: &impl Lock) {
        if self.crossing {
            self.crossing = false;
            self.give(lock);
        }
    }

    /// The drawing thread may be in a frame of its own, without the lock in
    /// its Present, which is to end with what it began with: the main thread
    /// lets the lock go until `in_frame` says it has ended, then takes it
    /// again. As an idler is switched.
    pub(super) fn drawing_frame_ends(&mut self, lock: &impl Lock, in_frame: impl Fn() -> bool) {
        if in_frame() {
            self.give(lock);
            while in_frame() {
                std::thread::yield_now();
            }
            self.take(lock, At::Frame);
        }
    }

    /// The game quits: the main thread holds the lock to the end, once the
    /// drawing thread's frame, if it is in one, has ended.
    pub(super) fn quits(&mut self, lock: &impl Lock, in_frame: impl Fn() -> bool) {
        self.take(lock, At::Frame);
        self.drawing_frame_ends(lock, in_frame);
    }
}

/// The drawing thread's side of the lock.
#[derive(Default)]
pub(super) struct DrawingSide {
    /// The lock is kept through Present: in a frame the write watch holds
    /// every other thread for, where the main thread may be held waiting its
    /// turn for the lock, which given to it would never come back.
    keeps_in_present: bool,
}

impl DrawingSide {
    /// The drawing thread's turn begins: it takes the lock; how long it
    /// waited.
    pub(super) fn turn_begins(&mut self, lock: &impl Lock) -> Duration {
        lock.take(At::DrawingFrame)
    }

    /// A frame of the drawing thread's begins, held by the write watch or
    /// not.
    pub(super) fn frame_begins(&mut self, watched: bool) {
        self.keeps_in_present = watched;
    }

    /// The frame goes into Present, which touches only the device: the lock
    /// goes for it, unless it is kept.
    pub(super) fn present_begins(&self, lock: &impl Lock) {
        if !self.keeps_in_present {
            lock.give();
        }
    }

    /// Present has returned: the lock again, for what the drawing does
    /// after it; how long it waited.
    pub(super) fn present_ends(&self, lock: &impl Lock) -> Duration {
        if self.keeps_in_present {
            return Duration::ZERO;
        }
        lock.take(At::AfterPresent)
    }

    /// The frame's drawing has returned.
    pub(super) fn frame_ends(&mut self) {
        self.keeps_in_present = false;
    }

    /// The drawing thread's turn ends: the lock goes.
    pub(super) fn turn_ends(&mut self, lock: &impl Lock) {
        lock.give();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Done {
        Took(At),
        Gave,
    }
    use Done::{Gave, Took};

    /// The lock as a record of what was done to it.
    #[derive(Default)]
    struct Recorder(RefCell<Vec<Done>>);

    impl Lock for Recorder {
        fn take(&self, at: At) -> Duration {
            self.0.borrow_mut().push(Took(at));
            Duration::ZERO
        }
        fn give(&self) {
            self.0.borrow_mut().push(Gave);
        }
    }

    impl Recorder {
        /// What was done since the last time asked.
        fn done(&self) -> Vec<Done> {
            std::mem::take(&mut self.0.borrow_mut())
        }
    }

    #[test]
    fn a_frame_takes_the_lock_and_gives_it_back() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        assert!(main.holds() && !main.may_pace());
        assert_eq!(lock.done(), [Took(At::Frame)]);
        main.frame_ends(&lock);
        assert!(!main.holds() && main.may_pace());
        assert_eq!(lock.done(), [Gave]);
    }

    #[test]
    fn a_frame_inside_a_frame_leaves_the_lock_to_the_outer_one() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        lock.done();
        main.frame_begins(&lock);
        main.frame_ends(&lock);
        assert!(main.holds(), "the outer frame still holds it");
        assert_eq!(lock.done(), []);
        main.frame_ends(&lock);
        assert_eq!(lock.done(), [Gave]);
    }

    #[test]
    fn a_day_lets_the_lock_go_and_takes_it_back_at_its_end() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        lock.done();
        main.day_begins(&lock);
        assert!(!main.holds() && !main.may_pace(), "a day does not pace");
        assert_eq!(lock.done(), [Gave]);
        // The check for a frame with no crossing gives nothing.
        main.frame_check(&lock);
        assert_eq!(lock.done(), []);
        main.day_ends(&lock);
        assert!(main.holds());
        assert_eq!(lock.done(), [Took(At::DayEnd)]);
        // A day's end heard again changes nothing.
        main.day_ends(&lock);
        assert_eq!(lock.done(), []);
        main.frame_ends(&lock);
        assert_eq!(lock.done(), [Gave]);
    }

    #[test]
    fn a_day_begun_without_the_lock_does_not_take_it_at_its_end() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.day_begins(&lock);
        main.day_ends(&lock);
        assert!(!main.holds());
        assert_eq!(lock.done(), []);
    }

    #[test]
    fn a_crossing_in_a_day_takes_the_lock_until_the_next_check_for_a_frame() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        main.day_begins(&lock);
        lock.done();
        assert!(main.crossing(&lock));
        assert!(main.holds());
        assert_eq!(lock.done(), [Took(At::Crossing)]);
        // A second crossing finds the lock held.
        assert!(!main.crossing(&lock));
        assert_eq!(lock.done(), []);
        main.frame_check(&lock);
        assert!(!main.holds());
        assert_eq!(lock.done(), [Gave]);
        main.frame_check(&lock);
        assert_eq!(lock.done(), [], "given once");
    }

    #[test]
    fn a_crossing_outside_a_day_takes_nothing() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        assert!(!main.crossing(&lock));
        main.frame_begins(&lock);
        lock.done();
        assert!(!main.crossing(&lock));
        main.frame_check(&lock);
        assert!(main.holds(), "the frame's lock is not a crossing's");
        assert_eq!(lock.done(), []);
    }

    #[test]
    fn a_frame_inside_the_day_after_a_crossing_ends_with_the_crossing_s_lock() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        main.day_begins(&lock);
        main.crossing(&lock);
        lock.done();
        // The frame inside the day finds the lock held and leaves it.
        main.frame_begins(&lock);
        main.frame_ends(&lock);
        assert!(main.holds());
        assert_eq!(lock.done(), []);
        main.frame_check(&lock);
        assert_eq!(lock.done(), [Gave]);
        // A frame inside the day with no crossing takes and gives it.
        main.frame_begins(&lock);
        main.frame_ends(&lock);
        assert_eq!(lock.done(), [Took(At::Frame), Gave]);
        main.day_ends(&lock);
        main.frame_ends(&lock);
        assert_eq!(lock.done(), [Took(At::DayEnd), Gave]);
    }

    #[test]
    fn a_frame_inside_the_day_forgets_the_crossing_it_took_the_lock_after() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        main.day_begins(&lock);
        lock.done();
        // A frame inside the day takes the lock, and a crossing in it finds
        // it held; the frame gives it, and the check after has none to give.
        main.frame_begins(&lock);
        assert!(!main.crossing(&lock));
        main.frame_ends(&lock);
        main.frame_check(&lock);
        assert_eq!(lock.done(), [Took(At::Frame), Gave]);
        main.day_ends(&lock);
        assert_eq!(lock.done(), [Took(At::DayEnd)]);
    }

    #[test]
    fn the_day_s_end_forgets_a_crossing_s_lock_and_keeps_it_for_the_frame() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        main.day_begins(&lock);
        main.crossing(&lock);
        lock.done();
        main.day_ends(&lock);
        assert!(main.holds());
        assert_eq!(lock.done(), [], "the crossing's lock is the frame's now");
        main.frame_check(&lock);
        assert_eq!(lock.done(), []);
        main.frame_ends(&lock);
        assert_eq!(lock.done(), [Gave]);
    }

    /// `in_frame` answering true `n` times, then false.
    fn in_frame_for(n: u32) -> impl Fn() -> bool {
        let left = Cell::new(n);
        move || {
            let now = left.get();
            left.set(now.saturating_sub(1));
            now > 0
        }
    }

    #[test]
    fn an_idler_switch_lets_the_drawing_thread_s_frame_end_first() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        lock.done();
        main.drawing_frame_ends(&lock, in_frame_for(3));
        assert!(main.holds());
        assert_eq!(lock.done(), [Gave, Took(At::Frame)]);
        main.drawing_frame_ends(&lock, in_frame_for(0));
        assert_eq!(lock.done(), [], "no frame of the drawing thread's");
    }

    #[test]
    fn quitting_holds_the_lock_to_the_end_once_the_drawing_thread_s_frame_ended() {
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.day_begins(&lock);
        main.quits(&lock, in_frame_for(2));
        assert!(main.holds());
        assert_eq!(lock.done(), [Took(At::Frame), Gave, Took(At::Frame)]);
        let (lock, mut main) = (Recorder::default(), MainSide::new());
        main.frame_begins(&lock);
        lock.done();
        main.quits(&lock, in_frame_for(0));
        assert!(main.holds());
        assert_eq!(lock.done(), [], "held already, and no frame to wait for");
    }

    #[test]
    fn the_drawing_thread_lets_the_lock_go_for_present() {
        let (lock, mut drawing) = (Recorder::default(), DrawingSide::default());
        drawing.turn_begins(&lock);
        drawing.frame_begins(false);
        drawing.present_begins(&lock);
        drawing.present_ends(&lock);
        drawing.frame_ends();
        drawing.turn_ends(&lock);
        assert_eq!(lock.done(), [Took(At::DrawingFrame), Gave, Took(At::AfterPresent), Gave]);
    }

    #[test]
    fn the_drawing_thread_keeps_the_lock_through_present_in_a_watched_frame() {
        let (lock, mut drawing) = (Recorder::default(), DrawingSide::default());
        drawing.turn_begins(&lock);
        drawing.frame_begins(true);
        drawing.present_begins(&lock);
        assert_eq!(drawing.present_ends(&lock), Duration::ZERO);
        drawing.frame_ends();
        drawing.turn_ends(&lock);
        assert_eq!(lock.done(), [Took(At::DrawingFrame), Gave]);
        // The next frame, not watched, lets it go again.
        drawing.turn_begins(&lock);
        drawing.frame_begins(false);
        drawing.present_begins(&lock);
        drawing.present_ends(&lock);
        assert_eq!(lock.done(), [Took(At::DrawingFrame), Gave, Took(At::AfterPresent)]);
    }
}
