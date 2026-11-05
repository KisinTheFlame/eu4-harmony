//! Armies and fleets, leaders, mercenaries, sieges and forts: the refusals
//! of the military panels' actions (docs/player-actions.md, A4).
use crate::{Refusal, Status};

/// The refusal of an action the game does not let the player take now
/// (`what`, such as "recruit a general"), with the game's `reasons`, one a
/// line, as the button's tooltip gives them.
pub fn not_allowed(what: &str, reasons: &[String]) -> Refusal {
    if reasons.is_empty() {
        Refusal::new(Status::Rejected, format!("the game does not let the player {what} now, and gives no reason"))
    } else {
        Refusal::new(Status::Rejected, reasons.join("\n"))
    }
}

/// The refusal of an action the unit's state makes pointless, in the
/// words `why`, as the panel shows the other button instead.
pub fn state(why: &str) -> Refusal {
    Refusal::new(Status::Rejected, why)
}

/// The refusal of a word that names none of `known`, for the argument
/// `argument`.
pub fn unknown(argument: &str, word: &str, known: &[&str]) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("{argument} {word:?} is none of {}", known.join(", ")))
}

/// The refusal of a leader the player does not have.
pub fn no_leader(id: u32) -> Refusal {
    Refusal::new(Status::NotFound, format!("the player has no leader {id}"))
}

/// The refusal of a leader given to a unit of the other kind: a general or a
/// conquistador leads an army, an admiral or an explorer a fleet.
pub fn wrong_unit(kind: &str, land: bool) -> Refusal {
    let (leads, not) = if land { ("a fleet", "an army") } else { ("an army", "a fleet") };
    Refusal::new(Status::Rejected, format!("a leader of the kind {kind} leads {leads}, not {not}"))
}

/// The refusal of dismissing a leader that still leads an army or a fleet,
/// which the game's command refuses: the leader is taken from it first.
pub fn leads_a_unit(id: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("leader {id} leads an army or a fleet; take it from there first (leader.remove)"))
}

/// The refusal of a fleet's action for an army.
pub fn not_fleet(id: u64) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("{id} is an army, not a fleet"))
}

/// The refusal of a mission given without the target it needs (`needs`,
/// such as "a trade node's number").
pub fn needs_target(mission: &str, needs: &str) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("the mission {mission} needs {needs} as its target"))
}

/// The refusal of a mercenary company the mercenary view does not list.
pub fn no_company(key: &str) -> Refusal {
    Refusal::new(Status::NotFound, format!("the mercenary view lists no company {key:?}"))
}

/// The refusal of a province with no siege for the siege view's buttons.
pub fn no_siege(province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("there is no siege in province {province}"))
}

/// The refusal of a fort action on a province with no fort of the player's.
pub fn no_fort(province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("province {province} has no fort of the player's"))
}

/// The refusal of mothballing a fort already mothballed, or reopening one
/// that is not.
pub fn fort_already(province: u32, mothballed: bool) -> Refusal {
    Refusal::new(Status::Rejected, format!("the fort in province {province} is {} already", if mothballed { "mothballed" } else { "open" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_gives_the_game_s_reasons_or_says_it_gives_none() {
        assert_eq!(not_allowed("recruit a general", &["需要 50 军事点数。".into()]).message, "需要 50 军事点数。");
        assert_eq!(not_allowed("recruit a general", &[]).message, "the game does not let the player recruit a general now, and gives no reason");
        assert_eq!(unknown("kind", "captain", &["general", "admiral"]).status, Status::InvalidArgument);
        assert_eq!(wrong_unit("general", false).message, "a leader of the kind general leads an army, not a fleet");
    }
}
