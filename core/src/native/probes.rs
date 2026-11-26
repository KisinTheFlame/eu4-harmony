//! Low-overhead instrumentation probes called by runtime hook sites.
//!
//! In a player build (`not(feature = "dev")`), every entry point compiles to an
//! inlined no-op. In a developer build (`feature = "dev"`), top-level `dev`
//! installs its concrete handlers once at bootstrap so lower nodes never depend
//! on `dev`.
use super::{hooks::Call, memory::Memory};

/// A token carried across a frame's `Present` call for performance timing.
#[cfg(feature = "dev")]
pub(crate) struct Submission(pub(crate) Option<(u64, std::time::Instant, Option<u64>)>);

#[cfg(not(feature = "dev"))]
pub(crate) struct Submission;

#[cfg(feature = "dev")]
pub(crate) struct ProbeHandlers {
    pub(crate) day_begins: fn(&Call),
    pub(crate) day_ends: fn(&Call),
    pub(crate) drawing_day_begins: fn(&Memory),
    pub(crate) frame_check_before: fn(&mut Call),
    pub(crate) frame_check_after: fn(&mut Call),
    pub(crate) frame_prepared: fn(),
    pub(crate) frame_invalidated: fn(),
    pub(crate) drawing_frame_begins: fn(&Memory) -> bool,
    pub(crate) drawing_frame_ends: fn(bool),
    pub(crate) submission_begins: fn() -> Submission,
    pub(crate) submission_ends: fn(Submission, bool, bool),
    pub(crate) device_reset: fn(),
    pub(crate) save_begins: fn(&mut Call),
    pub(crate) save_ends: fn(&mut Call),
    pub(crate) quarantine_begin: fn(&'static str),
    pub(crate) quarantine_end: fn(),
    pub(crate) quarantine_after_observer: fn() -> Option<fn(&mut Call)>,
}

#[cfg(feature = "dev")]
static HANDLERS: std::sync::OnceLock<ProbeHandlers> = std::sync::OnceLock::new();

#[cfg(feature = "dev")]
pub(crate) fn install(handlers: ProbeHandlers) {
    let _ = HANDLERS.set(handlers);
}

#[cfg(feature = "dev")]
pub(crate) fn day_begins(call: &Call) {
    if let Some(h) = HANDLERS.get() {
        (h.day_begins)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn day_begins(_call: &Call) {}

#[cfg(feature = "dev")]
pub(crate) fn day_ends(call: &Call) {
    if let Some(h) = HANDLERS.get() {
        (h.day_ends)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn day_ends(_call: &Call) {}

#[cfg(feature = "dev")]
pub(crate) fn drawing_day_begins(m: &Memory) {
    if let Some(h) = HANDLERS.get() {
        (h.drawing_day_begins)(m);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_day_begins(_m: &Memory) {}

#[cfg(feature = "dev")]
pub(crate) fn frame_check_before(call: &mut Call) {
    if let Some(h) = HANDLERS.get() {
        (h.frame_check_before)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_check_before(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn frame_check_after(call: &mut Call) {
    if let Some(h) = HANDLERS.get() {
        (h.frame_check_after)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_check_after(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn frame_prepared() {
    if let Some(h) = HANDLERS.get() {
        (h.frame_prepared)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_prepared() {}

#[cfg(feature = "dev")]
pub(crate) fn frame_invalidated() {
    if let Some(h) = HANDLERS.get() {
        (h.frame_invalidated)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_invalidated() {}

#[cfg(feature = "dev")]
pub(crate) fn drawing_frame_begins(m: &Memory) -> bool {
    HANDLERS.get().is_some_and(|h| (h.drawing_frame_begins)(m))
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_frame_begins(_m: &Memory) -> bool {
    false
}

#[cfg(feature = "dev")]
pub(crate) fn drawing_frame_ends(watched: bool) {
    if let Some(h) = HANDLERS.get() {
        (h.drawing_frame_ends)(watched);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_frame_ends(_watched: bool) {}

#[cfg(feature = "dev")]
pub(crate) fn submission_begins() -> Submission {
    HANDLERS.get().map_or(Submission(None), |h| (h.submission_begins)())
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn submission_begins() -> Submission {
    Submission
}

#[cfg(feature = "dev")]
pub(crate) fn submission_ends(submission: Submission, success: bool, main_thread: bool) {
    if let Some(h) = HANDLERS.get() {
        (h.submission_ends)(submission, success, main_thread);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn submission_ends(_submission: Submission, _success: bool, _main_thread: bool) {}

#[cfg(feature = "dev")]
pub(crate) fn device_reset() {
    if let Some(h) = HANDLERS.get() {
        (h.device_reset)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn device_reset() {}

#[cfg(feature = "dev")]
pub(crate) fn save_begins(call: &mut Call) {
    if let Some(h) = HANDLERS.get() {
        (h.save_begins)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn save_begins(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn save_ends(call: &mut Call) {
    if let Some(h) = HANDLERS.get() {
        (h.save_ends)(call);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn save_ends(_call: &mut Call) {}

#[cfg(feature = "dev")]
pub(crate) fn quarantine_begin(repair: &'static str) {
    if let Some(h) = HANDLERS.get() {
        (h.quarantine_begin)(repair);
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn quarantine_begin(_repair: &'static str) {}

#[cfg(feature = "dev")]
pub(crate) fn quarantine_end() {
    if let Some(h) = HANDLERS.get() {
        (h.quarantine_end)();
    }
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn quarantine_end() {}

#[cfg(feature = "dev")]
pub(crate) fn quarantine_after_observer() -> Option<fn(&mut Call)> {
    HANDLERS.get().and_then(|h| (h.quarantine_after_observer)())
}
#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn quarantine_after_observer() -> Option<fn(&mut Call)> {
    None
}
