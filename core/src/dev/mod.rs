//! Development tools seam: the one module through which the rest of the core
//! interacts with developer-only instrumentation, test switches and remote
//! capabilities.
//!
//! In a developer build (`feature = "dev"`), the functions here fan out to the
//! tool implementations under this directory according to `dev.toml`. In a
//! player build (or a `dev-test-games`-only build), every entry point compiles
//! to an inlined no-op or constant `false`, keeping `#[cfg]` out of the
//! framework's runtime modules.

#[cfg(feature = "dev")]
mod agents_order;
#[cfg(feature = "dev")]
mod census;
#[cfg(feature = "dev")]
mod command_log;
#[cfg(feature = "dev")]
mod crossings;
#[cfg(feature = "dev")]
mod memory_account;
#[cfg(feature = "dev")]
pub(crate) use crate::base::dev_options as options;
#[cfg(feature = "dev")]
mod performance;
#[cfg(feature = "dev")]
mod performance_hooks;
#[cfg(feature = "dev")]
mod trace;
#[cfg(feature = "dev")]
mod write_watch;

use crate::{native::hooks::Call, native::memory::Memory, native::points::Point};

/// Installs runtime probe handlers into `native::probes` in a developer build.
#[cfg(feature = "dev")]
pub(crate) fn install_probes() {
    crate::native::probes::install(crate::native::probes::ProbeHandlers {
        day_begins,
        day_ends,
        drawing_day_begins,
        frame_check_before,
        frame_check_after,
        frame_prepared,
        frame_invalidated,
        drawing_frame_begins,
        drawing_frame_ends,
        submission_begins: || performance::submission_begins().into_probe(),
        submission_ends: |s, ok, main| performance::submission_ends(performance::Submission::from_probe(s), ok, main),
        device_reset,
        save_begins,
        save_ends,
        quarantine_begin,
        quarantine_end,
        quarantine_after_observer,
    });
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn install_probes() {}

/// A token carried across a frame's `Present` call for performance timing.
#[cfg(feature = "dev")]
pub(crate) struct Submission(performance::Submission);

#[cfg(not(feature = "dev"))]
pub(crate) struct Submission;

pub(crate) use crate::base::dev_options::{build_label, fail_startup, load, report_lines, test_notice};

/// All static dependency points declared by development tools, checked by the
/// offline checks in a developer build regardless of `dev.toml`.
#[cfg(feature = "dev")]
pub(crate) fn points() -> Vec<Point> {
    let mut all = memory_account::points();
    all.extend(census::points());
    all.extend(crossings::points());
    all.extend(performance_hooks::points());
    all
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn points() -> Vec<Point> {
    Vec::new()
}

/// Adjusts the build's declared points for developer-only hook replacements
/// whose bytes should also be checked offline.
#[cfg(feature = "dev")]
pub(crate) fn adjust_declared(all: &mut Vec<Point>) {
    crate::speed::saving::parallel::replace_compare_points(all);
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn adjust_declared(_all: &mut Vec<Point>) {}

/// Adds and starts the development tools active for this run according to
/// `dev.toml`.
#[cfg(feature = "dev")]
pub(crate) fn installed_points(m: Memory, all: &mut Vec<Point>) {
    let opts = options::options();
    all.extend(memory_account::points());
    if opts.heap_census.enabled {
        all.extend(census::points());
    }
    if opts.crossings.enabled {
        all.extend(crossings::points());
    }
    if opts.performance.enabled {
        all.extend(performance_hooks::points());
        if crate::native::speed_up::game_path(&crate::speed::saving::background::SPEED_UP) {
            all.extend(crate::speed::saving::background::save_points());
        }
    }
    if crate::native::speed_up::checked(&crate::speed::saving::parallel::SPEED_UP) && !crate::native::speed_up::game_path(&crate::speed::saving::parallel::SPEED_UP) {
        crate::speed::saving::parallel::replace_compare_points(all);
    }
    if agents_order::chosen() {
        all.retain(|p| !matches!(p.place, crate::native::points::Place::Rva(crate::speed::army_search::CHILDREN_WALK)));
        all.extend(agents_order::points());
    }
    let watch = write_watch::chosen(&m);
    all.extend(watch.points());
    watch.start();
    let traced = trace::chosen(&m);
    all.extend(traced.points());
    traced.start();
    if opts.crossings.enabled {
        crossings::start();
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn installed_points(_m: Memory, _all: &mut Vec<Point>) {}

/// Starts post-install development listeners once all dependency points are in
/// place.
#[cfg(feature = "dev")]
pub(crate) fn after_install() {
    if options::options().command_log.enabled {
        command_log::start();
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn after_install() {}

/// First-frame test-switch actions when a new game session begins.
#[cfg(feature = "dev")]
pub(crate) fn new_session() {
    let switches = &options::options().test_switches;
    for text in &switches.post {
        match crate::game::commands::Commands::Game.post(text) {
            Ok(()) => log!("test command carried out: {text}"),
            Err(refused) => log!("test command not posted ({refused:?}): {text}"),
        }
    }
    if switches.grow_pages {
        log!("test switch grow-pages: every font page grows");
        crate::text::unicode::fill::want_room_everywhere();
    }
    if switches.crash {
        log!("test switch crash: crashing the game");
        // An access violation, which the game's crash handling takes.
        unsafe { std::ptr::read_volatile(8 as *const u8) };
    }
    if switches.panic {
        log!("test switch panic: panicking in framework code");
        panic!("the test switch panic is on");
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn new_session() {}

/// A game day begins on the main thread (`game::signals`).
#[cfg(feature = "dev")]
pub(crate) fn day_begins(call: &Call) {
    if options::options().crossings.enabled {
        crossings::day_begins(&call.memory());
    }
    performance_hooks::day_begins(call);
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn day_begins(_call: &Call) {}

/// A game day ends on the main thread (`game::signals`).
#[cfg(feature = "dev")]
pub(crate) fn day_ends(call: &Call) {
    performance_hooks::day_ends(call);
    if options::options().crossings.enabled {
        crossings::day_ends();
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn day_ends(_call: &Call) {}

/// A game day begins while the drawing thread is active (`drawing`).
#[cfg(feature = "dev")]
pub(crate) fn drawing_day_begins(m: &Memory) {
    write_watch::drawing::day_begins(m);
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_day_begins(_m: &Memory) {}

/// A check for a frame inside a day begins (`drawing`).
#[cfg(feature = "dev")]
pub(crate) fn frame_check_before(call: &mut Call) {
    if options::options().crossings.enabled {
        crossings::check_before(call);
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_check_before(_call: &mut Call) {}

/// A check for a frame inside a day ends (`drawing`).
#[cfg(feature = "dev")]
pub(crate) fn frame_check_after(call: &mut Call) {
    if options::options().crossings.enabled {
        crossings::check_after(call);
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_check_after(_call: &mut Call) {}

/// An interface frame has been prepared for presentation (`drawing`).
#[cfg(feature = "dev")]
pub(crate) fn frame_prepared() {
    performance::prepared();
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_prepared() {}

/// A prepared interface frame was invalidated by a crossing (`drawing::crossings`).
#[cfg(feature = "dev")]
pub(crate) fn frame_invalidated() {
    performance::invalidated();
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_invalidated() {}

/// The drawing thread is about to draw a frame; returns whether it is watched.
#[cfg(feature = "dev")]
pub(crate) fn drawing_frame_begins(m: &Memory) -> bool {
    write_watch::drawing::frame_begins(m)
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_frame_begins(_m: &Memory) -> bool {
    false
}

/// The drawing thread finished drawing a frame.
#[cfg(feature = "dev")]
pub(crate) fn drawing_frame_ends(watched: bool) {
    write_watch::drawing::frame_ends(watched);
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn drawing_frame_ends(_watched: bool) {}

/// A `Present` call begins (`framework_ui::hook`).
#[cfg(feature = "dev")]
pub(crate) fn submission_begins() -> Submission {
    Submission(performance::submission_begins())
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn submission_begins() -> Submission {
    Submission
}

/// A `Present` call returned (`framework_ui::hook`).
#[cfg(feature = "dev")]
pub(crate) fn submission_ends(submission: Submission, success: bool, main_thread: bool) {
    performance::submission_ends(submission.0, success, main_thread);
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn submission_ends(_submission: Submission, _success: bool, _main_thread: bool) {}

/// A graphics device reset is about to run (`framework_ui::hook`).
#[cfg(feature = "dev")]
pub(crate) fn device_reset() {
    performance::interruption("device reset");
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn device_reset() {}

/// A safe-point frame boundary (`safe_point`).
#[cfg(feature = "dev")]
pub(crate) fn frame_tick() {
    performance_hooks::frame();
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_tick() {}

/// The current game session ended (`safe_point`).
#[cfg(feature = "dev")]
pub(crate) fn session_ended() {
    performance_hooks::session_ended();
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn session_ended() {}

/// The game world reset (`sessions`).
#[cfg(feature = "dev")]
pub(crate) fn world_reset() {
    if options::options().heap_census.enabled {
        census::world_reset();
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn world_reset() {}

/// A local save begins (`saving::background`).
#[cfg(feature = "dev")]
pub(crate) fn save_begins(call: &mut Call) {
    performance_hooks::save_begins(call);
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn save_begins(_call: &mut Call) {}

/// A local save ends (`saving::background`).
#[cfg(feature = "dev")]
pub(crate) fn save_ends(call: &mut Call) {
    performance_hooks::save_ends(call);
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn save_ends(_call: &mut Call) {}

/// Marks a thread as freeing game memory on behalf of `repair`, for the heap
/// census's quarantine (`repairs`).
#[cfg(feature = "dev")]
pub(crate) fn quarantine_begin(repair: &'static str) {
    if options::options().heap_census.enabled {
        census::quarantine::begin(repair);
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn quarantine_begin(_repair: &'static str) {}

/// Ends a quarantine scope started by [`quarantine_begin`].
#[cfg(feature = "dev")]
pub(crate) fn quarantine_end() {
    if options::options().heap_census.enabled {
        census::quarantine::end();
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn quarantine_end() {}

/// An after-observer that closes a quarantine scope opened in a before-observer
/// (`repairs::text_boxes`), installed only when the heap census is enabled.
#[cfg(feature = "dev")]
pub(crate) fn quarantine_after_observer() -> Option<fn(&mut Call)> {
    fn end(_: &mut Call) {
        census::quarantine::end();
    }
    options::options().heap_census.enabled.then_some(end)
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn quarantine_after_observer() -> Option<fn(&mut Call)> {
    None
}

/// Answers remote control's `memory` capability group (`catalogue`).
#[cfg(feature = "dev")]
pub(crate) fn answer_memory(request: eu4_harmony_remote::memory::MemoryRequest) -> Result<eu4_harmony_remote::memory::MemoryAnswer, eu4_harmony_remote::Error> {
    memory_account::answer(request)
}

/// Answers remote control's `performance` capability group (`catalogue`).
#[cfg(feature = "dev")]
pub(crate) fn answer_performance(
    request: eu4_harmony_remote::performance::PerformanceRequest,
) -> crate::game::GResult<eu4_harmony_remote::performance::PerformanceAnswer> {
    performance_hooks::answer(request)
}
