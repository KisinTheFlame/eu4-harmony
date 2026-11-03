//! Texts: the player language. A mod's own texts are its extension's, in
//! the Lua host (`text.get`).
use super::{Caller, GResult};
use crate::lang;

/// The player language's Paradox name, for example `english` or `simp_chinese`.
pub fn language(_: Caller) -> GResult<&'static str> {
    Ok(lang::player().id())
}
