//! Loans.
use crate::{Refusal, Status};

/// The refusal of a loan ID that is none of the player's loans.
pub fn not_found(loan: u32) -> Refusal {
    Refusal::new(Status::NotFound, format!("the player has no loan {loan}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_loan_the_player_does_not_have_is_not_found() {
        let refusal = super::not_found(7);
        assert_eq!((refusal.status, refusal.message.as_str()), (crate::Status::NotFound, "the player has no loan 7"));
    }
}
