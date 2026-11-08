//! The speed-ups: each a change that makes the game faster (docs/game-speed.md,
//! docs/loading.md; ADR the-game-may-be-made-faster-with-reasoned-differences),
//! declared once with its name, the group it is named with, its dependency
//! points, and whether what it answers can be checked against the game's own
//! answer.
//!
//! A developer build with the test switches can leave any of them to the
//! game, by its name, its group or `all` in dev.toml's `game-paths`: its points
//! are then not installed, and the game runs its own code there, so that the
//! two can be measured against each other. It can check any that can be
//! checked, in `checked`: the speed-up then works out the game's answer as
//! well, keeps the game's, and tells [`compared`] whether the two agreed,
//! which logs a difference one way and counts both for the startup report.
//! A player's build installs every speed-up and checks none.
use crate::points::Point;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

pub mod pace;
pub mod pops;

pub use pace::{set_pace, Pace};

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

/// Every speed-up, in the order the report names them.
pub static ALL: [&SpeedUp; 14] = [
    &pops::SPEED_UP,
    &pace::SPEED_UP,
    &crate::loading::music::SPEED_UP,
    &crate::loading::lookups::SPEED_UP,
    &crate::loading::children::SPEED_UP,
    &crate::loading::opens::SPEED_UP,
    &crate::loading::listed::SPEED_UP,
    &crate::saving::background::SPEED_UP,
    &crate::saving::stored::SPEED_UP,
    &crate::saving::parallel::SPEED_UP,
    &crate::saving::sum::SPEED_UP,
    &crate::saving::update_thread::SPEED_UP,
    &crate::army_search::SPEED_UP,
    &crate::modifier_sums::SPEED_UP,
];

/// Every speed-up's points: what the offline checks check and the framework
/// may use, whether startup installs them or not.
pub fn points() -> Vec<Point> {
    ALL.iter().flat_map(|s| (s.points)()).collect()
}

/// The points startup installs: those of the speed-ups not left to the game.
pub fn installed() -> Vec<Point> {
    ALL.iter().filter(|s| !game_path(s)).flat_map(|s| (s.points)()).collect()
}

/// The speed-ups dev.toml leaves to the game and checks, each a bit of its
/// place in [`ALL`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Chosen {
    game_paths: u32,
    checked: u32,
}

static CHOSEN: OnceLock<Chosen> = OnceLock::new();

/// What dev.toml's `[test-switches]` chose, once read; nothing in a build
/// without the test switches, or when a name is not a speed-up's, which
/// [`check_switches`] fails startup for.
fn chosen() -> Chosen {
    *CHOSEN.get_or_init(|| {
        #[cfg(feature = "dev-test-switches")]
        let chosen = {
            let switches = &crate::dev::options().test_switches;
            Chosen::of(&switches.game_paths, &switches.checked).unwrap_or_default()
        };
        #[cfg(not(feature = "dev-test-switches"))]
        let chosen = Chosen::default();
        chosen
    })
}

#[cfg(any(test, feature = "dev-test-switches"))]
impl Chosen {
    /// From dev.toml's lists: a speed-up's name, its group or `all`; a
    /// speed-up both left to the game and checked is left to the game.
    fn of(game_paths: &[String], checked: &[String]) -> crate::Result<Self> {
        let game_paths = named(game_paths, "game-paths", |_| true)?;
        let checked = named(checked, "checked", |s| s.checkable)? & !game_paths;
        Ok(Self { game_paths, checked })
    }
}

/// The speed-ups `names` names among those `taking` takes, as bits; a name
/// that names none of them is refused.
#[cfg(any(test, feature = "dev-test-switches"))]
fn named(names: &[String], list: &str, taking: impl Fn(&SpeedUp) -> bool) -> crate::Result<u32> {
    let mut bits = 0;
    for name in names {
        let these: Vec<usize> = (0..ALL.len()).filter(|&i| name == "all" || ALL[i].name == name || ALL[i].group == name).collect();
        if these.is_empty() {
            let known: Vec<&str> = ALL.iter().map(|s| s.name).collect();
            return Err(format!("[test-switches] {list}: {name:?} is no speed-up, group or all; the speed-ups are {}", known.join(", ")));
        }
        let taken: Vec<usize> = these.iter().copied().filter(|&i| taking(ALL[i])).collect();
        if taken.is_empty() {
            return Err(format!("[test-switches] {list}: {name:?} cannot be checked against the game"));
        }
        bits |= taken.iter().fold(0, |b, &i| b | 1 << i);
    }
    Ok(bits)
}

/// Fails startup when dev.toml names a speed-up there is none of.
pub fn check_switches() -> crate::Result<()> {
    #[cfg(feature = "dev-test-switches")]
    {
        let switches = &crate::dev::options().test_switches;
        Chosen::of(&switches.game_paths, &switches.checked)?;
    }
    Ok(())
}

/// Its place in [`ALL`].
fn place(speed_up: &SpeedUp) -> usize {
    ALL.iter().position(|s| s.name == speed_up.name).expect("every speed-up is in ALL")
}

/// Whether the speed-up is left to the game, which its points then are:
/// asked by code of it the game may reach without them.
pub fn game_path(speed_up: &SpeedUp) -> bool {
    chosen().game_paths & 1 << place(speed_up) != 0
}

