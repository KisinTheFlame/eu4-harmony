//! Diplomacy. A country is named as the game names it, by its tag.
use crate::{Refusal, Status};
use std::fmt::Display;

/// The refusal of the player's own country where diplomacy takes another.
pub fn own_country() -> Refusal {
    Refusal::new(Status::InvalidArgument, "the player's own country")
}

/// The refusal of an empty casus belli; `none` is how a declaration without
/// one is named.
pub fn no_casus_belli(none: &str) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("no casus belli: name one by its key, or {none}"))
}

/// The refusal of a war goal the game does not offer the player under
/// `casus_belli` against `enemy`.
pub fn no_such_war_goal(casus_belli: &str, enemy: impl Display) -> Refusal {
    Refusal::new(Status::Rejected, format!("the game offers no such war goal under {casus_belli} against {enemy}; diplomacy.war_goals lists those it offers"))
}

/// The refusal of a country to call to arms that the declare war window
/// does not list among the allies.
pub fn not_listed(ally: impl Display) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("{ally} is not among the allies the declare war window lists"))
}

/// The refusal of an ally named to call to arms that the declare war window
/// does not let the player call, with its check's `reasons`, one a line.
pub fn not_callable(ally: impl Display, reasons: &[String]) -> Refusal {
    let call = format!("the declare war window does not let the player call {ally} to arms");
    Refusal::new(Status::Rejected, if reasons.is_empty() { format!("{call}, and gives no reason") } else { format!("{call}:\n{}", reasons.join("\n")) })
}

/// The refusal of a ticket that names no diplomatic action kept.
pub fn not_kept(ticket: u64) -> Refusal {
    Refusal::new(Status::NotFound, format!("no diplomatic action {ticket} is kept"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ally_not_callable_gives_the_window_s_reasons() {
        assert_eq!(
            not_callable("CND", &["CND would not accept".into()]).message,
            "the declare war window does not let the player call CND to arms:\nCND would not accept"
        );
        assert_eq!(not_callable("DUL", &[]).message, "the declare war window does not let the player call DUL to arms, and gives no reason");
    }
}
