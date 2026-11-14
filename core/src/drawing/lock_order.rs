//! End lock-order-only presentation ownership at checked stream-helper returns.
//! Cold construction, abnormal completion and genuine crossings retain the
//! ordinary frame-check lifetime. See docs/research/frame-drawing-p99-safe-boundary.md.
use super::{main_side, on_main, MainLock};
use crate::{hooks::Call, points::Point};
use std::sync::atomic::{AtomicU32, Ordering};

static DEPTH: AtomicU32 = AtomicU32::new(0);

pub(super) fn active() -> bool {
    DEPTH.load(Ordering::Relaxed) != 0
}

fn before(_: &mut Call) {
    if on_main() {
        DEPTH.fetch_add(1, Ordering::Relaxed);
        main_side(|s| s.order_scope_begins());
    }
}

fn after(call: &mut Call) {
    if on_main() {
        main_side(|s| s.order_scope_ends(&MainLock, call.faulted()));
        DEPTH.fetch_sub(1, Ordering::Relaxed);
    }
}

fn cold_path(_: &mut Call) {
    if on_main() && active() {
        main_side(|s| s.order_scope_cold());
    }
}

pub(super) fn points() -> Vec<Point> {
    // Each lookup's normal return follows its outer guard destructor.
    // Cold construction and its subsequent virtual call are excluded at the
    // cold entry, not guessed from an unlocked global-cache snapshot.
    // Evidence: docs/research/frame-drawing-p99-safe-boundary.md.
    vec![
        Point::keep(0x91b59, "48 8d 4c 24 48 e8 65 ac 97 01 48 8b c3 48 8b 5c 24 58 48 83 c4 20 5f 5e 5d c3", "facet normal exit releases its guard"),
        Point::keep(0x91b24, "48 8b d5 48 8d 4c 24 50 e8 af fd ff ff", "facet cold construction call"),
        Point::keep(0x91ad0, "48 8d 4c 24 40 e8 ee ac 97 01", "facet nested guard release"),
        Point::keep(0x1a0c7c8, "48 83 ec 28 48 63 01 85 c0 75 09 48 83 c4 28 e9 44 2d 05 00", "lock-zero guard destruction"),
        Point::keep(0x1a5f520, "48 8d 0d a9 d8 8d 00 48 ff 25 2a 72 10 00", "lock-zero LeaveCriticalSection"),
        Point::observe(0x1a0c408, "48 8b c4 48 89 58 10 48 89 68 18 48 89 70 20", "stream shared-state scope", 1, Some(before), Some(after)),
        Point::observe(0x91a70, "48 89 5c 24 20 55 56 57 48 83 ec 20", "stream facet lookup scope", 1, Some(before), Some(after)),
        Point::observe(0x918e0, "48 89 5c 24 10 48 89 74 24 18 55 57 41 56", "facet cold path retains ownership", 2, Some(cold_path), None),
        Point::keep(0x91759, "48 8d 4c 24 48 e8 65 b0 97 01 48 8b c3 48 8b 5c 24 58 48 83 c4 20 5f 5e 5d c3", "later facet normal exit releases its guard"),
        Point::keep(0x91724, "48 8b d5 48 8d 4c 24 50 e8 bf fd ff ff", "later facet cold construction call"),
        Point::keep(0x916d0, "48 8d 4c 24 40 e8 ee b0 97 01", "later facet nested guard release"),
        Point::observe(0x91670, "48 89 5c 24 20 55 56 57 48 83 ec 20", "later stream facet lookup scope", 1, Some(before), Some(after)),
        Point::observe(0x914f0, "48 89 5c 24 10 48 89 74 24 18 55 57 41 56", "later facet cold path retains ownership", 2, Some(cold_path), None),
        Point::keep(0x924d9, "48 8d 4c 24 48 e8 e5 a2 97 01 48 8b c3 48 8b 5c 24 58 48 83 c4 20 5f 5e 5d c3", "numeric facet normal exit releases its guard"),
        Point::keep(0x924a4, "48 8b d5 48 8d 4c 24 50 e8 df fb ff ff", "numeric facet cold construction call"),
        Point::keep(0x92450, "48 8d 4c 24 40 e8 6e a3 97 01", "numeric facet nested guard release"),
        Point::observe(0x923f0, "48 89 5c 24 20 55 56 57 48 83 ec 20", "numeric stream facet lookup scope", 1, Some(before), Some(after)),
        Point::observe(
            0x92090,
            "40 55 53 56 57 41 56 48 8d 6c 24 e0 48 81 ec 20 01 00 00",
            "numeric facet cold path retains ownership",
            2,
            Some(cold_path),
            None,
        ),
    ]
}
