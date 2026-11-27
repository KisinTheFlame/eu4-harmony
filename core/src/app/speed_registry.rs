//! Top-level registry of every speed-up in the core (assembled here so
//! `speed` and `render` stay independent DAG nodes).
use crate::native::points::Point;
use crate::native::speed_up::{self, SpeedUp};

/// Every speed-up, in the order the report names them.
pub(crate) static ALL: [&SpeedUp; 28] = [
    &crate::speed::pops::SPEED_UP,
    &crate::render::drawing::SPEED_UP,
    &crate::speed::loading::music::SPEED_UP,
    &crate::speed::loading::lookups::SPEED_UP,
    &crate::speed::loading::children::SPEED_UP,
    &crate::speed::loading::province_shapes::SPEED_UP,
    &crate::speed::loading::country_distances::SPEED_UP,
    &crate::speed::loading::flag_atlas::SPEED_UP,
    &crate::speed::loading::token_lookup::SPEED_UP,
    &crate::speed::loading::opens::SPEED_UP,
    &crate::speed::loading::listed::SPEED_UP,
    &crate::speed::loading::transfer::SPEED_UP,
    &crate::speed::loading::save_input::SPEED_UP,
    &crate::speed::loading::distance_scratch::SPEED_UP,
    &crate::speed::loading::parameters::SPEED_UP,
    &crate::speed::loading::localisation::SPEED_UP,
    &crate::speed::loading::trigger_profile::SPEED_UP,
    &crate::speed::loading::loading_screen::SPEED_UP,
    &crate::speed::loading::dlc_verify::SPEED_UP,
    &crate::speed::saving::background::SPEED_UP,
    &crate::speed::saving::stored::SPEED_UP,
    &crate::speed::saving::parallel::SPEED_UP,
    &crate::speed::saving::sum::SPEED_UP,
    &crate::speed::saving::update_thread::SPEED_UP,
    &crate::speed::army_checks::SPEED_UP,
    &crate::speed::support_checks::SPEED_UP,
    &crate::speed::settlement_growth::SPEED_UP,
    &crate::speed::modifier_sums::SPEED_UP,
];

/// Every speed-up's points: what the offline checks check and the framework
/// may use, whether startup installs them or not.
pub(crate) fn points() -> Vec<Point> {
    speed_up::declared_points(&ALL)
}

/// The points startup installs: those of the speed-ups not left to the game.
pub(crate) fn installed(keep_shared: bool) -> Vec<Point> {
    speed_up::installed_points(&ALL, keep_shared)
}

/// Validates `dev.toml`'s speed-up switches and installs the plan once.
pub(crate) fn init(game_paths: &[String], checked: &[String]) -> crate::base::Result<()> {
    speed_up::init(&ALL, game_paths, checked)
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
            [
                "music",
                "lookups",
                "country-distances",
                "flag-atlas",
                "token-lookup",
                "listed-folders",
                "script-parameters",
                "localisation",
                "trigger-profile",
                "dlc-verify",
                "parallel-saves",
                "army-checks",
                "support-checks",
                "settlement-growth",
                "modifier-sums"
            ]
            .iter()
            .fold(0, |b, n| b | bit(n))
        );
        let both = Chosen::of_list(&ALL, &strings(&["music"]), &strings(&["loading"])).unwrap();
        assert_eq!(
            (both.game_paths, both.checked),
            (
                bit("music"),
                bit("lookups")
                    | bit("country-distances")
                    | bit("flag-atlas")
                    | bit("token-lookup")
                    | bit("listed-folders")
                    | bit("script-parameters")
                    | bit("localisation")
                    | bit("trigger-profile")
                    | bit("dlc-verify")
            )
        );
        assert!(Chosen::of_list(&ALL, &[], &strings(&["children"])).unwrap_err().contains("cannot be checked"));
        assert!(Chosen::of_list(&ALL, &strings(&["muisc"]), &[]).unwrap_err().contains("no speed-up"));
    }

    #[test]
    fn the_report_names_what_was_chosen() {
        assert!(lines(&ALL, Chosen::default()).is_empty());
        let chosen = Chosen::of_list(&ALL, &strings(&["frames"]), &strings(&["army-checks"])).unwrap();
        assert_eq!(
            lines(&ALL, chosen),
            ["speed-ups left to the game: pops-polling, drawing-thread, modifier-sums", "speed-ups checked against the game: army-checks"]
        );
    }

    #[test]
    fn the_example_dev_toml_names_every_speed_up_and_group() {
        let example = include_str!("../../dev.toml.example");
        for s in ALL {
            assert!(example.contains(s.name), "{}", s.name);
            assert!(example.contains(&format!("(group \"{}\")", s.group)) || example.contains(&format!("(\"{}\")", s.group)), "{}", s.group);
        }
    }

    #[cfg(feature = "dev")]
    #[test]
    fn a_comparison_is_counted_and_a_difference_too() {
        use crate::native::speed_up::{compared, differences};
        let _ = speed_up::init(&ALL, &[], &[]);
        let speed_up = &crate::speed::saving::sum::SPEED_UP;
        let (checked, differed) = differences(speed_up);
        compared(speed_up, true, || unreachable!("an agreement says nothing"));
        compared(speed_up, false, || "the sum".into());
        assert_eq!(differences(speed_up), (checked + 2, differed + 1));
    }
}
