//! Pop-ups (GLOSSARY): the windows extensions show the player with a
//! title, a text and buttons, and answering one as the player does, by
//! pressing one of its buttons (docs/plans/extension-windows.md).
use serde::{Deserialize, Serialize};

group! {
    /// The extensions' pop-ups open in the game.
    "popup": PopupRequest, PopupAnswer;

    /// The pop-ups open, oldest first, each with the mod that showed it and
    /// its words in the player language.
    world List "list" {} -> Vec<Popup>, "the pop-ups: popup, mod, header, title, text, and the buttons' words left to right";
    /// Presses one of a pop-up's buttons, as the player does: the extension
    /// that showed it runs the button's action at the next frame, which
    /// closes the pop-up unless it says not to.
    world Answer "answer" {
        /// The pop-up's number, from popup.list.
        popup: u32,
        /// The button's number, from 1, left to right as popup.list gives
        /// them.
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
    /// The small line above its title, when it has one.
    pub header: Option<String>,
    pub title: String,
    pub text: String,
    /// The buttons' words, left to right as they stand, the game's own OK
    /// when the mod gave none.
    pub buttons: Vec<String>,
}
