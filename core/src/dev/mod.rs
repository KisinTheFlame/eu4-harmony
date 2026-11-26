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

#[cfg(feature = "dev")]
use crate::native::hooks::Call;
use crate::native::{memory::Memory, points::Point};

/// Installs runtime probe handlers into `native::probes` in a developer build.
#[cfg(feature = "dev")]
pub(crate) fn install_probes() {
    fn quarantine_after() -> Option<fn(&mut Call)> {
        fn end(_: &mut Call) {
            census::quarantine::end();
        }
        options::options().heap_census.enabled.then_some(end)
    }

    crate::native::probes::install(crate::native::probes::ProbeHandlers {
        day_begins: |call| {
            if options::options().crossings.enabled {
                crossings::day_begins(&call.memory());
            }
            performance_hooks::day_begins(call);
        },
        day_ends: |call| {
            performance_hooks::day_ends(call);
            if options::options().crossings.enabled {
                crossings::day_ends();
            }
        },
        drawing_day_begins: write_watch::drawing::day_begins,
        frame_check_before: |call| {
            if options::options().crossings.enabled {
                crossings::check_before(call);
            }
        },
        frame_check_after: |call| {
            if options::options().crossings.enabled {
                crossings::check_after(call);
            }
        },
        frame_prepared: performance::prepared,
        frame_invalidated: performance::invalidated,
        drawing_frame_begins: write_watch::drawing::frame_begins,
        drawing_frame_ends: write_watch::drawing::frame_ends,
        submission_begins: || performance::submission_begins().into_probe(),
        submission_ends: |s, ok, main| performance::submission_ends(performance::Submission::from_probe(s), ok, main),
        device_reset: || performance::interruption("device reset"),
        save_begins: performance_hooks::save_begins,
        save_ends: performance_hooks::save_ends,
        quarantine_begin: |repair| {
            if options::options().heap_census.enabled {
                census::quarantine::begin(repair);
            }
        },
        quarantine_end: || {
            if options::options().heap_census.enabled {
                census::quarantine::end();
            }
        },
        quarantine_after_observer: quarantine_after,
    });
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn install_probes() {}

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
    if crate::native::speed_up::checked(&crate::speed::saving::parallel::SPEED_UP)
        && !crate::native::speed_up::game_path(&crate::speed::saving::parallel::SPEED_UP)
    {
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
        match crate::world::commands::Commands::Game.post(text) {
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

/// A safe-point frame boundary (`app::safe_point`).
#[cfg(feature = "dev")]
pub(crate) fn frame_tick() {
    performance_hooks::frame();
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn frame_tick() {}

/// The current game session ended (`app::safe_point`).
#[cfg(feature = "dev")]
pub(crate) fn session_ended() {
    performance_hooks::session_ended();
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn session_ended() {}

/// The game world reset (`session::sessions`).
#[cfg(feature = "dev")]
pub(crate) fn world_reset() {
    if options::options().heap_census.enabled {
        census::world_reset();
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn world_reset() {}

/// Answers remote control's `memory` capability group (`catalogue`).
#[cfg(feature = "dev")]
pub(crate) fn answer_memory(request: eu4_harmony_remote::memory::MemoryRequest) -> Result<eu4_harmony_remote::memory::MemoryAnswer, eu4_harmony_remote::Error> {
    memory_account::answer(request)
}

/// Answers remote control's `performance` capability group (`catalogue`).
#[cfg(feature = "dev")]
pub(crate) fn answer_performance(
    request: eu4_harmony_remote::performance::PerformanceRequest,
) -> crate::world::GResult<eu4_harmony_remote::performance::PerformanceAnswer> {
    performance_hooks::answer(request)
}