/// Whether what the speed-up answers is checked against the game's answer.
pub fn checked(speed_up: &SpeedUp) -> bool {
    chosen().checked & 1 << place(speed_up) != 0
}

/// Answers checked and answers that differed, for each speed-up.
static COUNTS: [[AtomicU64; 2]; 14] = [const { [AtomicU64::new(0), AtomicU64::new(0)] }; 14];
/// A speed-up's first differences are each logged; after these, only when
/// their count reaches a power of two.
const LOGGED: u64 = 20;

/// The speed-up's answer was checked against the game's: `same` when they
/// agreed, `what` saying what differed when they did not.
pub fn compared(speed_up: &SpeedUp, same: bool, what: impl FnOnce() -> String) {
    let counts = &COUNTS[place(speed_up)];
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
pub fn differences(speed_up: &SpeedUp) -> (u64, u64) {
    let counts = &COUNTS[place(speed_up)];
    (counts[0].load(Ordering::Relaxed), counts[1].load(Ordering::Relaxed))
}

/// The startup report's lines: the speed-ups left to the game and checked;
/// none when every speed-up is on and none is checked.
pub fn report_lines() -> Vec<String> {
    lines(chosen())
}

fn lines(chosen: Chosen) -> Vec<String> {
    let names = |bits: u32| ALL.iter().enumerate().filter(|(i, _)| bits & 1 << i != 0).map(|(_, s)| s.name).collect::<Vec<_>>().join(", ");
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
pub fn check_lines() -> Vec<String> {
    let chosen = chosen();
    ALL.iter()
        .enumerate()
        .filter(|(i, _)| chosen.checked & 1 << i != 0)
        .map(|(_, s)| {
            let (checked, differed) = differences(s);
            format!("{}: {checked} answer(s) checked, {differed} differing from the game's", s.name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn bit(name: &str) -> u32 {
        1 << ALL.iter().position(|s| s.name == name).unwrap()
    }

    #[test]
    fn every_speed_up_has_a_name_of_its_own_and_a_group() {
        for (i, s) in ALL.iter().enumerate() {
            assert!(ALL[..i].iter().all(|t| t.name != s.name), "{} twice", s.name);
            assert!(ALL.iter().all(|t| t.group != s.name), "{} is a group's name too", s.name);
            assert!(["frames", "loading", "saving", "ai"].contains(&s.group), "{}'s group {}", s.name, s.group);
        }
        assert_eq!(COUNTS.len(), ALL.len());
        assert!(ALL.len() <= 32);
    }

    #[test]
    fn a_speed_up_is_named_by_its_name_its_group_or_all() {
        let chosen = Chosen::of(&strings(&["music", "saving"]), &[]).unwrap();
        let saving = ["background-compression", "stored-copy", "parallel-saves", "save-sum", "update-thread"].iter().fold(0, |b, n| b | bit(n));
        assert_eq!(chosen.game_paths, bit("music") | saving);
        assert_eq!(Chosen::of(&strings(&["all"]), &[]).unwrap().game_paths.count_ones() as usize, ALL.len());
        assert_eq!(Chosen::of(&[], &[]).unwrap(), Chosen::default());
    }

    #[test]
    fn only_what_can_be_checked_is_and_the_game_s_path_comes_first() {
        let all = Chosen::of(&[], &strings(&["all"])).unwrap();
        assert_eq!(all.checked, ["music", "lookups", "listed-folders", "parallel-saves", "army-searches", "modifier-sums"].iter().fold(0, |b, n| b | bit(n)));
        let both = Chosen::of(&strings(&["music"]), &strings(&["loading"])).unwrap();
        assert_eq!((both.game_paths, both.checked), (bit("music"), bit("lookups") | bit("listed-folders")));
        assert!(Chosen::of(&[], &strings(&["children"])).unwrap_err().contains("cannot be checked"));
        assert!(Chosen::of(&strings(&["muisc"]), &[]).unwrap_err().contains("no speed-up"));
    }

    #[test]
    fn the_report_names_what_was_chosen() {
        assert!(lines(Chosen::default()).is_empty());
        let chosen = Chosen::of(&strings(&["frames"]), &strings(&["army-searches"])).unwrap();
        assert_eq!(
            lines(chosen),
            ["speed-ups left to the game: pops-polling, fastest-pace, modifier-sums", "speed-ups checked against the game: army-searches"]
        );
    }

    #[test]
    fn the_example_dev_toml_names_every_speed_up_and_group() {
        let example = include_str!("../dev.toml.example");
        for s in ALL {
            assert!(example.contains(s.name), "{}", s.name);
            assert!(example.contains(&format!("(group \"{}\")", s.group)) || example.contains(&format!("(\"{}\")", s.group)), "{}", s.group);
        }
    }

    #[test]
    fn a_comparison_is_counted_and_a_difference_too() {
        let speed_up = &crate::saving::sum::SPEED_UP;
        let (checked, differed) = differences(speed_up);
        compared(speed_up, true, || unreachable!("an agreement says nothing"));
        compared(speed_up, false, || "the sum".into());
        assert_eq!(differences(speed_up), (checked + 2, differed + 1));
    }
}
