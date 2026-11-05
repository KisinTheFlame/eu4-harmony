//! Texts: the player language. A mod's own texts are only an extension's.
use crate::game::{self};
use eu4_harmony_remote::texts::{TextAnswer, TextRequest};

layer! {
    run TextRequest => TextAnswer;
    Language => fn language() -> &'static str = game::texts::language;
}
