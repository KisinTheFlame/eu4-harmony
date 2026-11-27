//! Provinces.
use crate::{Code, Error};

/// The refusal of a number that names no province.
pub fn not_found(id: u32) -> Error {
    Error::new(Code::NotFound, format!("no province {id}"))
}

/// The refusal of a province the player has not discovered, which the
/// player's interface shows as terra incognita.
pub fn not_discovered(id: u32) -> Error {
    Error::new(Code::NotFound, format!("the player has not discovered province {id}"))
}
