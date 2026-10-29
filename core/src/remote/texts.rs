//! Texts: the player language. A mod's own texts are only an extension's.
use super::Live;
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    texts::{TextAnswer, TextRequest},
    Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn language(&mut self) -> GResult<&'static str>;
}

impl Layer for Live {
    fn language(&mut self) -> GResult<&'static str> {
        game::texts::language(Caller::Remote)
    }
}

pub fn run(request: TextRequest, game: &mut (impl Layer + ?Sized)) -> Result<TextAnswer, Error> {
    match request {
        TextRequest::Language {} => Ok(TextAnswer::Language(game.language()?.to_string())),
    }
}
