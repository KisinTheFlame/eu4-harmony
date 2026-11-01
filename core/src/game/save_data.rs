//! Extension save data: a mod's own data kept in the current save (ADR
//! 0020). Only extensions have it.
use super::{fail, Caller, GResult, Status};
use crate::saves;
use serde_json::Value;

const DATA: &str = "extension save data";

/// The caller's data for the current save; None when it has none.
pub fn get(caller: Caller) -> GResult<Option<Value>> {
    Ok(saves::get(&caller.mod_id(DATA)?))
}

/// Replaces the caller's data, written into the next save; None removes it.
/// More than 16 MiB of JSON is refused.
pub fn set(caller: Caller, value: Option<Value>) -> GResult<()> {
    saves::set(&caller.mod_id(DATA)?, value).or_else(|e| fail(Status::InvalidArgument, e))
}
