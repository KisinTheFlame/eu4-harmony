//! Pure replay of a bounded performance timeline. Time is in microseconds;
//! conversion to the report's milliseconds happens only at the boundary.
use super::{Context, Kind, Window};
use eu4_harmony_remote::{
    performance::{Age, Days, Distribution, Drawing, Interface, Interruption, Report, Segment},
    Date,
};
use std::collections::BTreeMap;

fn ms(us: u64) -> f64 {
    us as f64 / 1000.0
}
fn rate(count: u64, elapsed: u64, unit: u64) -> f64 {
    if elapsed == 0 {
        0.0
    } else {
        count as f64 * unit as f64 / elapsed as f64
    }
}

fn distribution(mut values: Vec<u64>) -> Distribution {
    if values.is_empty() {
        return Distribution::default();
    }
    values.sort_unstable();
    let percentile = |percent: usize| ms(values[(values.len() * percent).div_ceil(100) - 1]);
    Distribution {
        samples: values.len() as u64,
        mean_ms: Some(values.iter().map(|&v| v as f64).sum::<f64>() / values.len() as f64 / 1000.0),
        p50_ms: Some(percentile(50)),
        p95_ms: Some(percentile(95)),
        p99_ms: Some(percentile(99)),
        max_ms: values.last().copied().map(ms),
    }
}

