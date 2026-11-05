//! Province administration and buildings: the province view's actions.
use crate::{Refusal, Status};

/// The refusal of an action the game does not let the player take now, such
/// as `develop` or `build`, with the game's `reasons`, one a line.
pub fn not_allowed(action: &str, reasons: &[String]) -> Refusal {
    if reasons.is_empty() {
        Refusal::new(Status::Rejected, format!("the game does not let the player {action} there now, and gives no reason"))
    } else {
        Refusal::new(Status::Rejected, reasons.join("\n"))
    }
}

/// The refusal of an action on a province the player does not own.
pub fn not_owned(province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("the player does not own province {province}"))
}

/// The refusal of an edict for a province in no state of the player's.
pub fn not_in_a_state(province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("province {province} is in no state of the player's"))
}

/// The refusal of cancelling an edict where none is set.
pub fn no_edict_set(province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("no edict is set in the state of province {province}"))
}

/// The refusal of changing a province's culture to the primary culture it
/// has.
pub fn primary_culture_already(province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("the culture of province {province} is the player's primary culture already"))
}

/// The refusal of a content key that names no edict.
pub fn no_such_edict(key: &str) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("no edict {key}"))
}

/// The refusal of a content key that names no building.
pub fn no_such_building(key: &str) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("no building {key}"))
}

/// The refusal of destroying a building that does not stand in the province.
pub fn building_absent(key: &str, province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("there is no {key} in province {province}"))
}

/// The refusal of cancelling what the player is not doing in a province:
/// `what` is "building", "coring" or "changing the culture of".
pub fn not_doing(what: &str, province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("the player is not {what} province {province}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_gives_the_game_s_reasons_one_a_line() {
        let reasons = ["没有足够的行政点数".to_string(), "Under siege".to_string()];
        assert_eq!(not_allowed("develop", &reasons).message, "没有足够的行政点数\nUnder siege");
        assert_eq!(not_allowed("develop", &[]).message, "the game does not let the player develop there now, and gives no reason");
        assert_eq!(not_allowed("build", &[]).status, Status::Rejected);
    }

    #[test]
    fn unknown_keys_are_invalid_arguments() {
        assert_eq!(no_such_edict("edict_x").status, Status::InvalidArgument);
        assert_eq!(no_such_building("castle").message, "no building castle");
        assert_eq!(not_doing("coring", 151).message, "the player is not coring province 151");
    }
}
