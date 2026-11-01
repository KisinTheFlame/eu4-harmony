//! Texts.
use crate::{Refusal, Status};

/// The refusal of a key the mod's localisation has no text for.
pub fn not_found(key: &str) -> Refusal {
    Refusal::new(Status::NotFound, format!("no localisation text for {key}"))
}