/// Each range is a continuously increasing age over the same amount of wall
/// time. Quantiles integrate these ranges; frames do not supply the weights.
fn age(ranges: &[(u64, u64)], elapsed: u64) -> Age {
    let known: u64 = ranges.iter().map(|&(a, b)| b - a).sum();
    let max = ranges.iter().map(|&(_, b)| b).max();
    let percentile = |percent: u64| {
        if known == 0 {
            return None;
        }
        let wanted = (known * percent).div_ceil(100);
        let mut lo = 0;
        let mut hi = max.unwrap_or(0);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let below: u64 = ranges.iter().map(|&(a, b)| mid.min(b).saturating_sub(a)).sum();
            if below >= wanted {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        Some(ms(lo))
    };
    Age {
        known_ms: ms(known),
        unknown_ms: ms(elapsed.saturating_sub(known)),
        mean_ms: (known != 0).then(|| ranges.iter().map(|&(a, b)| (a as f64 + b as f64) * 0.5 * (b - a) as f64).sum::<f64>() / known as f64 / 1000.0),
        p50_ms: percentile(50),
        p95_ms: percentile(95),
        p99_ms: percentile(99),
        max_ms: max.map(ms),
    }
}

/// Lifetimes belong to the collection, not to a speed/pause segment. The
/// preparing segment owns the outcome even when a later segment submits it.
#[derive(Default)]
struct Updates {
    entries: BTreeMap<u64, Update>,
    prepared: Option<u64>,
}

struct Update {
    session: u64,
    ready: u64,
    submitted: Option<u64>,
    invalidated: bool,
    superseded: bool,
}

struct Account {
    start: u64,
    context: Context,
    day: Option<(u64, i32)>,
    completed: u64,
    faulted: u64,
    first_date: Option<Date>,
    last_date: Option<Date>,
    last_day: Option<u64>,
    day_intervals: Vec<u64>,
    durations: Vec<u64>,
    ordinary: Vec<u64>,
    months: Vec<u64>,
    years: Vec<u64>,
    submissions: u64,
    main_submissions: u64,
    failed: u64,
    first_submission: Option<u64>,
    last_submission: Option<u64>,
    submission_intervals: Vec<u64>,
    call_durations: Vec<u64>,
    unattributed: u64,
    updates: Vec<u64>,
    inside_days: u64,
    last_ready: Option<u64>,
    ready_intervals: Vec<u64>,
    submitted_ready: Option<u64>,
    age_at: u64,
    age_ranges: Vec<(u64, u64)>,
    interruptions: Vec<Interruption>,
    malformed: bool,
}

impl Account {
    fn new(start: u64, context: Context) -> Self {
        Self {
            start,
            context,
            day: None,
            completed: 0,
            faulted: 0,
            first_date: None,
            last_date: None,
            last_day: None,
            day_intervals: Vec::new(),
            durations: Vec::new(),
            ordinary: Vec::new(),
            months: Vec::new(),
            years: Vec::new(),
            submissions: 0,
            main_submissions: 0,
            failed: 0,
            first_submission: None,
            last_submission: None,
            submission_intervals: Vec::new(),
            call_durations: Vec::new(),
            unattributed: 0,
            updates: Vec::new(),
            inside_days: 0,
            last_ready: None,
            ready_intervals: Vec::new(),
            submitted_ready: None,
            age_at: start,
            age_ranges: Vec::new(),
            interruptions: Vec::new(),
            malformed: false,
        }
    }

    fn marker(&mut self, at: u64, kind: &str) {
        self.interruptions.push(Interruption { at_ms: ms(at), kind: kind.into() });
    }

    fn advance_age(&mut self, at: u64) {
        if at > self.age_at {
            if let Some(ready) = self.submitted_ready {
                self.age_ranges.push((self.age_at.saturating_sub(ready), at.saturating_sub(ready)));
            }
            self.age_at = at;
        }
    }

    fn apply(&mut self, at: u64, kind: Kind, updates: &mut Updates) {
        match kind {
            Kind::Context(_) => unreachable!("context changes start a new account"),
            Kind::DayBegins { date } => {
                if self.day.replace((at, date)).is_some() {
                    self.malformed = true;
                    self.marker(at, "overlapping_day_entries");
                }
            }
            Kind::DayEnds { date, faulted } => {
                let began = self.day.take();
                if faulted {
                    self.faulted += 1;
                    self.marker(at, "day_faulted");
                    return;
                }
                self.completed += 1;
                self.first_date.get_or_insert(Date(date));
                self.last_date = Some(Date(date));
                if let Some(last) = self.last_day.replace(at) {
                    self.day_intervals.push(at - last);
                }
                if let Some((began, _)) = began {
                    let duration = at - began;
                    self.durations.push(duration);
                    let (_, month, day) = Date(date).ymd();
                    if day == 1 && month == 1 {
                        self.years.push(duration);
                    } else if day == 1 {
                        self.months.push(duration);
                    } else {
                        self.ordinary.push(duration);
                    }
                }
            }
            Kind::Prepared { id } => {
                if let Some(last) = self.last_ready.replace(at) {
                    self.ready_intervals.push(at - last);
                }
                if self.day.is_some() {
                    self.inside_days += 1;
                }
                if let Some(prior) = updates.prepared.replace(id).and_then(|p| updates.entries.get_mut(&p)) {
                    prior.superseded = true;
                }
                self.updates.push(id);
                let update = Update { session: self.context.session, ready: at, submitted: None, invalidated: false, superseded: false };
                if updates.entries.insert(id, update).is_some() {
                    self.malformed = true;
                }
            }
            Kind::Invalidated { id } => {
                if let Some(update) = updates.entries.get_mut(&id).filter(|u| u.session == self.context.session) {
                    update.invalidated = true;
                }
                if updates.prepared == Some(id) {
                    updates.prepared = None;
                }
            }
            Kind::Submitted { began, update, success, main } => {
                if let Some(began) = began.filter(|&began| began >= self.start && began <= at) {
                    self.call_durations.push(at - began);
                }
                if !success {
                    self.failed += 1;
                    self.marker(at, "submission_failed");
                    return;
                }
                self.advance_age(at);
                self.submissions += 1;
                self.main_submissions += u64::from(main);
                self.first_submission.get_or_insert(at);
                if let Some(last) = self.last_submission.replace(at) {
                    self.submission_intervals.push(at - last);
                }
                self.submitted_ready = update.and_then(|id| updates.entries.get_mut(&id)).filter(|u| u.session == self.context.session).map(|update| {
                    update.submitted.get_or_insert(at);
                    update.ready
                });
                if self.submitted_ready.is_none() {
                    self.unattributed += 1;
                }
            }
            Kind::Interruption(kind) => self.marker(at, kind),
        }
    }

    fn finish(mut self, end: u64, updates: &Updates, collection_end: u64) -> (Segment, bool) {
        self.advance_age(end);
        let elapsed = end - self.start;
        let unfinished_gap = self.last_submission.map(|last| end - last);
        let twice = self.context.target_us.map(|target| target * 2);
        let over_target = twice.map(|threshold| self.submission_intervals.iter().filter(|&&v| v > threshold).count() as u64);
        let long_fraction = twice.map(|threshold| {
            let closed: u64 = self.submission_intervals.iter().copied().filter(|&v| v > threshold).sum();
            let tail = unfinished_gap.filter(|&v| v > threshold).unwrap_or(0);
            rate(closed + tail, elapsed, 1)
        });
        let over_50 = self.submission_intervals.iter().filter(|&&v| v > 50_000).count() as u64;
        let over_100 = self.submission_intervals.iter().filter(|&&v| v > 100_000).count() as u64;
        let mut submitted = 0;
        let mut superseded = 0;
        let mut invalidated = 0;
        let mut pending = 0;
        let mut pending_oldest = None;
        let mut delays = Vec::new();
        for update in self.updates.iter().filter_map(|id| updates.entries.get(id)) {
            if let Some(at) = update.submitted {
                submitted += 1;
                delays.push(at - update.ready);
            } else if update.invalidated {
                invalidated += 1;
            } else if update.superseded {
                superseded += 1;
            } else {
                pending += 1;
                let age = collection_end - update.ready;
                pending_oldest = Some(pending_oldest.map_or(age, |old: u64| old.max(age)));
            }
        }
        let segment = Segment {
            start_ms: ms(self.start),
            elapsed_ms: ms(elapsed),
            session: self.context.session,
            speed: self.context.speed,
            paused: self.context.paused,
            target_interval_ms: self.context.target_us.map(ms),
            days: Days {
                completed: self.completed,
                faulted: self.faulted,
                completed_per_minute: rate(self.completed, elapsed, 60_000_000),
                first_completed_date: self.first_date,
                last_completed_date: self.last_date,
                duration: distribution(self.durations),
                ordinary_duration: distribution(self.ordinary),
                month_start_duration: distribution(self.months),
                year_start_duration: distribution(self.years),
                completion_interval: distribution(self.day_intervals),
                unfinished_completion_gap_ms: self.last_day.map(|last| ms(end - last)),
                unfinished_day_ms: self.day.map(|(began, _)| ms(end - began)),
            },
            drawing: Drawing {
                successful_submissions: self.submissions,
                main_thread_submissions: self.main_submissions,
                failed_submissions: self.failed,
                submissions_per_second: rate(self.submissions, elapsed, 1_000_000),
                interval: distribution(self.submission_intervals),
                call_duration: distribution(self.call_durations),
                over_twice_target: over_target,
                over_twice_target_per_minute: over_target.map(|count| rate(count, elapsed, 60_000_000)),
                long_gap_time_fraction: long_fraction,
                over_50_ms: over_50,
                over_50_ms_per_minute: rate(over_50, elapsed, 60_000_000),
                over_100_ms: over_100,
                over_100_ms_per_minute: rate(over_100, elapsed, 60_000_000),
                unfinished_gap_ms: unfinished_gap.map(ms),
                initial_submission_wait_ms: ms(self.first_submission.unwrap_or(end) - self.start),
                unattributed_submissions: self.unattributed,
            },
            interface: Interface {
                updates: self.updates.len() as u64,
                updates_inside_days: self.inside_days,
                updates_per_second: rate(self.updates.len() as u64, elapsed, 1_000_000),
                ready_interval: distribution(self.ready_intervals),
                first_submission_delay: distribution(delays),
                submitted_updates: submitted,
                superseded_updates: superseded,
                invalidated_updates: invalidated,
                pending_updates: pending,
                unfinished_ready_gap_ms: self.last_ready.map(|last| ms(end - last)),
                pending_oldest_ms: pending_oldest.map(ms),
                age: age(&self.age_ranges, elapsed),
            },
            interruptions: self.interruptions,
        };
        (segment, self.malformed)
    }
}

pub(super) fn report(mut window: Window, elapsed: u64) -> Report {
    let retained_end = window.truncated_at.unwrap_or(elapsed).min(elapsed);
    // Different threads capture their timestamps before taking the short
    // recorder lock. Stable sorting restores observation order afterwards.
    window.events.sort_by_key(|event| event.at);
    let recorded_events = window.events.len() as u64;
    let mut account = Account::new(0, window.initial);
    let mut accounts = Vec::new();
    let mut updates = Updates::default();
    for event in window.events.into_iter().filter(|event| event.at < retained_end) {
        if let Kind::Context(context) = event.kind {
            account.advance_age(event.at);
            let mut next = Account::new(event.at, context);
            if context.session == account.context.session {
                next.submitted_ready = account.submitted_ready;
            } else if let Some(id) = updates.prepared.take() {
                if let Some(update) = updates.entries.get_mut(&id) {
                    update.invalidated = true;
                }
            }
            accounts.push((account, event.at));
            account = next;
        } else {
            account.apply(event.at, event.kind, &mut updates);
        }
    }
    accounts.push((account, retained_end));
    // Resolve each preparation's final outcome only after the whole retained
    // timeline, including submissions that return in a later same-session
    // segment. The preparing segment counts it once.
    let mut segments = Vec::new();
    let mut malformed = false;
    for (account, end) in accounts {
        let (segment, bad) = account.finish(end, &updates, retained_end);
        if segment.elapsed_ms > 0.0 {
            segments.push(segment);
        }
        malformed |= bad;
    }
    let mut warnings = Vec::new();
    if window.dropped != 0 {
        warnings.push("The bounded timeline overflowed; segment statistics cover recorded_ms only. Whole-window observed counts include the dropped events, but lost intervals and their tail are unmeasured.".into());
    }
    if malformed {
        warnings.push("Overlapping day entries or duplicate update identifiers made the timeline incomplete.".into());
    }
    if segments.iter().any(|s| s.days.faulted != 0) {
        warnings.push("At least one daily update faulted.".into());
    }
    let in_game: Vec<_> = segments.iter().filter(|s| s.session != 0).collect();
    if in_game.iter().map(|s| s.drawing.successful_submissions).sum::<u64>() == 0 {
        warnings.push("No successful in-game submission was observed.".into());
    }
    if in_game.iter().map(|s| s.interface.updates).sum::<u64>() == 0 {
        warnings.push("No in-game interface preparation was observed.".into());
    }
    if in_game.iter().map(|s| s.interface.submitted_updates).sum::<u64>() == 0 {
        warnings.push("No successful submission could be attributed to an interface update.".into());
    }
    if segments.iter().flat_map(|s| &s.interruptions).any(|i| matches!(i.kind.as_str(), "game context unavailable" | "day date unavailable")) {
        warnings.push("Required game state was unavailable at a measurement boundary.".into());
    }
    if in_game.iter().any(|s| !s.paused) && in_game.iter().map(|s| s.days.completed).sum::<u64>() == 0 {
        warnings.push("No completed day was observed while time was running.".into());
    }
    Report {
        schema_version: 1,
        requested_seconds: window.seconds,
        elapsed_ms: ms(elapsed),
        reached_deadline: elapsed == u64::from(window.seconds) * 1_000_000,
        complete: warnings.is_empty(),
        recorded_events,
        dropped_events: window.dropped,
        recorded_ms: ms(retained_end),
        observed: window.observed,
        warnings,
        segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn context() -> Context {
        Context { session: 1, speed: 5, paused: false, target_us: Some(10_000) }
    }
    fn window(events: &[(u64, Kind)]) -> Window {
        let mut w = Window::new(1, Instant::now(), 1, context(), 100);
        for &(at, kind) in events {
            w.push(at, kind);
        }
        w
    }
    fn submit(at: u64, update: Option<u64>) -> Kind {
        Kind::Submitted { began: Some(at.saturating_sub(1_000)), update, success: true, main: false }
    }
    fn date(month: u32, day: u32) -> i32 {
        Date::from_ymd(1700, month, day).unwrap().0
    }

    #[test]
    fn submissions_without_any_update_attribution_are_incomplete() {
        let r = report(
            window(&[
                (0, Kind::DayBegins { date: date(1, 1) }),
                (1_000, Kind::Prepared { id: 1 }),
                (2_000, submit(2_000, None)),
                (3_000, Kind::DayEnds { date: date(1, 2), faulted: false }),
            ]),
            10_000,
        );
        assert!(!r.complete);
        assert_eq!(r.segments[0].drawing.successful_submissions, 1);
        assert_eq!(r.segments[0].interface.first_submission_delay.samples, 0);
        assert_eq!(r.segments[0].interface.age.known_ms, 0.0);
    }

    #[test]
    fn nested_updates_do_not_split_a_day_and_calendar_classes_are_exclusive() {
        let r = report(
            window(&[
                (0, Kind::DayBegins { date: date(1, 30) }),
                (1_000, Kind::Prepared { id: 1 }),
                (2_000, submit(2_000, Some(1))),
                (10_000, Kind::DayEnds { date: date(1, 31), faulted: false }),
                (20_000, Kind::DayBegins { date: date(1, 31) }),
                (50_000, Kind::DayEnds { date: date(2, 1), faulted: false }),
                (60_000, Kind::DayBegins { date: date(12, 31) }),
                (100_000, Kind::DayEnds { date: Date::from_ymd(1701, 1, 1).unwrap().0, faulted: false }),
            ]),
            110_000,
        );
        let s = &r.segments[0];
        assert_eq!(s.days.completed, 3);
        assert_eq!(s.days.duration.samples, 3);
        assert_eq!(s.days.ordinary_duration.max_ms, Some(10.0));
        assert_eq!(s.days.month_start_duration.max_ms, Some(30.0));
        assert_eq!(s.days.year_start_duration.max_ms, Some(40.0));
        assert_eq!(s.days.completion_interval.max_ms, Some(50.0));
        assert_eq!(s.interface.updates_inside_days, 1);
        assert!(r.complete);
    }

    #[test]
    fn repeated_failed_and_in_flight_submissions_keep_update_outcomes_honest() {
        let r = report(
            window(&[
                (0, Kind::Prepared { id: 1 }),
                (2_000, Kind::Prepared { id: 2 }),
                // Update 1 was superseded while its frame was in flight, but its
                // successful submission still counts as an actual shown update.
                (3_000, submit(3_000, Some(1))),
                (4_000, Kind::Submitted { began: Some(3_000), update: Some(2), success: false, main: false }),
                (5_000, Kind::Invalidated { id: 2 }),
                (6_000, Kind::Prepared { id: 3 }),
                (7_000, Kind::Prepared { id: 4 }),
                (8_000, Kind::Submitted { began: Some(7_000), update: Some(4), success: true, main: true }),
                (9_000, submit(9_000, Some(4))),
                (10_000, Kind::Prepared { id: 5 }),
            ]),
            20_000,
        );
        let s = &r.segments[0];
        assert_eq!(s.interface.submitted_updates, 2);
        assert_eq!(s.interface.invalidated_updates, 1);
        assert_eq!(s.interface.superseded_updates, 1);
        assert_eq!(s.interface.pending_updates, 1);
        assert_eq!(s.interface.first_submission_delay.samples, 2);
        assert_eq!(s.drawing.successful_submissions, 3);
        assert_eq!(s.drawing.failed_submissions, 1);
        assert_eq!(s.drawing.main_thread_submissions, 1);
        assert_eq!(s.interface.pending_oldest_ms, Some(10.0));
    }

    #[test]
    fn invalidation_during_an_in_flight_submission_keeps_its_actual_first_submission() {
        let r = report(
            window(&[
                (0, Kind::Prepared { id: 1 }),
                (2_000, Kind::Invalidated { id: 1 }),
                (3_000, submit(3_000, Some(1))),
                (4_000, Kind::Submitted { began: Some(3_000), update: None, success: false, main: false }),
            ]),
            10_000,
        );
        let s = &r.segments[0];
        assert_eq!(s.interface.submitted_updates, 1);
        assert_eq!(s.interface.invalidated_updates, 0);
        assert_eq!(s.interface.first_submission_delay.max_ms, Some(3.0));
        assert_eq!(s.interface.age.known_ms, 7.0);
        assert_eq!(s.interface.age.max_ms, Some(10.0));
    }

    #[test]
    fn a_freeze_through_the_window_end_contributes_age_and_an_unfinished_gap() {
        let r = report(window(&[(0, Kind::Prepared { id: 1 }), (10_000, submit(10_000, Some(1))), (20_000, Kind::DayBegins { date: date(1, 2) })]), 1_000_000);
        let s = &r.segments[0];
        assert_eq!(s.drawing.interval.samples, 0);
        assert_eq!(s.drawing.unfinished_gap_ms, Some(990.0));
        assert_eq!(s.drawing.long_gap_time_fraction, Some(0.99));
        assert_eq!(s.days.unfinished_day_ms, Some(980.0));
        assert_eq!(s.interface.age.known_ms, 990.0);
        assert_eq!(s.interface.age.unknown_ms, 10.0);
        assert_eq!(s.interface.age.p50_ms, Some(505.0));
        assert_eq!(s.interface.age.p99_ms, Some(990.1));
        assert!(r.reached_deadline);
    }

    #[test]
    fn context_changes_split_intervals_and_prevent_cross_session_attribution() {
        let mut next = context();
        next.session = 2;
        next.paused = true;
        next.target_us = None;
        let r = report(
            window(&[
                (0, Kind::Prepared { id: 1 }),
                (2_000, submit(2_000, Some(1))),
                (10_000, Kind::Context(next)),
                (12_000, submit(12_000, Some(1))),
                (13_000, Kind::Prepared { id: 2 }),
                (14_000, submit(14_000, Some(2))),
            ]),
            20_000,
        );
        assert_eq!(r.segments.len(), 2);
        assert_eq!(r.segments[0].drawing.interval.samples, 0);
        assert_eq!(r.segments[1].drawing.interval.samples, 1);
        assert_eq!(r.segments[1].drawing.unattributed_submissions, 1);
        assert_eq!(r.segments[1].interface.age.unknown_ms, 4.0);
        assert_eq!(r.segments[1].drawing.over_twice_target, None);
    }

    #[test]
    fn pause_speed_and_target_segments_preserve_known_age_without_new_submissions() {
        let mut paused = context();
        paused.paused = true;
        let mut slower = paused;
        slower.speed = 4;
        let mut unlimited = slower;
        unlimited.target_us = None;
        let r = report(
            window(&[
                (0, Kind::Prepared { id: 1 }),
                (2_000, submit(2_000, Some(1))),
                (10_000, Kind::Context(paused)),
                (20_000, Kind::Context(slower)),
                (30_000, Kind::Context(unlimited)),
            ]),
            100_000,
        );
        assert_eq!(r.segments.len(), 4);
        let age = &r.segments[3].interface.age;
        assert_eq!(age.known_ms, 70.0);
        assert_eq!(age.unknown_ms, 0.0);
        assert_eq!(age.mean_ms, Some(65.0));
        assert_eq!(age.max_ms, Some(100.0));
        assert_eq!(r.segments.iter().map(|s| s.interface.age.known_ms).sum::<f64>(), 98.0);
        assert_eq!(r.segments.iter().map(|s| s.interface.submitted_updates).sum::<u64>(), 1);
        assert!(r.segments[1..].iter().all(|s| s.interface.updates == 0 && s.interface.submitted_updates == 0));
    }

    #[test]
    fn update_outcomes_resolve_across_segments_once_in_the_preparing_segment() {
        let mut paused = context();
        paused.paused = true;
        let mut slower = paused;
        slower.speed = 4;
        let mut unlimited = slower;
        unlimited.target_us = None;
        let r = report(
            window(&[
                (0, Kind::Prepared { id: 1 }),
                (1_000, Kind::Prepared { id: 2 }),
                (2_000, Kind::Context(paused)),
                (3_000, Kind::Invalidated { id: 2 }),
                (4_000, submit(4_000, Some(1))),
                (5_000, submit(5_000, Some(2))),
                (6_000, Kind::Prepared { id: 3 }),
                (7_000, Kind::Context(slower)),
                (8_000, Kind::Prepared { id: 4 }),
                (9_000, Kind::Invalidated { id: 4 }),
                (10_000, Kind::Prepared { id: 5 }),
                (11_000, Kind::Context(unlimited)),
                (13_000, Kind::Prepared { id: 6 }),
                (14_000, Kind::Context(paused)),
            ]),
            20_000,
        );
        let first = &r.segments[0].interface;
        assert_eq!(first.updates, 2);
        assert_eq!(first.submitted_updates, 2);
        assert_eq!(first.pending_updates, 0);
        assert_eq!(first.first_submission_delay.samples, 2);
        assert_eq!(first.first_submission_delay.max_ms, Some(4.0));
        assert_eq!(r.segments[1].interface.superseded_updates, 1);
        assert_eq!(r.segments[2].interface.invalidated_updates, 1);
        assert_eq!(r.segments[2].interface.superseded_updates, 1);
        assert_eq!(r.segments[3].interface.pending_updates, 1);
        assert_eq!(r.segments[3].interface.pending_oldest_ms, Some(7.0));
        assert_eq!(r.segments[4].interface.updates, 0);
        assert_eq!(r.segments[4].interface.pending_updates, 0);
        assert_eq!(r.segments.iter().map(|s| s.interface.updates).sum::<u64>(), 6);
        for s in &r.segments {
            let i = &s.interface;
            assert_eq!(i.updates, i.submitted_updates + i.superseded_updates + i.invalidated_updates + i.pending_updates);
            assert_eq!(s.drawing.unattributed_submissions, 0);
        }
    }

    #[test]
    fn a_new_session_invalidates_pending_updates_and_rejects_old_associations() {
        let mut next = context();
        next.session = 2;
        let r = report(
            window(&[
                (0, Kind::Prepared { id: 1 }),
                (2_000, Kind::Context(next)),
                (3_000, submit(3_000, Some(1))),
                (4_000, Kind::Prepared { id: 2 }),
                (5_000, submit(5_000, Some(2))),
            ]),
            10_000,
        );
        assert_eq!(r.segments[0].interface.invalidated_updates, 1);
        assert_eq!(r.segments[0].interface.pending_updates, 0);
        assert_eq!(r.segments[0].interface.submitted_updates, 0);
        let next = &r.segments[1];
        assert_eq!(next.drawing.unattributed_submissions, 1);
        assert_eq!(next.interface.submitted_updates, 1);
        assert_eq!(next.interface.age.unknown_ms, 3.0);
        assert_eq!(next.interface.age.known_ms, 5.0);
        assert_eq!(next.interface.age.max_ms, Some(6.0));
    }

    #[test]
    fn overflow_preserves_whole_window_counts_and_does_not_invent_the_missing_tail() {
        let mut w = Window::new(1, Instant::now(), 1, context(), 2);
        w.push(0, Kind::Prepared { id: 1 });
        w.push(10_000, submit(10_000, Some(1)));
        w.push(20_000, Kind::Prepared { id: 2 });
        w.push(30_000, submit(30_000, Some(2)));
        let r = report(w, 100_000);
        assert!(!r.complete);
        assert_eq!(r.dropped_events, 2);
        assert_eq!(r.observed.prepared_updates, 2);
        assert_eq!(r.observed.successful_submissions, 2);
        assert_eq!(r.recorded_ms, 20.0);
        assert_eq!(r.elapsed_ms, 100.0);
        assert_eq!(r.segments[0].drawing.unfinished_gap_ms, Some(10.0));
    }

    #[test]
    fn nearest_rank_and_time_weighted_percentiles_have_different_weights() {
        let d = distribution(vec![1_000, 2_000, 30_000, 40_000]);
        assert_eq!(d.p50_ms, Some(2.0));
        assert_eq!(d.p95_ms, Some(40.0));
        let a = age(&[(0, 1_000), (0, 1_000), (0, 98_000)], 100_000);
        assert_eq!(a.p50_ms, Some(48.0));
        assert_eq!(a.p99_ms, Some(97.0));
        assert_eq!(a.unknown_ms, 0.0);
    }

    #[test]
    fn missing_observations_are_not_zero_latency_and_failed_days_do_not_complete() {
        let r = report(window(&[(0, Kind::DayBegins { date: date(1, 1) }), (10_000, Kind::DayEnds { date: date(1, 2), faulted: true })]), 20_000);
        assert!(!r.complete);
        assert_eq!(r.segments[0].days.completed, 0);
        assert_eq!(r.segments[0].days.faulted, 1);
        assert_eq!(r.segments[0].drawing.interval.p50_ms, None);
        assert_eq!(r.segments[0].interface.age.p50_ms, None);
        assert_eq!(r.segments[0].interface.age.unknown_ms, 20.0);
    }

    #[test]
    fn timestamp_order_and_threshold_boundaries_are_explicit() {
        let r = report(
            window(&[
                (30_000, submit(30_000, Some(1))),
                (0, Kind::Prepared { id: 1 }),
                (10_000, submit(10_000, Some(1))),
                (80_001, submit(80_001, Some(1))),
                (180_002, submit(180_002, Some(1))),
            ]),
            190_000,
        );
        let d = &r.segments[0].drawing;
        assert_eq!(d.over_twice_target, Some(2)); // exactly 2T is not over
        assert_eq!(d.over_50_ms, 2);
        assert_eq!(d.over_100_ms, 1);
        assert_eq!(d.interval.samples, 3);
    }
}
