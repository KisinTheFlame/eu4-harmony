//! Bounded event collection on the game's clock, with deterministic replay.
//! Hot calls keep timestamps and associations only; report calculation happens
//! after taking the collection out of the lock. No game memory is read here.
use crate::game::{fail, GResult, Status};
use eu4_harmony_remote::performance::{Observed, Report};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

mod account;

/// At 144 submissions/s this holds ten minutes plus updates and day events.
/// An unlimited drawing rate may exceed it; overflow is explicit in the report.
const MAX_EVENTS: usize = 262_144;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<State> = Mutex::new(State::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Context {
    session: u64,
    speed: u32,
    paused: bool,
    target_us: Option<u64>,
}

struct State {
    context: Option<Context>,
    generation: u64,
    window: Option<Window>,
}

impl State {
    const fn new() -> Self {
        Self { context: None, generation: 0, window: None }
    }
}

struct Window {
    generation: u64,
    began: Instant,
    seconds: u32,
    initial: Context,
    current: Context,
    next_update: u64,
    prepared: Option<u64>,
    events: Vec<Event>,
    dropped: u64,
    truncated_at: Option<u64>,
    observed: Observed,
    capacity: usize,
}

impl Window {
    fn new(generation: u64, began: Instant, seconds: u32, context: Context, capacity: usize) -> Self {
        Self {
            generation,
            began,
            seconds,
            initial: context,
            current: context,
            next_update: 0,
            prepared: None,
            events: Vec::with_capacity(capacity),
            dropped: 0,
            truncated_at: None,
            observed: Observed::default(),
            capacity,
        }
    }

    fn elapsed(&self, now: Instant) -> u64 {
        micros(now.saturating_duration_since(self.began))
    }

    fn deadline(&self) -> u64 {
        u64::from(self.seconds) * 1_000_000
    }

    fn push(&mut self, at: u64, kind: Kind) {
        match &kind {
            Kind::DayEnds { faulted: false, .. } => self.observed.completed_days += 1,
            Kind::Submitted { success: true, .. } => self.observed.successful_submissions += 1,
            Kind::Submitted { success: false, .. } => self.observed.failed_submissions += 1,
            Kind::Prepared { .. } => self.observed.prepared_updates += 1,
            _ => {}
        }
        if self.events.len() < self.capacity {
            self.events.push(Event { at, kind });
        } else {
            self.dropped += 1;
            self.truncated_at = Some(self.truncated_at.map_or(at, |old| old.min(at)));
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Event {
    at: u64,
    kind: Kind,
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Context(Context),
    DayBegins { date: i32 },
    DayEnds { date: i32, faulted: bool },
    Prepared { id: u64 },
    Invalidated { id: u64 },
    Submitted { began: Option<u64>, update: Option<u64>, success: bool, main: bool },
    Interruption(&'static str),
}

/// An in-flight submission's attribution, captured before releasing the
/// presentation lock. A later preparation cannot change this association.
pub struct Submission(Option<Started>);
struct Started {
    generation: u64,
    at: Instant,
    update: Option<u64>,
}

/// A cheap gate before a caller reads game memory for a measurement.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Updates the initial context even while collection is off. Changes within
/// the window split its timeline rather than mixing unrelated intervals.
pub fn context(session: u64, speed: u32, paused: bool, target_interval: Option<Duration>) {
    let now = Instant::now();
    let context = Context { session, speed, paused, target_us: target_interval.map(micros).filter(|&us| us != 0) };
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.context = Some(context);
    if let Some(window) = state.window.as_mut() {
        let at = window.elapsed(now);
        if at >= window.deadline() {
            ACTIVE.store(false, Ordering::Relaxed);
        } else if context != window.current {
            window.push(at, Kind::Context(context));
            if context.session != window.current.session {
                window.prepared = None;
            }
            window.current = context;
        }
    }
}

/// Starts one window; even an expired window must be collected before a new
/// one can replace it, so a second client cannot silently discard its report.
pub fn begin(seconds: u32) -> GResult<()> {
    if !(1..=600).contains(&seconds) {
        return fail(Status::InvalidArgument, "performance collection lasts from 1 to 600 seconds");
    }
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.window.is_some() {
        return fail(Status::Rejected, "collect the previous performance window with performance.end first");
    }
    let Some(context) = state.context.filter(|c| c.session != 0 && (1..=5).contains(&c.speed)) else {
        return fail(Status::NotReady, "performance collection needs the current game's context");
    };
    state.generation = state.generation.wrapping_add(1);
    // Reserve the bounded storage before the timestamp: its allocation must
    // not be charged to the measured window.
    let mut window = Window::new(state.generation, Instant::now(), seconds, context, MAX_EVENTS);
    window.began = Instant::now();
    state.window = Some(window);
    ACTIVE.store(true, Ordering::Relaxed);
    Ok(())
}

/// Takes the timeline out of the lock before sorting and computing statistics.
/// The deadline, not this request's queueing delay, bounds a finished window.
pub fn end() -> GResult<Report> {
    let (window, now) = {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let Some(window) = state.window.take() else {
            return fail(Status::Rejected, "no performance window to collect");
        };
        ACTIVE.store(false, Ordering::Relaxed);
        (window, now)
    };
    let elapsed = window.elapsed(now).min(window.deadline());
    Ok(account::report(window, elapsed))
}

fn with_window(now: Instant, f: impl FnOnce(&mut Window, u64)) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(window) = state.window.as_mut() else { return };
    // A caller may have observed ACTIVE before the previous window ended,
    // then waited until a new window was installed. Do not backdate it into
    // that new collection.
    if now < window.began {
        return;
    }
    let at = window.elapsed(now);
    if at >= window.deadline() {
        ACTIVE.store(false, Ordering::Relaxed);
        return;
    }
    f(window, at);
}

fn record(kind: Kind) {
    if active() {
        let now = Instant::now();
        with_window(now, |w, at| w.push(at, kind));
    }
}

pub fn day_begins(date: i32) {
    record(Kind::DayBegins { date });
}

pub fn day_ends(date: i32, faulted: bool) {
    record(Kind::DayEnds { date, faulted });
}

/// Publishes an update for both drawing-thread and main-thread drawing paths.
pub fn prepared() {
    if active() {
        let now = Instant::now();
        with_window(now, |w, at| {
            w.next_update += 1;
            let id = w.next_update;
            w.prepared = Some(id);
            w.push(at, Kind::Prepared { id });
        });
    }
}

/// The prepared data is no longer eligible for new submissions. A frame
/// already in flight keeps its token and can still submit that earlier update.
pub fn invalidated() {
    if active() {
        let now = Instant::now();
        with_window(now, |w, at| {
            if let Some(id) = w.prepared.take() {
                w.push(at, Kind::Invalidated { id });
            }
        });
    }
}

pub fn submission_begins() -> Submission {
    if !active() {
        return Submission(None);
    }
    let now = Instant::now();
    let mut started = None;
    with_window(now, |w, _| {
        started = Some(Started { generation: w.generation, at: now, update: w.prepared });
    });
    Submission(started)
}

/// Call immediately after the actual submission returns and before reacquiring
/// the presentation lock. Waiting for this module's lock is excluded as well.
pub fn submission_ends(submission: Submission, success: bool, main_thread: bool) {
    if !active() {
        return;
    }
    let now = Instant::now();
    with_window(now, |w, at| w.submission_ends(at, submission, success, main_thread));
}

impl Window {
    fn submission_ends(&mut self, at: u64, submission: Submission, success: bool, main: bool) {
        let (began, update) = match submission.0 {
            Some(started) if started.generation == self.generation => (Some(self.elapsed(started.at)), started.update),
            // Present may already have been running when collection began,
            // with collection off or with a token from an earlier window.
            // Its return counts here, but neither old association carries over.
            Some(_) | None => (None, None),
        };
        self.push(at, Kind::Submitted { began, update, success, main });
    }
}

/// Explicit point markers; paired labels such as save_begin/save_end retain
/// their timings and remain in the inclusive performance measurements.
pub fn interruption(kind: &'static str) {
    record(Kind::Interruption(kind));
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_from_before_the_window_count_without_old_entry_or_update() {
        let began = Instant::now();
        let context = Context { session: 1, speed: 5, paused: true, target_us: None };
        let mut w = Window::new(2, began, 1, context, 20);
        w.push(0, Kind::Prepared { id: 1 });
        w.submission_ends(1_000, Submission(None), true, false);
        let old_token = || Submission(Some(Started { generation: 1, at: began - Duration::from_millis(1), update: Some(1) }));
        w.submission_ends(2_000, old_token(), true, false);
        w.submission_ends(3_000, old_token(), false, false);
        let current = Submission(Some(Started { generation: 2, at: began + Duration::from_micros(3_500), update: Some(1) }));
        w.submission_ends(4_000, current, true, false);
        let r = account::report(w, 10_000);
        assert!(r.complete);
        assert_eq!(r.observed.successful_submissions, 3);
        assert_eq!(r.observed.failed_submissions, 1);
        let s = &r.segments[0];
        assert_eq!(s.drawing.successful_submissions, 3);
        assert_eq!(s.drawing.failed_submissions, 1);
        assert_eq!(s.drawing.initial_submission_wait_ms, 1.0);
        assert_eq!(s.drawing.unattributed_submissions, 2);
        assert_eq!(s.drawing.call_duration.samples, 1);
        assert_eq!(s.drawing.call_duration.max_ms, Some(0.5));
        assert_eq!(s.interface.submitted_updates, 1);
        assert_eq!(s.interface.first_submission_delay.max_ms, Some(4.0));
        assert_eq!(s.interface.age.unknown_ms, 4.0);
    }

    #[test]
    fn lifecycle_rejects_replacement_caps_the_deadline_and_discards_old_token_associations() {
        assert_eq!(begin(0).unwrap_err().status, Status::InvalidArgument);
        assert_eq!(begin(601).unwrap_err().status, Status::InvalidArgument);
        assert_eq!(begin(1).unwrap_err().status, Status::NotReady);
        context(1, 5, true, Some(Duration::from_millis(10)));
        begin(1).unwrap();
        assert!(active());
        assert_eq!(begin(1).unwrap_err().status, Status::Rejected);
        prepared();
        context(1, 4, false, None);
        assert_eq!(STATE.lock().unwrap().window.as_ref().unwrap().prepared, Some(1));
        let old_token = submission_begins();
        context(2, 4, false, None);
        assert_eq!(STATE.lock().unwrap().window.as_ref().unwrap().prepared, None);
        {
            let mut state = STATE.lock().unwrap();
            state.window.as_mut().unwrap().began = Instant::now() - Duration::from_secs(2);
        }
        prepared();
        assert!(!active());
        assert_eq!(begin(1).unwrap_err().status, Status::Rejected);
        let report = end().unwrap();
        assert_eq!(report.elapsed_ms, 1000.0);
        assert!(report.reached_deadline);
        assert_eq!(report.recorded_events, 3);
        assert_eq!(end().unwrap_err().status, Status::Rejected);
        begin(1).unwrap();
        submission_ends(old_token, true, false);
        // Keep the observed return strictly before the half-open window's
        // end without depending on the clock advancing between these calls.
        STATE.lock().unwrap().window.as_mut().unwrap().began -= Duration::from_millis(1);
        let report = end().unwrap();
        assert_eq!(report.observed.successful_submissions, 1);
        assert_eq!(report.segments[0].drawing.unattributed_submissions, 1);
        assert_eq!(report.segments[0].drawing.call_duration.samples, 0);
        assert!(!report.reached_deadline);
        assert!(!active());
        *STATE.lock().unwrap() = State::new();
    }
}
