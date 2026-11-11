//! Countries.
use crate::{Refusal, Status};

/// The refusal of a tag that names no country.
pub fn not_found(tag: &str) -> Refusal {
    Refusal::new(Status::NotFound, format!("no country {tag}"))
}

/// The refusal of a country's opinion asked of itself, for which the game
/// has no opinion to show ("We love ourselves!"); none for two countries.
pub fn of_itself(of: &str, toward: &str) -> Option<Refusal> {
    (of == toward).then(|| Refusal::new(Status::InvalidArgument, format!("{of}'s opinion of itself: of and toward are one country")))
}
