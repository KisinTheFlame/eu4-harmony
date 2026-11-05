//! Government: ideas, policies, reforms, the rank, estates, parliament,
//! decisions and missions, the refusals the framework makes itself.
use crate::{Refusal, Status};

/// The refusal of `what` (such as "taking this idea group") the game does not
/// allow now, with the game's `reasons`, one a line.
pub fn not_allowed(reasons: &[String], what: &str) -> Refusal {
    if reasons.is_empty() {
        Refusal::new(Status::Rejected, format!("the game does not allow {what} now, and gives no reason"))
    } else {
        Refusal::new(Status::Rejected, reasons.join("\n"))
    }
}

/// The refusal of a content key that names nothing of `kind` (such as "idea
/// group") the player has or is offered.
pub fn not_found(kind: &str, key: &str) -> Refusal {
    Refusal::new(Status::NotFound, format!("no {kind} {key:?} for the player"))
}

/// The refusal of an action on something of the player's in a state it is
/// not in: `why`, such as "the player has not taken the idea group
/// defensive_ideas".
pub fn not_now(why: impl Into<String>) -> Refusal {
    Refusal::new(Status::Rejected, why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_gives_the_game_s_reasons_or_says_it_gives_none() {
        let reasons = ["拥有至少 3 个省份".to_string(), "Is at peace".to_string()];
        assert_eq!(not_allowed(&reasons, "taking it").message, "拥有至少 3 个省份\nIs at peace");
        assert_eq!(not_allowed(&[], "taking it").message, "the game does not allow taking it now, and gives no reason");
        assert_eq!(not_allowed(&[], "x").status, Status::Rejected);
    }

    #[test]
    fn an_unknown_key_is_not_found() {
        let refusal = not_found("idea group", "flying_ideas");
        assert_eq!((refusal.status, refusal.message.as_str()), (Status::NotFound, "no idea group \"flying_ideas\" for the player"));
    }
}
