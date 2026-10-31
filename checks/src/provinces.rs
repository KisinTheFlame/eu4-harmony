//! Provinces.
use crate::{Refusal, Status};

/// The refusal of a number that names no province.
pub fn not_found(id: u32) -> Refusal {
    Refusal::new(Status::NotFound, format!("no province {id}"))
}
