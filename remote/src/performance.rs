//! Bounded, on-demand performance measurements in a developer build.
//! Submission timing ends at the observed Present wrapper's CPU return,
//! before presentation-lock reacquisition, not at display.
use serde::{Deserialize, Serialize};

group! {
    /// Performance measurements from the game's own monotonic clock.
    "performance": PerformanceRequest, PerformanceAnswer;

    /// Begins one bounded window. A previous window must be collected first.
    remote_world Begin "begin" {
        /// Window length in seconds, from 1 to 600.
        seconds: u32,
    } -> (), "nothing; collection stops at its in-process deadline, including when the client collects it later";
    /// Ends and collects the current window, or collects one whose deadline
    /// already passed. Ending early reports the shorter elapsed window.
    remote_anywhere End "end" {} -> Report, "schema_version; requested_seconds; elapsed_ms; reached_deadline; complete and warnings; recorded_events and dropped_events; segments by session, speed, pause state and target interval, with days, drawing and interface measurements";
}

/// One bounded collection. No field describes monitor display latency.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Report {
    pub schema_version: u32,
    pub requested_seconds: u32,
    pub elapsed_ms: f64,
    pub reached_deadline: bool,
    /// False when records were lost or required observations were missing.
    pub complete: bool,
    pub recorded_events: u64,
    pub dropped_events: u64,
    /// Retained timeline duration; shorter than elapsed_ms after overflow.
    pub recorded_ms: f64,
    /// Counts remain available even if the bounded timeline overflows.
    pub observed: Observed,
    pub warnings: Vec<String>,
    pub segments: Vec<Segment>,
}

/// Whole-window event counts, independent of timeline storage capacity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Observed {
    pub completed_days: u64,
    pub successful_submissions: u64,
    pub failed_submissions: u64,
    pub prepared_updates: u64,
}

/// A stretch with one session, speed, pause state and drawing target.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Segment {
    /// Offset from the collection's beginning.
    pub start_ms: f64,
    pub elapsed_ms: f64,
    pub session: u64,
    pub speed: u32,
    pub paused: bool,
    /// None means no declared target (unlimited or native drawing); a target
    /// is never inferred from measured drawing.
    pub target_interval_ms: Option<f64>,
    pub days: Days,
    pub drawing: Drawing,
    pub interface: Interface,
    /// Explicit markers, including submission failures and device resets.
    pub interruptions: Vec<Interruption>,
}

/// Exact nearest-rank quantiles over the recorded event intervals.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Distribution {
    pub samples: u64,
    pub mean_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Days {
    pub completed: u64,
    pub faulted: u64,
    pub completed_per_minute: f64,
    pub first_completed_date: Option<crate::Date>,
    pub last_completed_date: Option<crate::Date>,
    /// Full call elapsed time, including nested interface updates and waits.
    pub duration: Distribution,
    pub ordinary_duration: Distribution,
    pub month_start_duration: Distribution,
    pub year_start_duration: Distribution,
    pub completion_interval: Distribution,
    /// Time since the last completion, censored by the window/segment end.
    pub unfinished_completion_gap_ms: Option<f64>,
    /// Elapsed time of a day still executing at the window/segment end.
    pub unfinished_day_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Drawing {
    pub successful_submissions: u64,
    pub main_thread_submissions: u64,
    pub failed_submissions: u64,
    pub submissions_per_second: f64,
    pub interval: Distribution,
    /// CPU elapsed time around the observed submission wrapper, including
    /// its small wrapper/lock-release cost but excluding lock reacquisition.
    pub call_duration: Distribution,
    pub over_twice_target: Option<u64>,
    pub over_twice_target_per_minute: Option<f64>,
    /// Includes the known part of an unfinished gap exceeding the threshold.
    pub long_gap_time_fraction: Option<f64>,
    pub over_50_ms: u64,
    pub over_50_ms_per_minute: f64,
    pub over_100_ms: u64,
    pub over_100_ms_per_minute: f64,
    pub unfinished_gap_ms: Option<f64>,
    /// Time before the first successful submission, or the whole segment.
    pub initial_submission_wait_ms: f64,
    pub unattributed_submissions: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Interface {
    pub updates: u64,
    pub updates_inside_days: u64,
    pub updates_per_second: f64,
    pub ready_interval: Distribution,
    pub first_submission_delay: Distribution,
    /// Mutually exclusive final outcomes for every update prepared here.
    pub submitted_updates: u64,
    pub superseded_updates: u64,
    pub invalidated_updates: u64,
    pub pending_updates: u64,
    pub unfinished_ready_gap_ms: Option<f64>,
    pub pending_oldest_ms: Option<f64>,
    pub age: Age,
}

/// Wall-time-weighted age of the last attributable submitted update.
/// Age keeps increasing during a submission freeze and while paused.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Age {
    pub known_ms: f64,
    pub unknown_ms: f64,
    pub mean_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Interruption {
    /// Offset from the beginning of the collection.
    pub at_ms: f64,
    pub kind: String,
}
