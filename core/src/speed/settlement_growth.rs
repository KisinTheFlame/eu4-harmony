//! Caching the best-province development lookup inside AI settlement-growth
//! evaluation (`0x1b5170` -> `0xbde10`, `docs/game/ai/ai.md`).
//!
//! For every owned province where `may_promote_settlement_growth` (`0x262060`)
//! returns true, `0x1b5170` steps the random generator (`0x14c71b0`), computes
//! `dev_type = (rng as i32) % 3` (which takes at most five values, `-2..=2`),
//! and calls `0xbde10(country_ai, &0, dev_type)` — which scans every owned
//! province of the country and all of its subjects — just to test whether the
//! current province equals the single best province. That makes `0x1b5170`
//! quadratic in empire size.
//!
//! Because `0xbde10` is read-only and `0x1b5170` leaves the world unchanged
//! while scanning, caching `0xbde10(ai, &0, dev_type)` in a 5-slot per-thread
//! scope for the duration of one `0x1b5170` call reduces `O(N^2)` to `O(N)`
//! while advancing the RNG identically and producing bit-for-bit identical
//! results.
use crate::base::called_by_game;
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;

/// AI colonization agent's settlement-growth candidate scan (`(colonize_agent)`).
const SETTLEMENT_SCAN: usize = 0x1b5170;
/// Best province to develop across country and subjects: `(country_ai, *const i32, i32) -> province`.
const BEST_DEV_FN: usize = 0xbde10;

type SettlementScanFn = unsafe extern "C" fn(usize);
type BestDevFn = unsafe extern "C" fn(usize, *const i32, i32) -> usize;

static ORIGINAL_SETTLEMENT_SCAN: Function<SettlementScanFn> = Function::new();
static ORIGINAL_BEST_DEV: Function<BestDevFn> = Function::new();

/// Five-slot cache for `dev_type in -2..=2` during one `0x1b5170` call.
#[derive(Default)]
struct Scope {
    ai: usize,
    entries: [Option<usize>; 5],
}

impl Scope {
    fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn slot_of(dev_type: i32) -> Option<usize> {
        (-2..=2).contains(&dev_type).then_some((dev_type + 2) as usize)
    }

    fn get(&self, ai: usize, dev_type: i32) -> Option<usize> {
        if self.ai != ai {
            return None;
        }
        self.entries[Self::slot_of(dev_type)?]
    }

    fn put(&mut self, ai: usize, dev_type: i32, province: usize) {
        if let Some(slot) = Self::slot_of(dev_type) {
            if self.ai != ai {
                self.ai = ai;
                self.entries = [None; 5];
            }
            self.entries[slot] = Some(province);
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };
}

struct ScopeGuard;

impl ScopeGuard {
    fn enter() -> Self {
        ACTIVE.with(|stack| stack.borrow_mut().push(Scope::new()));
        Self
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        ACTIVE.with(|stack| {
            let _ = stack.borrow_mut().pop();
        });
    }
}

#[called_by_game]
unsafe extern "C" fn settlement_scan(agent: usize) {
    let _guard = ScopeGuard::enter();
    unsafe { ORIGINAL_SETTLEMENT_SCAN.get()(agent) };
}

#[called_by_game]
unsafe extern "C" fn cached_best_dev(ai: usize, cost_ptr: *const i32, dev_type: i32) -> usize {
    let original = ORIGINAL_BEST_DEV.get();
    let in_scan = ACTIVE.with(|stack| !stack.borrow().is_empty());
    if !in_scan {
        return unsafe { original(ai, cost_ptr, dev_type) };
    }
    let cost = if cost_ptr.is_null() { -1 } else { unsafe { *cost_ptr } };
    if cost != 0 || Scope::slot_of(dev_type).is_none() {
        return unsafe { original(ai, cost_ptr, dev_type) };
    }
    let cached = ACTIVE.with(|stack| stack.borrow().last().and_then(|s| s.get(ai, dev_type)));
    if let Some(province) = cached {
        if speed::checked(&SPEED_UP) {
            let game = unsafe { original(ai, cost_ptr, dev_type) };
            speed::compared(&SPEED_UP, province == game, || format!("settlement-growth dev_type={dev_type}: cached {province:#x}, game {game:#x}"));
        }
        return province;
    }
    let province = unsafe { original(ai, cost_ptr, dev_type) };
    ACTIVE.with(|stack| {
        if let Some(scope) = stack.borrow_mut().last_mut() {
            scope.put(ai, dev_type, province);
        }
    });
    province
}

/// Checked, every cached best-development answer is also recomputed and
/// compared with the game's.
pub const SPEED_UP: SpeedUp = SpeedUp::new("settlement-growth", "ai", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            SETTLEMENT_SCAN,
            "40 55 56 41 54 41 56 48 83 ec 38 4c 8b 0d 4e 87 18 02",
            "AI settlement-growth province scan",
            settlement_scan as SettlementScanFn,
            &ORIGINAL_SETTLEMENT_SCAN,
        ),
        Point::replace(
            BEST_DEV_FN,
            "48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57",
            "best province to develop",
            cached_best_dev as BestDevFn,
            &ORIGINAL_BEST_DEV,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_caches_all_five_signed_mod_three_values_and_isolates_nested_calls() {
        let mut scope = Scope::new();
        for dev_type in -2..=2 {
            assert_eq!(scope.get(0x1000, dev_type), None);
            scope.put(0x1000, dev_type, (dev_type + 10) as usize);
            assert_eq!(scope.get(0x1000, dev_type), Some((dev_type + 10) as usize));
        }
        assert_eq!(scope.get(0x2000, 0), None);
        assert_eq!(Scope::slot_of(-3), None);
        assert_eq!(Scope::slot_of(3), None);

        let _outer = ScopeGuard::enter();
        ACTIVE.with(|s| s.borrow_mut().last_mut().unwrap().put(0x1000, 1, 0xaaa));
        {
            let _inner = ScopeGuard::enter();
            assert_eq!(ACTIVE.with(|s| s.borrow().last().unwrap().get(0x1000, 1)), None);
            ACTIVE.with(|s| s.borrow_mut().last_mut().unwrap().put(0x1000, 1, 0xbbb));
            assert_eq!(ACTIVE.with(|s| s.borrow().last().unwrap().get(0x1000, 1)), Some(0xbbb));
        }
        assert_eq!(ACTIVE.with(|s| s.borrow().last().unwrap().get(0x1000, 1)), Some(0xaaa));
    }
}
