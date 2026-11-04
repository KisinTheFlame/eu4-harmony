//! Peace, the transfer of occupation and calls to arms: the refusals the
//! framework words itself. The game's own reasons are passed through as its
//! tooltips give them, one a line.
use crate::{Refusal, Status};
use std::fmt::Display;

/// The refusal of a country the player is not at war with, where the peace
/// view needs one they are.
pub fn not_at_war(enemy: impl Display) -> Refusal {
    Refusal::new(Status::Rejected, format!("the player is not at war with {enemy}"))
}

/// The refusal of a term the peace view does not offer: `term` as the
/// request names it.
pub fn no_such_term(term: impl Display) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("the peace view offers no term {term}; peace.terms lists those it offers"))
}

/// The refusal of the same term given twice.
pub fn term_twice(term: impl Display) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("the term {term} is given twice"))
}

/// The refusal of a term the peace view does not let the player add, with
/// its tooltip's `reasons`, one a line.
pub fn term_refused(term: impl Display, reasons: &[String]) -> Refusal {
    let refused = format!("the peace view does not let the player add {term}");
    Refusal::new(Status::Rejected, if reasons.is_empty() { format!("{refused}, and gives no reason") } else { format!("{refused}:\n{}", reasons.join("\n")) })
}

/// The refusal of an amount of gold the peace view does not let the player
/// set: below nothing, or above `most`, in ducats.
pub fn gold_out_of_range(amount: f64, most: f64) -> Refusal {
    Refusal::new(Status::Rejected, format!("the peace view lets the player set from 0 to {most:.1} ducats, not {amount}"))
}

/// The refusal of an ally the transfer occupation window does not list for
/// a province.
pub fn not_a_receiver(ally: impl Display, province: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("the transfer occupation window does not list {ally} to take province {province}"))
}

/// The refusal of a call to arms the diplomacy panel does not offer.
pub fn no_call_to_arms(ally: impl Display) -> Refusal {
    Refusal::new(Status::Rejected, format!("the diplomacy panel offers no call to arms to {ally}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_term_refused_gives_the_view_s_reasons() {
        assert_eq!(
            term_refused("humiliate", &["Already humiliated".into()]).message,
            "the peace view does not let the player add humiliate:\nAlready humiliated"
        );
        assert_eq!(term_refused("humiliate", &[]).message, "the peace view does not let the player add humiliate, and gives no reason");
        assert_eq!(no_such_term("demand_provinces 236").status, Status::InvalidArgument);
        assert_eq!(gold_out_of_range(500.0, 123.44).message, "the peace view lets the player set from 0 to 123.4 ducats, not 500");
    }
}
