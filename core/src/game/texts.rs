//! Texts: the player language, and a mod's own localisation texts, which only
//! extensions have.
use super::{Caller, GResult};
use crate::{lang, loc};

/// The player language's Paradox name, for example `english` or `simp_chinese`.
pub fn language(_: Caller) -> GResult<&'static str> {
    Ok(lang::player().id())
}

/// The caller's localisation text for `key` (line breaks as `\n`), in the
/// player language or else in English.
pub fn text(caller: Caller, key: &str) -> GResult<String> {
    let id = caller.mod_id("localisation texts")?;
    match loc::mod_text(&id, key) {
        Some(text) => Ok(text),
        None => Err(eu4_harmony_checks::texts::not_found(key).into()),
    }
}
