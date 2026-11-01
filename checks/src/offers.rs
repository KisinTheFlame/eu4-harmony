//! Offers: other countries' proposals waiting for the player's answer.
use crate::{Refusal, Status};

/// The refusal of a number that names no offer waiting.
pub fn not_found(offer: u32) -> Refusal {
    Refusal::new(Status::NotFound, format!("no offer {offer}"))
}
