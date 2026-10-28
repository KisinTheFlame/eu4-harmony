//! The interface elements a mod declares: showing, enabling and labeling
//! them. Only extensions have them.
use super::{Caller, GResult};
use crate::{ui, unicode::game_text};

const ELEMENTS: &str = "interface elements";

pub fn set_visible(caller: Caller, name: &str, visible: bool) -> GResult<()> {
    ui::set_visible(caller.extension(ELEMENTS)?, name, visible)
}

pub fn set_enabled(caller: Caller, name: &str, enabled: bool) -> GResult<()> {
    ui::set_enabled(caller.extension(ELEMENTS)?, name, enabled)
}

pub fn set_text(caller: Caller, name: &str, text: &str) -> GResult<()> {
    ui::set_text(caller.extension(ELEMENTS)?, name, game_text::from_str(text))
}

/// The tooltip of a button; an empty text removes it.
pub fn set_tooltip(caller: Caller, name: &str, text: &str) -> GResult<()> {
    ui::set_tooltip(caller.extension(ELEMENTS)?, name, game_text::from_str(text))
}
