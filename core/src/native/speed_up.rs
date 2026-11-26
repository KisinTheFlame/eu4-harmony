//! Speed-up descriptors and `dev.toml` switch queries (`game-paths` and
//! `checked`), decoupled from the concrete speed-up list (`ALL`) which top-level
//! assembly registers at startup.
use super::points::Point;
use crate::base::{dev_options, Result};
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
}

const MAX_SPEED_UPS: usize = 32;

static REGISTRY: OnceLock<&'static [&'static SpeedUp]> = OnceLock::new();

/// Registers the full ordered list of speed-ups once at startup.
pub(crate) fn set_registry(all: &'static [&'static SpeedUp]) {
    assert!(all.len() <= MAX_SPEED_UPS, "more than {MAX_SPEED_UPS} speed-ups");
    let _ = REGISTRY.set(all);
}

fn all() -> &'static [&'static SpeedUp] {
    REGISTRY.get().copied().unwrap_or(&[])
}

/// The speed-ups dev.toml leaves to the game and checks, each a bit of its
/// place in the registered speed-up list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Chosen {
    pub(crate) game_paths: u32,
    pub(crate) checked: u32,
}

static CHOSEN: OnceLock<Chosen> = OnceLock::new();

/// What dev.toml's `[test-switches]` chose, once read; nothing in a player
/// build, or when a name is not a speed-up's, which [`check_switches`] fails
/// startup for.
fn chosen() -> Chosen {
    *CHOSEN.get_or_init(|| {
        let (game_paths, checked) = dev_options::speed_switches();
        Chosen::of_list(all(), game_paths, checked).unwrap_or_default()
    })
}

impl Chosen {
    /// From dev.toml's lists: a speed-up's name, its group or `all`; a
    /// speed-up both left to the game and checked is left to the game.
    pub(crate) fn of_list(list: &[&SpeedUp], game_paths: &[String], checked: &[String]) -> Result<Self> {
        let game_paths = named(list, game_paths, "game-paths", |_| true)?;
        let checked = named(list, checked, "checked", |s| s.checkable)? & !game_paths;
        Ok(Self { game_paths, checked })
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

/// Fails startup when dev.toml names a speed-up there is none of.
pub(crate) fn check_switches(list: &[&SpeedUp]) -> Result<()> {
    let (game_paths, checked) = dev_options::speed_switches();
    Chosen::of_list(list, game_paths, checked)?;
    Ok(())
}

/// Its place in the registered speed-up list, if registered.
fn place(speed_up: &SpeedUp) -> Option<usize> {
    all().iter().position(|s| s.name == speed_up.name)
}

/// Whether the speed-up is left to the game, which its points then are:
/// asked by code of it the game may reach without them.
pub(crate) fn game_path(speed_up: &SpeedUp) -> bool {
    place(speed_up).is_some_and(|i| chosen().game_paths & 1 << i != 0)
}

/// Whether what the speed-up answers is checked against the game's answer.
pub(crate) fn checked(speed_up: &SpeedUp) -> bool {
    place(speed_up).is_some_and(|i| chosen().checked & 1 << i != 0)
}

/// Answers checked and answers that differed, for each speed-up.
static COUNTS: [[AtomicU64; 2]; MAX_SPEED_UPS] = [const { [AtomicU64::new(0), AtomicU64::new(0)] }; MAX_SPEED_UPS];
/// A speed-up's first differences are each logged; after these, only when
/// their count reaches a power of two.
const LOGGED: u64 = 20;

/// The speed-up's answer was checked against the game's: `same` when they
/// agreed, `what` saying what differed when they did not.
pub(crate) fn compared(speed_up: &SpeedUp, same: bool, what: impl FnOnce() -> String) {
    let Some(i) = place(speed_up) else { return };
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

/// How many of the speed-up's answers were checked, and how many of those
/// differed from the game's.
pub(crate) fn differences(speed_up: &SpeedUp) -> (u64, u64) {
    let Some(i) = place(speed_up) else { return (0, 0) };
    let counts = &COUNTS[i];
    (counts[0].load(Ordering::Relaxed), counts[1].load(Ordering::Relaxed))
}

/// The startup report's lines: the speed-ups left to the game and checked;
/// none when every speed-up is on and none is checked.
pub(crate) fn report_lines() -> Vec<String> {
    lines(all(), chosen())
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
pub(crate) fn check_lines() -> Vec<String> {
    let chosen = chosen();
    all()
        .iter()
        .enumerate()
        .filter(|(i, _)| chosen.checked & 1 << i != 0)
        .map(|(_, s)| {
            let (checked, differed) = differences(s);
            format!("{}: {checked} answer(s) checked, {differed} differing from the game's", s.name)
        })
        .collect()
}
