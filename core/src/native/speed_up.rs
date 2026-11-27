//! Speed-up descriptors, switch validation, point-plan assembly, and answer
//! comparison (`ADR the-game-may-be-made-faster-with-reasoned-differences`).
//!
//! In a player build (`not(feature = "dev")`), [`game_path`] and [`checked`]
//! compile to constant `false`, eliminating hot-path registry lookups.
//! In a developer build (`feature = "dev"`), [`init`] validates `dev.toml`'s
//! `game-paths` and `checked` lists once at startup against the full speed-up
//! list and installs the resolved [`SpeedUpPlan`].
use super::points::Point;
use crate::base::Result;
#[cfg(feature = "dev")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// One speed-up.
pub struct SpeedUp {
    /// Its name in dev.toml and the report.
    pub name: &'static str,
    /// The group it is also named with: `frames`, `loading`, `saving`, `ai`.
    pub group: &'static str,
    /// Its points, which are installed unless the game is left its own path.
    pub points: fn() -> Vec<Point>,
    /// Whether what it answers can be checked against the game's answer: its
    /// module then calls [`compared`] for each answer it checks.
    pub checkable: bool,
    /// Optional point transformation applied when checked against the game in
    /// a developer build (also applied when gathering declared points for
    /// offline checks).
    pub compare_points: Option<fn(&mut Vec<Point>)>,
    /// Optional observation points retained even when this speed-up is left to
    /// the game, whenever an active tool needs them (e.g., performance save
    /// observation while background compression is left to the game).
    pub shared_points: Option<fn() -> Vec<Point>>,
}

impl SpeedUp {
    pub const fn new(name: &'static str, group: &'static str, points: fn() -> Vec<Point>, checkable: bool) -> Self {
        Self { name, group, points, checkable, compare_points: None, shared_points: None }
    }

    #[cfg(any(test, feature = "dev"))]
    pub const fn with_compare(mut self, compare_points: fn(&mut Vec<Point>)) -> Self {
        self.compare_points = Some(compare_points);
        self
    }

    pub const fn with_shared(mut self, shared_points: fn() -> Vec<Point>) -> Self {
        self.shared_points = Some(shared_points);
        self
    }
}

const MAX_SPEED_UPS: usize = 32;

/// The speed-ups `dev.toml` leaves to the game and checks, each a bit of its
/// place in the registered speed-up list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Chosen {
    pub(crate) game_paths: u32,
    pub(crate) checked: u32,
}

impl Chosen {
    /// From `dev.toml`'s lists: a speed-up's name, its group or `all`; a
    /// speed-up both left to the game and checked is left to the game.
    pub(crate) fn of_list(list: &[&SpeedUp], game_paths: &[String], checked: &[String]) -> Result<Self> {
        let game_paths = named(list, game_paths, "game-paths", |_| true)?;
        let checked = named(list, checked, "checked", |s| s.checkable)? & !game_paths;
        Ok(Self { game_paths, checked })
    }
}

/// A validated speed-up plan for one run (or test).
#[derive(Clone, Copy)]
pub(crate) struct SpeedUpPlan<'a> {
    list: &'a [&'a SpeedUp],
    chosen: Chosen,
}

impl<'a> SpeedUpPlan<'a> {
    pub(crate) fn new(list: &'a [&'a SpeedUp], game_paths: &[String], checked: &[String]) -> Result<Self> {
        assert!(list.len() <= MAX_SPEED_UPS, "more than {MAX_SPEED_UPS} speed-ups");
        let chosen = Chosen::of_list(list, game_paths, checked)?;
        Ok(Self { list, chosen })
    }

    #[cfg(any(test, feature = "dev"))]
    fn place(&self, speed_up: &SpeedUp) -> Option<usize> {
        self.list.iter().position(|s| s.name == speed_up.name)
    }

    #[cfg(any(test, feature = "dev"))]
    pub(crate) fn is_game_path(&self, speed_up: &SpeedUp) -> bool {
        self.place(speed_up).is_some_and(|i| self.chosen.game_paths & (1 << i) != 0)
    }

    #[cfg(any(test, feature = "dev"))]
    pub(crate) fn is_checked(&self, speed_up: &SpeedUp) -> bool {
        self.place(speed_up).is_some_and(|i| self.chosen.checked & (1 << i) != 0)
    }

    /// Every point declared by this list (including developer check-mode
    /// replacements), for offline byte verification.
    pub(crate) fn declared_points(list: &[&SpeedUp]) -> Vec<Point> {
        #[cfg(feature = "dev")]
        {
            let mut all: Vec<Point> = list.iter().flat_map(|s| (s.points)()).collect();
            for s in list {
                if let Some(replace) = s.compare_points {
                    replace(&mut all);
                }
            }
            all
        }
        #[cfg(not(feature = "dev"))]
        {
            list.iter().flat_map(|s| (s.points)()).collect()
        }
    }

    /// Points to install for this plan: every speed-up not left to the game,
    /// plus `compare_points` for checked speed-ups and `shared_points` when
    /// `keep_shared` is set.
    pub(crate) fn installed_points(&self, keep_shared: bool) -> Vec<Point> {
        let mut all = Vec::new();
        for (i, s) in self.list.iter().enumerate() {
            if self.chosen.game_paths & (1 << i) == 0 {
                all.extend((s.points)());
                if self.chosen.checked & (1 << i) != 0 {
                    if let Some(replace) = s.compare_points {
                        replace(&mut all);
                    }
                }
            } else if keep_shared {
                if let Some(shared) = s.shared_points {
                    all.extend(shared());
                }
            }
        }
        all
    }

    pub(crate) fn report_lines(&self) -> Vec<String> {
        lines(self.list, self.chosen)
    }
}

