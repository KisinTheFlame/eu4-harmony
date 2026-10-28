//! The events remote control keeps: the ones extensions hear, numbered, the
//! latest few thousand of them.
use crate::{ArmyId, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The events remote control keeps.
    "events": EventsRequest, EventsAnswer;

    /// The events numbered above `since`, oldest first. With `wait_ms`, when
    /// there is none yet, the answer waits for the first one, at most that
    /// long.
    anywhere Read "" {
        /// The number of the last event already seen; 0 or absent for every
        /// event kept.
        since: Option<u64>,
        /// How many milliseconds to wait for an event, at most 60000; absent
        /// to answer at once.
        wait_ms: Option<u32>,
    } -> Events, "the events, the number of the last one, and whether some after since were dropped";
}

/// Events read from remote control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    pub events: Vec<Numbered>,
    /// The number of the latest event; 0 before the first. Read from here
    /// next time.
    pub last: u64,
    /// Events after `since` were dropped before they were read.
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
