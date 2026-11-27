//! Texts.
use crate::{Code, Error};

/// The refusal of a key the mod's localisation has no text for.
pub fn not_found(key: &str) -> Error {
    Error::new(Code::NotFound, format!("no localisation text for {key}"))
}