static PLAN: OnceLock<SpeedUpPlan<'static>> = OnceLock::new();

/// Validates `game_paths` and `checked` against `all` and installs the active
/// plan once at startup.
pub(crate) fn init(all: &'static [&'static SpeedUp], game_paths: &[String], checked: &[String]) -> Result<()> {
    let plan = SpeedUpPlan::new(all, game_paths, checked)?;
    let _ = PLAN.set(plan);
    Ok(())
}

/// Every speed-up's declared points (for offline checks and `Memory::at`).
pub(crate) fn declared_points(all: &[&SpeedUp]) -> Vec<Point> {
    SpeedUpPlan::declared_points(all)
}

/// The points startup installs for the initialized plan.
pub(crate) fn installed_points(all: &[&SpeedUp], keep_shared: bool) -> Vec<Point> {
    match PLAN.get() {
        Some(plan) => plan.installed_points(keep_shared),
        None => SpeedUpPlan { list: all, chosen: Chosen::default() }.installed_points(keep_shared),
    }
}

/// The speed-ups `names` names among those `taking` takes, as bits; a name
/// that names none of them is refused.
fn named(all: &[&SpeedUp], names: &[String], list: &str, taking: impl Fn(&SpeedUp) -> bool) -> Result<u32> {
    let mut bits = 0;
    for name in names {
        let these: Vec<usize> = (0..all.len()).filter(|&i| name == "all" || all[i].name == name || all[i].group == name).collect();
        if these.is_empty() {
            let known: Vec<&str> = all.iter().map(|s| s.name).collect();
            return Err(format!("[test-switches] {list}: {name:?} is no speed-up, group or all; the speed-ups are {}", known.join(", ")));
        }
        let taken: Vec<usize> = these.iter().copied().filter(|&i| taking(all[i])).collect();
        if taken.is_empty() {
            return Err(format!("[test-switches] {list}: {name:?} cannot be checked against the game"));
        }
        bits |= taken.iter().fold(0, |b, &i| b | 1 << i);
    }
    Ok(bits)
}

