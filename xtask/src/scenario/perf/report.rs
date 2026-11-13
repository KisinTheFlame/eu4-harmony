//! Human-readable companion to the complete, versioned JSON measurement.
use eu4_harmony_remote::performance::{Distribution, Report};

fn number(n: Option<f64>) -> String {
    n.map_or_else(|| "unmeasured".into(), |v| format!("{v:.3}"))
}

fn distribution(name: &str, d: &Distribution) -> String {
    format!("| {name} | {} | {} | {} | {} | {} | {} |", d.samples, number(d.mean_ms), number(d.p50_ms), number(d.p95_ms), number(d.p99_ms), number(d.max_ms))
}

pub(super) fn metrics(name: &str, report: &Report) -> Vec<String> {
    let mut md = vec![
        String::new(),
        format!("### {name}: in-process performance"),
        String::new(),
        format!(
            "Schema {}; {:.3} s measured, {:.3} s retained; deadline reached: {}; complete: {}; {} events retained, {} lost.",
            report.schema_version,
            report.elapsed_ms / 1000.0,
            report.recorded_ms / 1000.0,
            report.reached_deadline,
            report.complete,
            report.recorded_events,
            report.dropped_events
        ),
    ];
    if !report.complete {
        md.push("Incomplete measurement: these figures must not be used as a passing comparison.".into());
    }
    for warning in &report.warnings {
        md.push(format!("Warning: {warning}"));
    }
    for (i, s) in report.segments.iter().enumerate() {
        md.extend([
            String::new(),
            format!("#### Segment {}: session {}, speed {}, paused {}", i + 1, s.session, s.speed, s.paused),
            String::new(),
            format!(
                "Offset {:.3} s; duration {:.3} s; target interval {} ms (unmeasured means no declared target).",
                s.start_ms / 1000.0,
                s.elapsed_ms / 1000.0,
                number(s.target_interval_ms)
            ),
            String::new(),
            "| Figure | Value |".into(),
            "| --- | --- |".into(),
            format!("| Completed days; days per minute | {}; {:.3} |", s.days.completed, s.days.completed_per_minute),
            format!("| Faulted daily calls | {} |", s.days.faulted),
            format!("| Successful submissions; per second | {}; {:.3} |", s.drawing.successful_submissions, s.drawing.submissions_per_second),
            format!("| Main-thread submissions; failed submissions | {}; {} |", s.drawing.main_thread_submissions, s.drawing.failed_submissions),
            format!(
                "| Submission gaps over twice target; per minute | {}; {} |",
                s.drawing.over_twice_target.map_or_else(|| "unmeasured".into(), |n| n.to_string()),
                number(s.drawing.over_twice_target_per_minute)
            ),
            format!("| Fraction of time in gaps over twice target | {} |", number(s.drawing.long_gap_time_fraction)),
            format!("| Gaps over 50 ms; per minute | {}; {:.3} |", s.drawing.over_50_ms, s.drawing.over_50_ms_per_minute),
            format!("| Gaps over 100 ms; per minute | {}; {:.3} |", s.drawing.over_100_ms, s.drawing.over_100_ms_per_minute),
            format!(
                "| Ready updates; per second; inside days | {}; {:.3}; {} |",
                s.interface.updates, s.interface.updates_per_second, s.interface.updates_inside_days
            ),
            format!(
                "| Update outcomes: submitted, superseded, invalidated, pending | {}, {}, {}, {} |",
                s.interface.submitted_updates, s.interface.superseded_updates, s.interface.invalidated_updates, s.interface.pending_updates
            ),
            format!("| Submissions without an attributable update | {} |", s.drawing.unattributed_submissions),
            format!("| Initial wait for a submission, ms | {:.3} |", s.drawing.initial_submission_wait_ms),
            format!("| Unfinished submission gap, ms | {} |", number(s.drawing.unfinished_gap_ms)),
            format!("| Unfinished day; completion gap, ms | {}; {} |", number(s.days.unfinished_day_ms), number(s.days.unfinished_completion_gap_ms)),
            format!(
                "| Unfinished update gap; oldest pending update, ms | {}; {} |",
                number(s.interface.unfinished_ready_gap_ms),
                number(s.interface.pending_oldest_ms)
            ),
            format!("| Interface age: known / unknown time, ms | {:.3} / {:.3} |", s.interface.age.known_ms, s.interface.age.unknown_ms),
            String::new(),
            "| Interval, ms | Samples | Mean | P50 | P95 | P99 | Maximum |".into(),
            "| --- | --- | --- | --- | --- | --- | --- |".into(),
            distribution("Whole daily call", &s.days.duration),
            distribution("Ordinary daily call", &s.days.ordinary_duration),
            distribution("Month-start daily call", &s.days.month_start_duration),
            distribution("Year-start daily call", &s.days.year_start_duration),
            distribution("Day completion interval", &s.days.completion_interval),
            distribution("Submission interval", &s.drawing.interval),
            distribution("Present wrapper call", &s.drawing.call_duration),
            distribution("Update-ready interval", &s.interface.ready_interval),
            distribution("Update ready to first submission", &s.interface.first_submission_delay),
            format!(
                "| Last submitted update's age (time-weighted) | wall time | {} | {} | {} | {} | {} |",
                number(s.interface.age.mean_ms),
                number(s.interface.age.p50_ms),
                number(s.interface.age.p95_ms),
                number(s.interface.age.p99_ms),
                number(s.interface.age.max_ms)
            ),
        ]);
        if !s.interruptions.is_empty() {
            md.extend([String::new(), "Interruptions (offsets from the window's start):".into(), String::new()]);
            md.extend(s.interruptions.iter().map(|v| format!("- {:.3} s: {}", v.at_ms / 1000.0, v.kind)));
        }
    }
    md
}
