//! The events remote control keeps: the ones extensions hear, numbered, the
//! latest 10,000 of them.
use crate::{ArmyId, Date};
use serde::{Deserialize, Serialize};

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
    anywhere Read "" {
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

/// Something that happened in the game, as extensions hear it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A campaign was loaded or started.
    Session { session: u64 },
    /// A new day, once visibility reflects it; with the game running fast,
    /// days in between may be left out.
    Day { date: Date },
    /// The player gave one of their armies an order, by hand or through
    /// remote control.
    ArmyOrdered { army: ArmyId },
    /// One of the player's armies lost a battle.
    ArmyDefeated { army: ArmyId },
}
