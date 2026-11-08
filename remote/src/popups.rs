//! Pop-ups (GLOSSARY): the windows extensions show the player with a
//! title, a text and buttons, and answering one as the player does, by
//! pressing one of its buttons (docs/extension-windows.md).
use serde::{Deserialize, Serialize};

group! {
    /// The extensions' pop-ups open in the game.
    "popup": PopupRequest, PopupAnswer;

    /// The pop-ups open, oldest first, each with the mod that showed it and
    /// its words in the player language.
    world List "list" {} -> Vec<Popup>, "the pop-ups: popup, mod, title, text, and the buttons' words in order";
    /// Presses one of a pop-up's buttons, as the player does: the pop-up
    /// closes, and the extension that showed it has its answer at the next
    /// frame.
    world Answer "answer" {
        /// The pop-up's number, from popup.list.
        popup: u32,
        /// The button's number, from 1, in popup.list's order.
        button: u32,
    } -> (), "nothing";
}

/// A pop-up open in the game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Popup {
    /// Names the pop-up while it is open; a later one has another.
    pub popup: u32,
    /// The id of the mod whose extension showed it.
    #[serde(rename = "mod")]
    pub mod_id: String,
    pub title: String,
    pub text: String,
    /// The buttons' words, in the order they stand.
    pub buttons: Vec<String>,
}
