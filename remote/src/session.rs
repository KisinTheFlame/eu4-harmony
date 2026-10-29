//! The campaign being played.
use crate::{CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The campaign being played.
    "session": SessionRequest, SessionAnswer;

    /// The session, the date, whether the game is paused and the player's
    /// country, and which process answers.
    game Get "" {} -> Session, "the session: process (as menu answers it), session (its number, which grows when another game is loaded or started), date, paused, visibility_current (whether fog of war reflects the date yet: army.in_province waits for it) and player, the player's country";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// When the game process answering started, in milliseconds since 1970
    /// (as menu answers it); sessions are numbered again in a new process.
    pub process: u64,
    /// Increases whenever a different campaign is loaded or started.
    pub session: u64,
    pub date: Date,
    pub paused: bool,
    /// Visibility (fog of war) reflects the current date.
    pub visibility_current: bool,
    pub player: CountryTag,
}
