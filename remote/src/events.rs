//! The events remote control keeps: the ones extensions hear, numbered, the
//! latest [`KEPT`] of them.
use crate::{ArmyId, Date};
use serde::{Deserialize, Serialize};

/// How many events are kept.
pub const KEPT: usize = 10_000;
/// The longest a read may wait, in milliseconds, as `wait_ms` states it.
pub const LONGEST_WAIT: u32 = 60_000;

group! {
    /// The events remote control keeps.
    "events": EventsRequest, EventsAnswer;

    /// The events numbered above `since`, oldest first. With `wait_ms`, when
    /// there is none yet, the answer waits for the first one, up to that
    /// long from when the game takes the request up. The game takes
    /// requests up and answers them only at its frames, and it has none at
    /// startup until the main menu shows, nor while a save loads: a request
    /// sent then is taken up only after that, and an answer due then comes
    /// late, after it, however long that takes.
    remote_anywhere Read "" {
        /// The number of the last event already seen; 0 or absent for every
        /// event kept.
        since: Option<u64>,
        /// How many milliseconds to wait for an event, at most 60000; absent
        /// to answer at once.
        wait_ms: Option<u32>,
        /// The process since came from, as an earlier answer gave it. When
        /// it is not the one answering, since is from before the game
        /// started again: every event kept is read, with missed.
        process: Option<u64>,
    } -> Events, "the process answering, the events, the number of the last one, and whether some after since are gone unread: dropped, or since is from before the game started again (another process, or beyond the last one)";
}

/// Events read from remote control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    /// When the game process answering started, in milliseconds since 1970
    /// (as menu answers it). Events are numbered from 1 again in a new
    /// process: pass it back with since.
    pub process: u64,
    pub events: Vec<Numbered>,
    /// The number of the latest event; 0 before the first. Read from here
    /// next time.
    pub last: u64,
    /// Some events after `since` are gone unread: they were dropped before
    /// this read, or `since` is from before the game started again (its
    /// process is another, or it is beyond `last`) and the events that
    /// followed it then are gone.
    pub missed: bool,
}

/// An event and its number, which increases by one with each event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Numbered {
    pub number: u64,
    #[serde(flatten)]
    pub event: Event,
}

/// Why a session is left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEnd {
    /// For the main menu.
    Return,
    /// For a save loaded from the session.
    Load,
    /// For a new game started from the session.
    NewGame,
}

/// Every event by its name, with what it carries and when it comes.
pub const EVENTS: &[(&str, &str)] = &[
    ("session", "a session began: a save was loaded or a game started; number, the session's, as session.get answers it"),
    ("session_ended", "the session is being left, while its state is still there to read; reason: return (for the main menu), load (for a save) or new_game"),
    ("day", "a new day, once visibility reflects it; date. With the game running fast, days in between may be left out"),
    ("army_ordered", "the player gave one of their armies an order, by hand or through remote control; army"),
    ("army_defeated", "one of the player's armies lost a battle; army"),
];

/// Something that happened in the game, as extensions hear it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A session began: a save was loaded or a game started. Its number, as
    /// session.get answers it.
    Session { number: u64 },
    /// The session is being left, while its state is still there to read.
    SessionEnded { reason: SessionEnd },
    /// A new day, once visibility reflects it; with the game running fast,
    /// days in between may be left out.
    Day { date: Date },
    /// The player gave one of their armies an order, by hand or through
    /// remote control.
    ArmyOrdered { army: ArmyId },
    /// One of the player's armies lost a battle.
    ArmyDefeated { army: ArmyId },
}
