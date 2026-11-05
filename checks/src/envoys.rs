//! Missionaries, colonists, merchants, trade companies and centers of trade:
//! the actions of part A8 of docs/player-actions.md.
use crate::{Refusal, Status};

/// The refusal of an action the game does not let the player take now
/// (`what`, such as "send a missionary there"), with the game's `reasons`,
/// one a line.
pub fn not_allowed(what: &str, reasons: &[String]) -> Refusal {
    if reasons.is_empty() {
        Refusal::new(Status::Rejected, format!("the game does not let the player {what} now, and gives no reason"))
    } else {
        Refusal::new(Status::Rejected, reasons.join("\n"))
    }
}

/// The refusal of recalling an envoy of a `kind` ("missionary") from where
/// the player has none.
pub fn none_there(kind: &str) -> Refusal {
    Refusal::new(Status::Rejected, format!("the player has no {kind} there"))
}

/// The refusal of recalling an envoy of a `kind` ("merchant") that is
/// still on its way.
pub fn on_its_way(kind: &str) -> Refusal {
    Refusal::new(Status::Rejected, format!("the player's {kind} is still on its way there"))
}

/// The refusal of changing a center of trade in a province that has none.
pub fn no_center_of_trade() -> Refusal {
    Refusal::new(Status::Rejected, "the province has no center of trade")
}

/// The refusal of raising the automatic missionaries when the game already
/// sends all of the player's `missionaries`.
pub fn all_automatic(missionaries: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("the game already sends all of the player's {missionaries} missionaries by itself"))
}

/// The refusal of lowering the automatic missionaries when the game sends
/// none.
pub fn none_automatic() -> Refusal {
    Refusal::new(Status::Rejected, "the game sends none of the player's missionaries by itself")
}

/// The refusal of a content key `key` that names no `kind` ("trade company
/// investment", "trade node").
pub fn unknown(kind: &str, key: &str) -> Refusal {
    Refusal::new(Status::NotFound, format!("no {kind} is named {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_gives_the_game_s_reasons_or_says_it_gives_none() {
        let reasons = ["没有可用的传教士".to_string(), "Under siege".to_string()];
        assert_eq!(not_allowed("send a missionary there", &reasons).message, "没有可用的传教士\nUnder siege");
        assert_eq!(not_allowed("send a missionary there", &[]).message, "the game does not let the player send a missionary there now, and gives no reason");
    }

    #[test]
    fn the_framework_s_own_refusals_name_what_is_missing() {
        assert_eq!(none_there("colonist").message, "the player has no colonist there");
        assert_eq!(all_automatic(3).message, "the game already sends all of the player's 3 missionaries by itself");
        assert_eq!(unknown("trade node", "nowhere").status, Status::NotFound);
    }
}
