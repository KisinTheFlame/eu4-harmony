//! Texts: the player language. A mod's own texts are only an extension's.
use crate::game::{self};
use eu4_harmony_remote::texts::{TextAnswer, TextRequest};
use eu4_harmony_remote::texts::args;

layer! {
    run TextRequest => TextAnswer;
    Language => fn language -> &'static str = caller game::texts::language;
}
