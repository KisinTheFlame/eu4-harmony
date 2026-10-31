//! Recruiting.
use crate::{Refusal, Status};

/// The refusal of a kind, by its key (`infantry`, `heavy_ship`), the player
/// has no unit type of.
pub fn no_unit_type(kind: &str) -> Refusal {
    Refusal::new(Status::Rejected, format!("the player has no unit type of the kind {kind}"))
}

/// The refusal of a recruit the game does not let the player make there now,
/// with the game's `reasons`, one a line.
pub fn not_allowed(reasons: &[String]) -> Refusal {
    if reasons.is_empty() {
        Refusal::new(Status::Rejected, "the game does not let the player recruit it there, and gives no reason")
    } else {
        Refusal::new(Status::Rejected, reasons.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recruit_not_allowed_gives_the_game_s_reasons() {
        let reasons = ["君士坦丁堡不是我们的领土".to_string(), "No manpower".to_string()];
        assert_eq!(not_allowed(&reasons).message, "君士坦丁堡不是我们的领土\nNo manpower");
        assert_eq!(not_allowed(&[]).message, "the game does not let the player recruit it there, and gives no reason");
    }
}
