//! Domain-scoped instrumentation probe ports called by runtime hook sites.
//!
//! In a player build (`not(feature = "dev")`), every entry point compiles to an
//! inlined no-op. In a developer build (`feature = "dev"`), `dev` installs
//! concrete adapters for each domain once at bootstrap so lower nodes never
//! depend on `dev`.
use super::{hooks::Call, memory::Memory};

/// Opaque token carried across a frame's `Present` call for performance timing.
/// Its payload lives inside `dev::performance`, never leaked across this seam.
#[cfg(feature = "dev")]
pub(crate) struct Submission(pub(crate) bool);

#[cfg(not(feature = "dev"))]
pub(crate) struct Submission;

#[cfg(feature = "dev")]
pub(crate) struct DayProbes {
    pub(crate) day_begins: fn(&Call),
    pub(crate) day_ends: fn(&Call),
    pub(crate) frame_check_before: fn(&mut Call),
    pub(crate) frame_check_after: fn(&mut Call),
}

#[cfg(feature = "dev")]
pub(crate) struct DrawingProbes {
    pub(crate) day_begins: fn(&Memory),
    pub(crate) frame_begins: fn(&Memory) -> bool,
    pub(crate) frame_ends: fn(bool),
}

#[cfg(feature = "dev")]
pub(crate) struct PerformanceProbes {
    pub(crate) frame_prepared: fn(),
    pub(crate) frame_invalidated: fn(),
    pub(crate) submission_begins: fn() -> Submission,
    pub(crate) submission_ends: fn(Submission, bool, bool),
    pub(crate) device_reset: fn(),
    pub(crate) save_begins: fn(&mut Call),
    pub(crate) save_ends: fn(&mut Call),
}

#[cfg(feature = "dev")]
pub(crate) struct QuarantineProbes {
    pub(crate) begin: fn(&'static str),
    pub(crate) end: fn(),
}

#[cfg(feature = "dev")]
static DAY: std::sync::OnceLock<DayProbes> = std::sync::OnceLock::new();
#[cfg(feature = "dev")]
static DRAWING: std::sync::OnceLock<DrawingProbes> = std::sync::OnceLock::new();
#[cfg(feature = "dev")]
static PERFORMANCE: std::sync::OnceLock<PerformanceProbes> = std::sync::OnceLock::new();
#[cfg(feature = "dev")]
static QUARANTINE: std::sync::OnceLock<QuarantineProbes> = std::sync::OnceLock::new();

#[cfg(feature = "dev")]
pub(crate) fn install_day(probes: DayProbes) {
    let _ = DAY.set(probes);
}

#[cfg(feature = "dev")]
pub(crate) fn install_drawing(probes: DrawingProbes) {
    let _ = DRAWING.set(probes);
}

#[cfg(feature = "dev")]
pub(crate) fn install_performance(probes: PerformanceProbes) {
    let _ = PERFORMANCE.set(probes);
}

#[cfg(feature = "dev")]
pub(crate) fn install_quarantine(probes: QuarantineProbes) {
    let _ = QUARANTINE.set(probes);
}

#[cfg(feature = "dev")]
pub(crate) fn day_begins(call: &Call) {
    if let Some(h) = DAY.get() {
        (h.day_begins)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn day_begins(_call: &Call) {}

#[cfg(feature = "dev")]
pub(crate) fn day_ends(call: &Call) {
    if let Some(h) = DAY.get() {
        (h.day_ends)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn day_ends(_call: &Call) {}

#[cfg(feature = "dev")]
pub(crate) fn drawing_day_begins(m: &Memory) {
    if let Some(h) = DRAWING.get() {
        (h.day_begins)(m);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_day_begins(_m: &Memory) {}

#[cfg(feature = "dev")]
pub(crate) fn frame_check_before(call: &mut Call) {
    if let Some(h) = DAY.get() {
        (h.frame_check_before)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_check_before(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn frame_check_after(call: &mut Call) {
    if let Some(h) = DAY.get() {
        (h.frame_check_after)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_check_after(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn frame_prepared() {
    if let Some(h) = PERFORMANCE.get() {
        (h.frame_prepared)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_prepared() {}

#[cfg(feature = "dev")]
pub(crate) fn frame_invalidated() {
    if let Some(h) = PERFORMANCE.get() {
        (h.frame_invalidated)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_invalidated() {}

#[cfg(feature = "dev")]
pub(crate) fn drawing_frame_begins(m: &Memory) -> bool {
    DRAWING.get().is_some_and(|h| (h.frame_begins)(m))
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_frame_begins(_m: &Memory) -> bool {
    false
}

#[cfg(feature = "dev")]
pub(crate) fn drawing_frame_ends(watched: bool) {
    if let Some(h) = DRAWING.get() {
        (h.frame_ends)(watched);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_frame_ends(_watched: bool) {}

#[cfg(feature = "dev")]
pub(crate) fn submission_begins() -> Submission {
    PERFORMANCE.get().map_or(Submission(false), |h| (h.submission_begins)())
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn submission_begins() -> Submission {
    Submission
}

#[cfg(feature = "dev")]
pub(crate) fn submission_ends(submission: Submission, success: bool, main_thread: bool) {
    if let Some(h) = PERFORMANCE.get() {
        (h.submission_ends)(submission, success, main_thread);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn submission_ends(_submission: Submission, _success: bool, _main_thread: bool) {}

#[cfg(feature = "dev")]
pub(crate) fn device_reset() {
    if let Some(h) = PERFORMANCE.get() {
        (h.device_reset)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn device_reset() {}

#[cfg(feature = "dev")]
pub(crate) fn save_begins(call: &mut Call) {
    if let Some(h) = PERFORMANCE.get() {
        (h.save_begins)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn save_begins(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn save_ends(call: &mut Call) {
    if let Some(h) = PERFORMANCE.get() {
        (h.save_ends)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn save_ends(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn quarantine_begin(repair: &'static str) {
    if let Some(h) = QUARANTINE.get() {
        (h.begin)(repair);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn quarantine_begin(_repair: &'static str) {}

#[cfg(feature = "dev")]
pub(crate) fn quarantine_end() {
    if let Some(h) = QUARANTINE.get() {
        (h.end)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn quarantine_end() {}
