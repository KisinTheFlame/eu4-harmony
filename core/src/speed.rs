//! The speed-ups: each a change that makes the game faster (docs/plans/README.md#performance,
//! docs/game/program/loading.md; ADR the-game-may-be-made-faster-with-reasoned-differences),
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
pub(crate) use crate::native::speed_up::{check_lines, checked, compared, differences, game_path, report_lines, SpeedUp};
use crate::native::points::Point;

pub mod pops;

/// Every speed-up, in the order the report names them.
pub static ALL: [&SpeedUp; 20] = [
    &pops::SPEED_UP,
    &crate::drawing::SPEED_UP,
    &crate::loading::music::SPEED_UP,
    &crate::loading::lookups::SPEED_UP,
    &crate::loading::children::SPEED_UP,
    &crate::loading::opens::SPEED_UP,
    &crate::loading::listed::SPEED_UP,
    &crate::loading::transfer::SPEED_UP,
    &crate::loading::save_input::SPEED_UP,
    &crate::loading::distance_scratch::SPEED_UP,
    &crate::loading::parameters::SPEED_UP,
    &crate::loading::localisation::SPEED_UP,
    &crate::loading::trigger_profile::SPEED_UP,
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

/// Fails startup when dev.toml names a speed-up there is none of.
pub fn check_switches() -> crate::base::Result<()> {
    crate::native::speed_up::check_switches(&ALL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::speed_up::{lines, Chosen};

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
        assert!(ALL.len() <= 32);
    }

    #[test]
    fn a_speed_up_is_named_by_its_name_its_group_or_all() {
        let chosen = Chosen::of_list(&ALL, &strings(&["music", "saving"]), &[]).unwrap();
        let saving = ["background-compression", "stored-copy", "parallel-saves", "save-sum", "update-thread"].iter().fold(0, |b, n| b | bit(n));
        assert_eq!(chosen.game_paths, bit("music") | saving);
        assert_eq!(Chosen::of_list(&ALL, &strings(&["all"]), &[]).unwrap().game_paths.count_ones() as usize, ALL.len());
        assert_eq!(Chosen::of_list(&ALL, &[], &[]).unwrap(), Chosen::default());
    }

    #[test]
    fn only_what_can_be_checked_is_and_the_game_s_path_comes_first() {
        let all = Chosen::of_list(&ALL, &[], &strings(&["all"])).unwrap();
        assert_eq!(
            all.checked,
            ["music", "lookups", "listed-folders", "script-parameters", "localisation", "trigger-profile", "parallel-saves", "army-searches", "modifier-sums"]
                .iter()
                .fold(0, |b, n| b | bit(n))
        );
        let both = Chosen::of_list(&ALL, &strings(&["music"]), &strings(&["loading"])).unwrap();
        assert_eq!(
            (both.game_paths, both.checked),
            (bit("music"), bit("lookups") | bit("listed-folders") | bit("script-parameters") | bit("localisation") | bit("trigger-profile"))
        );
        assert!(Chosen::of_list(&ALL, &[], &strings(&["children"])).unwrap_err().contains("cannot be checked"));
        assert!(Chosen::of_list(&ALL, &strings(&["muisc"]), &[]).unwrap_err().contains("no speed-up"));
    }

    #[test]
    fn the_report_names_what_was_chosen() {
        assert!(lines(&ALL, Chosen::default()).is_empty());
        let chosen = Chosen::of_list(&ALL, &strings(&["frames"]), &strings(&["army-searches"])).unwrap();
        assert_eq!(
            lines(&ALL, chosen),
            ["speed-ups left to the game: pops-polling, drawing-thread, modifier-sums", "speed-ups checked against the game: army-searches"]
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
        crate::native::speed_up::set_registry(&ALL);
        let speed_up = &crate::saving::sum::SPEED_UP;
        let (checked, differed) = differences(speed_up);
        compared(speed_up, true, || unreachable!("an agreement says nothing"));
        compared(speed_up, false, || "the sum".into());
        assert_eq!(differences(speed_up), (checked + 2, differed + 1));
    }
}
