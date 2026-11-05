//! What every panel's action refuses with when the game does not let the
//! player do it (docs/player-actions.md): the game's reasons, one a line.
use crate::{Refusal, Status};

/// The refusal of an action the game does not let the player do now, with
/// the game's `reasons`, one a line; `doing` names the action, as "raise
/// stability", for when the game gives none.
pub fn not_allowed(doing: &str, reasons: &[String]) -> Refusal {
    if reasons.is_empty() {
        Refusal::new(Status::Rejected, format!("the game does not let the player {doing} now, and gives no reason"))
    } else {
        Refusal::new(Status::Rejected, reasons.join("\n"))
    }
}

/// The refusal of an action whose command the game's own check refused after
/// the framework's checks let it through.
pub fn command_refused(doing: &str) -> Refusal {
    Refusal::new(Status::Rejected, format!("the game's command refuses to {doing}, and gives no reason"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_not_allowed_gives_the_game_s_reasons_or_says_it_gives_none() {
        let reasons = ["稳定度已达上限。".to_string(), "No power".to_string()];
        assert_eq!(not_allowed("raise stability", &reasons).message, "稳定度已达上限。\nNo power");
        assert_eq!(not_allowed("raise stability", &[]).message, "the game does not let the player raise stability now, and gives no reason");
        assert_eq!(command_refused("raise stability").status, Status::Rejected);
    }
}
