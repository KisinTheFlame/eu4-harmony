//! The game being played.
use crate::{CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The game being played.
    "session": SessionRequest, SessionAnswer;

    /// The session: its number, the player's country and the date. Whether
    /// the game is paused is time's; which process answers is menu's.
    game Get "get" {} -> Session, "the session: number (which grows when another game is loaded or started, and starts again in a new process), player, the player's country, and date";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Increases whenever another game is loaded or started; numbered from 1
    /// again in a new process (menu's).
    pub number: u64,
    pub player: CountryTag,
    pub date: Date,
}
