//! Questions: the game's events waiting for the player's answer.
use crate::{Refusal, Status};

/// The refusal of a number that names no question waiting.
pub fn not_found(question: u32) -> Refusal {
    Refusal::new(Status::NotFound, format!("no question {question}"))
}

/// The refusal of an option a question does not have.
pub fn no_option(question: u32, option: u32) -> Refusal {
    Refusal::new(Status::InvalidArgument, format!("question {question} has no option {option}"))
}

/// The refusal of an option of a question the player may not choose.
pub fn not_allowed(question: u32, option: u32) -> Refusal {
    Refusal::new(Status::Rejected, format!("the player may not choose option {option} of question {question}"))
}
