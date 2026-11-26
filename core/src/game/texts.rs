//! Texts: the player language. A mod's own texts are its extension's, in
//! the Lua host (`text.get`).
use super::{Caller, GResult};
use crate::base::lang;

/// The player language's Paradox name, for example `english` or `simp_chinese`.
pub fn language(_: Caller) -> GResult<&'static str> {
    Ok(lang::player().id())
}

/// The group's dependency points: none of its own.
pub fn points() -> Vec<crate::native::points::Point> {
    Vec::new()
}

crate::catalogue::bind! {
    eu4_harmony_remote::texts: TextRequest => TextAnswer;
    Language => caller language;
}
