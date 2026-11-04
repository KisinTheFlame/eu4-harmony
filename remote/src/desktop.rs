//! The game on the player's desktop: whether it is hidden, and showing and
//! hiding it again, for a game started hidden (`-harmony_hidden`). Remote
//! control only.
use serde::{Deserialize, Serialize};

group! {
    /// The game on the player's desktop: a hidden start, showing and hiding it.
    "desktop": DesktopRequest, DesktopAnswer;

    /// Whether the game's window is hidden, whether the game was started
    /// hidden, and whether it plays sound.
    remote_anywhere Get "get" {} -> Desktop, "hidden (the window is hidden now), hidden_start (the game was started with -harmony_hidden) and sound (it plays sound: a hidden start plays none without -harmony_sound)";
    /// Shows the window of a game started hidden, as the player would have
    /// it, without taking the focus; from then on the player's mouse and
    /// keyboard reach it as usual. Refused (invalid_argument) for a game not
    /// started hidden.
    remote_anywhere Show "show" {} -> Desktop, "as desktop.get answers it, afterwards";
    /// Hides the window of a game started hidden again, remote control
    /// holding the cursor as at the start. Refused (invalid_argument) for a
    /// game not started hidden.
    remote_anywhere Hide "hide" {} -> Desktop, "as desktop.get answers it, afterwards";
}

/// The game on the player's desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Desktop {
    /// The game's window is hidden.
    pub hidden: bool,
    /// The game was started hidden.
    pub hidden_start: bool,
    /// The game plays sound; chosen as it starts, it stays so until it ends.
    pub sound: bool,
}
