//! Countries.
use crate::{Refusal, Status};

/// The refusal of a tag that names no country.
pub fn not_found(tag: &str) -> Refusal {
    Refusal::new(Status::NotFound, format!("no country {tag}"))
}
