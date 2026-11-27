//! Countries.
use crate::{Code, Error};

/// The refusal of a tag that names no country.
pub fn not_found(tag: &str) -> Error {
    Error::new(Code::NotFound, format!("no country {tag}"))
}

/// The refusal of a country's opinion asked of itself, for which the game
/// has no opinion to show ("We love ourselves!"); none for two countries.
pub fn of_itself(of: &str, toward: &str) -> Option<Error> {
    (of == toward).then(|| Error::new(Code::InvalidArgument, format!("{of}'s opinion of itself: of and toward are one country")))
}
