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
    /// In observe mode, puts a country in view, or another in its place, as
    /// the player's Ctrl and click on one of its provinces does: the map
    /// shows its fog of war, and what acts as a country acts as it while the
    /// AI goes on playing it. The session goes on. Rejected when the player
    /// plays a country, or the country owns no province; not_found for a tag
    /// no country has.
    world View "view" {
        /// The country to put in view.
        country: CountryTag,
    } -> Session, "the session, as session.get answers it, the country in view its player";
    /// In observe mode, takes the country in view out, as the menubar's
    /// button does: the whole map shows, and what acts as a country fails
    /// with no_country. Nothing with none in view; rejected when the player
    /// plays a country.
    world LeaveView "leave_view" {} -> Session, "the session, as session.get answers it, its player null";
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
