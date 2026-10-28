//! The campaign being played.
use crate::{CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The campaign being played.
    "session": SessionRequest, SessionAnswer;

    /// The session, the date, whether the game is paused and the player's
    /// country, and which process answers.
    game Get "" {} -> Session, "the session";
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