/// Whether the speed-up is left to the game, which its points then are:
/// asked by code of it the game may reach without them.
#[cfg(feature = "dev")]
pub(crate) fn game_path(speed_up: &SpeedUp) -> bool {
    PLAN.get().is_some_and(|p| p.is_game_path(speed_up))
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn game_path(_speed_up: &SpeedUp) -> bool {
    false
}

/// Whether what the speed-up answers is checked against the game's answer.
#[cfg(feature = "dev")]
pub(crate) fn checked(speed_up: &SpeedUp) -> bool {
    PLAN.get().is_some_and(|p| p.is_checked(speed_up))
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) const fn checked(_speed_up: &SpeedUp) -> bool {
    false
}

/// Answers checked and answers that differed, for each speed-up.
#[cfg(feature = "dev")]
static COUNTS: [[AtomicU64; 2]; MAX_SPEED_UPS] = [const { [AtomicU64::new(0), AtomicU64::new(0)] }; MAX_SPEED_UPS];
/// A speed-up's first differences are each logged; after these, only when
/// their count reaches a power of two.
#[cfg(feature = "dev")]
const LOGGED: u64 = 20;

/// The speed-up's answer was checked against the game's: `same` when they
/// agreed, `what` saying what differed when they did not.
#[cfg(feature = "dev")]
pub(crate) fn compared(speed_up: &SpeedUp, same: bool, what: impl FnOnce() -> String) {
    let Some(i) = PLAN.get().and_then(|p| p.place(speed_up)) else { return };
    let counts = &COUNTS[i];
    let checked = counts[0].fetch_add(1, Ordering::Relaxed) + 1;
    if same {
        return;
    }
    let differed = counts[1].fetch_add(1, Ordering::Relaxed) + 1;
    if differed <= LOGGED || differed.is_power_of_two() {
        log!("speed-up {}: differs from the game's: {} ({differed} of {checked} checked differ)", speed_up.name, what());
    }
}

#[cfg(not(feature = "dev"))]
#[inline(always)]
pub(crate) fn compared(_speed_up: &SpeedUp, _same: bool, _what: impl FnOnce() -> String) {}

/// How many of the speed-up's answers were checked, and how many of those
/// differed from the game's.
#[cfg(feature = "dev")]
pub(crate) fn differences(speed_up: &SpeedUp) -> (u64, u64) {
    let Some(i) = PLAN.get().and_then(|p| p.place(speed_up)) else { return (0, 0) };
    let counts = &COUNTS[i];
    (counts[0].load(Ordering::Relaxed), counts[1].load(Ordering::Relaxed))
}

/// The startup report's lines: the speed-ups left to the game and checked;
/// none when every speed-up is on and none is checked.
pub(crate) fn report_lines() -> Vec<String> {
    PLAN.get().map_or_else(Vec::new, SpeedUpPlan::report_lines)
}

pub(crate) fn lines(list: &[&SpeedUp], chosen: Chosen) -> Vec<String> {
    let names = |bits: u32| list.iter().enumerate().filter(|(i, _)| bits & 1 << i != 0).map(|(_, s)| s.name).collect::<Vec<_>>().join(", ");
    let mut lines = Vec::new();
    if chosen.game_paths != 0 {
        lines.push(format!("speed-ups left to the game: {}", names(chosen.game_paths)));
    }
    if chosen.checked != 0 {
        lines.push(format!("speed-ups checked against the game: {}", names(chosen.checked)));
    }
    lines
}

/// What came of the checks so far, one line a speed-up checked.
#[cfg(feature = "dev")]
pub(crate) fn check_lines() -> Vec<String> {
    let Some(plan) = PLAN.get() else { return Vec::new() };
    plan.list
        .iter()
        .enumerate()
        .filter(|(i, _)| plan.chosen.checked & 1 << i != 0)
        .map(|(_, s)| {
            let (checked, differed) = differences(s);
            format!("{}: {checked} answer(s) checked, {differed} differing from the game's", s.name)
        })
        .collect()
}

#[cfg(not(feature = "dev"))]
pub(crate) fn check_lines() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p_main() -> Vec<Point> {
        vec![Point::keep(0x1000usize, "90", "main")]
    }
    fn p_shared() -> Vec<Point> {
        vec![Point::keep(0x2000usize, "90", "shared")]
    }
    fn replace_cmp(points: &mut Vec<Point>) {
        points.push(Point::keep(0x3000usize, "90", "compare"));
    }

    static S_A: SpeedUp = SpeedUp::new("alpha", "saving", p_main, true).with_compare(replace_cmp);
    static S_B: SpeedUp = SpeedUp::new("beta", "saving", p_main, false).with_shared(p_shared);

    #[test]
    fn plan_assembles_installed_compare_and_shared_points_without_globals() {
        let list = [&S_A, &S_B];
        // Both enabled, alpha checked -> alpha gets compare points, beta gets main points.
        let plan = SpeedUpPlan::new(&list, &[], &["alpha".into()]).unwrap();
        assert!(!plan.is_game_path(&S_A) && plan.is_checked(&S_A));
        let whats: Vec<_> = plan.installed_points(false).into_iter().map(|p| p.what).collect();
        assert_eq!(whats, ["main", "compare", "main"]);

        // Both left to game, keep_shared=true -> only beta's shared observation points stay.
        let left = SpeedUpPlan::new(&list, &["saving".into()], &[]).unwrap();
        assert!(left.is_game_path(&S_A) && left.is_game_path(&S_B));
        assert!(left.installed_points(false).is_empty());
        let shared: Vec<_> = left.installed_points(true).into_iter().map(|p| p.what).collect();
        assert_eq!(shared, ["shared"]);
    }
}
