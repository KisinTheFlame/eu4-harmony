//! The game on the player's desktop: whether it is hidden, whether it plays
//! sound, and showing and hiding its window in a hidden start
//! (`-harmony_hidden`) or a test game. Remote control only.
use serde::{Deserialize, Serialize};

group! {
    /// The game on the player's desktop: window visibility, audio mute, and showing/hiding the window.
    "desktop": DesktopRequest, DesktopAnswer;

    /// Whether the game's window is hidden, whether the game was started
    /// hidden, and whether it plays sound.
    remote_anywhere Get "get" {} -> Desktop, "hidden (the window is hidden now), hidden_start (the game was started with -harmony_hidden) and sound (it plays sound: false when started with -harmony_mute)";
    /// Shows the window of a game started hidden or as a test game, without
    /// taking the focus; from then on the player's mouse and keyboard reach it
    /// as usual. Refused (invalid_argument) for an ordinary shown start.
    remote_anywhere Show "show" {} -> Desktop, "as desktop.get answers it, afterwards";
    /// Hides the window of a non-fullscreen game started hidden or as a test
    /// game, remote control holding the cursor as at a hidden start. Refused
    /// (invalid_argument) for an ordinary shown start or a fullscreen window.
    remote_anywhere Hide "hide" {} -> Desktop, "as desktop.get answers it, afterwards";
}

/// The game on the player's desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Desktop {
    /// The game's window is hidden.
    pub hidden: bool,
    /// The game was started with `-harmony_hidden`.
    pub hidden_start: bool,
    /// The game plays sound (`false` when started with `-harmony_mute`); chosen
    /// as it starts, it stays so until it ends.
    pub sound: bool,
}
