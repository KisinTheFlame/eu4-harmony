//! Extension save data: a mod's own data kept with the current save (ADR
//! 0007). Only extensions have it.
use super::{Caller, GResult};
use crate::saves;
use serde_json::Value;

const DATA: &str = "extension save data";

/// The caller's data for the current save; None when it has none.
pub fn get(caller: Caller) -> GResult<Option<Value>> {
    Ok(saves::get(&caller.mod_id(DATA)?))
}

/// Replaces the caller's data, written alongside the next save; None removes it.
pub fn set(caller: Caller, value: Option<Value>) -> GResult<()> {
    saves::set(&caller.mod_id(DATA)?, value);
    Ok(())
}
