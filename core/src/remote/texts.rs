//! Texts: the player language. A mod's own texts are only an extension's.
use crate::game::{self};
use eu4_harmony_remote::{
    texts::{TextAnswer, TextRequest},
    Error,
};

layer! {
    fn language() -> &'static str = game::texts::language;
}

pub fn run(request: TextRequest, game: &mut (impl Layer + ?Sized)) -> Result<TextAnswer, Error> {
    match request {
        TextRequest::Language {} => Ok(TextAnswer::Language(game.language()?.to_string())),
    }
}
