//! The game being played.
use crate::{CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The game being played.
    "session": SessionRequest, SessionAnswer;

    /// The session: its number, the country remote control acts as, whether
    /// the player observes, and the date. Whether the game is paused is
    /// time's; which process answers is menu's.
    world Get "get" {} -> Session, "the session: number (which grows when another game is loaded or started, and starts again in a new process), player, the country acted as (the one the player plays, or in observe mode the country in view; null with none in view), observing, whether the player observes, and date";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Session {
    /// Increases whenever another game is loaded or started; numbered from 1
    /// again in a new process (menu's).
    pub number: u64,
    /// The country acted as: the one the player plays, or in observe mode
    /// the country in view; none while none is in view (GLOSSARY: Observe
    /// mode, Country in view).
    pub player: Option<CountryTag>,
    /// The player observes: plays no country, the AI playing them all.
    pub observing: bool,
    pub date: Date,
}
