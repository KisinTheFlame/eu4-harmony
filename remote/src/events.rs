//! The events remote control keeps: the ones extensions hear, numbered, the
//! latest [`KEPT`] of them.
use crate::{ArmyId, CountryList, CountryTag, Date, ProvinceId, Words};
use serde::{Deserialize, Serialize};

/// How many events are kept.
pub const KEPT: usize = 10_000;
/// The longest a read may wait, in milliseconds, as `wait_ms` states it.
pub const LONGEST_WAIT: u32 = 60_000;

group! {
    /// The events remote control keeps.
    "events": EventsRequest, EventsAnswer;

    /// The events numbered above `since`, oldest first, of the kinds asked
    /// for and naming the countries asked for. With `wait_ms`, when there is
    /// none yet, the answer waits for the first one, up to that
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
        /// Only events of these kinds (`day`, `month`, …); every kind when
        /// absent. A kind no extension listens to is worked out only once a
        /// remote client has read it, or every kind, in this game process.
        kinds: Option<Words>,
        /// Only events that name one of these countries, and those that
        /// name none (`day`); every event when absent.
        countries: Option<CountryList>,
    } -> Events, "the process answering, the events, the number of the last one, and whether some after since are gone unread: dropped, or since is from before the game started again (another process, or beyond the last one)";
}

/// Events read from remote control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Numbered {
    pub number: u64,
    #[serde(flatten)]
    pub event: Event,
}

/// Why a session is left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum SessionEnd {
    /// For the main menu.
    Return,
    /// For a save loaded from the session.
    Load,
    /// For a new game started from the session.
    NewGame,
    /// The player's country changed (the game console's `tag`, say): the
    /// next session, of the same game, begins at once.
    Country,
}

/// The kind of an [`Event`], one for each of its variants, which `kinds`,
/// [`EVENTS`] and an extension's handler (`on_<kind>`) name by
/// [`Kind::name`]. Whoever works an event out only for those who listen
/// asks by it, so that a kind misspelled does not compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Session,
    SessionEnded,
    Day,
    Month,
    Year,
    ArmyOrdered,
    ArmyDefeated,
    WarDeclared,
    PeaceMade,
    ProvinceOwnerChanged,
    BuildingFinished,
    Battle,
}

impl Kind {
    /// Its name, as the event's `event` field gives it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::SessionEnded => "session_ended",
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
            Self::ArmyOrdered => "army_ordered",
            Self::ArmyDefeated => "army_defeated",
            Self::WarDeclared => "war_declared",
            Self::PeaceMade => "peace_made",
            Self::ProvinceOwnerChanged => "province_owner_changed",
            Self::BuildingFinished => "building_finished",
            Self::Battle => "battle",
        }
    }

    /// The kind named `name`; none for a name no kind has.
    pub fn named(name: &str) -> Option<Self> {
        EVENTS.iter().map(|(kind, _)| *kind).find(|kind| kind.name() == name)
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(self.name())
    }
}

/// By its name.
impl Serialize for Kind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// Every kind of event, with what it carries and when it comes.
pub const EVENTS: &[(Kind, &str)] = &[
    (Kind::Session, "a session began: a save was loaded or a game started; session, its number, as session.get answers it"),
    (Kind::SessionEnded, "the session is being left, while its state is still there to read; reason: return (for the main menu), load (for a save), new_game, or country (the player's country changed)"),
    (Kind::Day, "a new day, once visibility reflects it; date. With the game running fast, days in between may be left out"),
    (Kind::Month, "the first day heard in a new month; date, which with the game running fast may be past the 1st"),
    (Kind::Year, "the first day heard in a new year, after month; date"),
    (Kind::ArmyOrdered, "the player gave one of their armies an order, by hand or through remote control; army"),
    (Kind::ArmyDefeated, "one of the player's armies lost a battle; army"),
    (Kind::WarDeclared, "a war began between any two countries; war, its name; attacker and defender, the leaders of its sides"),
    (
        Kind::PeaceMade,
        "countries left a war, by a peace (or ceasing to be); war, its name; attackers and defenders, those that left on each side; ended, whether the war is over",
    ),
    (Kind::ProvinceOwnerChanged, "a province changed owner; province; from and to, the old and the new owner, null for none"),
    (Kind::BuildingFinished, "a building was finished in a province, any country's; province, building (its content key), country (the province's owner)"),
    (
        Kind::Battle,
        "a land or naval battle was decided: in the player's view one the player sees or takes part in, in the full view every one; province, naval, attackers, defenders (the countries on each side, the leading one first), winner (attackers or defenders), attacker_losses, defender_losses (men in a land battle, ships in a naval one)",
    ),
];

/// Something that happened in the game, as extensions hear it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A session began: a save was loaded or a game started. Its number, as
    /// session.get answers it (`number` there; here the event's own number
    /// is `number`).
    Session { session: u64 },
    /// The session is being left, while its state is still there to read.
    SessionEnded { reason: SessionEnd },
    /// A new day, once visibility reflects it; with the game running fast,
    /// days in between may be left out.
    Day { date: Date },
    /// The first day heard in a new month; with the game running fast it
    /// may be past the 1st.
    Month { date: Date },
    /// The first day heard in a new year, after its month.
    Year { date: Date },
    /// The player gave one of their armies an order, by hand or through
    /// remote control.
    ArmyOrdered { army: ArmyId },
    /// One of the player's armies lost a battle.
    ArmyDefeated { army: ArmyId },
    /// A war began, between any two countries: its name, as the game shows
    /// it, and the leaders of its sides.
    WarDeclared { war: String, attacker: CountryTag, defender: CountryTag },
    /// Countries left a war on a day: by a peace, the whole war's or a
    /// separate one, or by ceasing to be. Those on each side that left, and
    /// whether the war is over.
    PeaceMade { war: String, attackers: Vec<CountryTag>, defenders: Vec<CountryTag>, ended: bool },
    /// A province changed owner: the old and the new owner, null for none.
    ProvinceOwnerChanged { province: ProvinceId, from: Option<CountryTag>, to: Option<CountryTag> },
    /// A building was finished in a province, any country's.
    BuildingFinished {
        province: ProvinceId,
        /// Its content key.
        building: String,
        /// The province's owner.
        country: CountryTag,
    },
    /// A land or naval battle was decided: in the player's view one the
    /// player sees or takes part in, in the full view every one.
    Battle {
        province: ProvinceId,
        naval: bool,
        /// The countries on each side, the leading one first.
        attackers: Vec<CountryTag>,
        defenders: Vec<CountryTag>,
        winner: Winner,
        /// What each side lost: men in a land battle, ships in a naval
        /// one.
        attacker_losses: i64,
        defender_losses: i64,
    },
}

/// Which side won a battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Winner {
    Attackers,
    Defenders,
}

impl Event {
    /// Its kind.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Session { .. } => Kind::Session,
            Self::SessionEnded { .. } => Kind::SessionEnded,
            Self::Day { .. } => Kind::Day,
            Self::Month { .. } => Kind::Month,
            Self::Year { .. } => Kind::Year,
            Self::ArmyOrdered { .. } => Kind::ArmyOrdered,
            Self::ArmyDefeated { .. } => Kind::ArmyDefeated,
            Self::WarDeclared { .. } => Kind::WarDeclared,
            Self::PeaceMade { .. } => Kind::PeaceMade,
            Self::ProvinceOwnerChanged { .. } => Kind::ProvinceOwnerChanged,
            Self::BuildingFinished { .. } => Kind::BuildingFinished,
            Self::Battle { .. } => Kind::Battle,
        }
    }

    /// The countries it names; none for one about the game as a whole or
    /// about the player's own armies.
    pub fn countries(&self) -> Vec<CountryTag> {
        match self {
            Self::WarDeclared { attacker, defender, .. } => vec![*attacker, *defender],
            Self::PeaceMade { attackers, defenders, .. } => attackers.iter().chain(defenders).copied().collect(),
            Self::ProvinceOwnerChanged { from, to, .. } => from.iter().chain(to).copied().collect(),
            Self::BuildingFinished { country, .. } => vec![*country],
            Self::Battle { attackers, defenders, .. } => attackers.iter().chain(defenders).copied().collect(),
            _ => Vec::new(),
        }
    }
}

/// Which events a read asks for: those of `kinds`, and among them those that
/// name one of `countries` or none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub kinds: Option<Vec<String>>,
    pub countries: Option<Vec<CountryTag>>,
}

impl Filter {
    pub fn new(kinds: Option<Words>, countries: Option<CountryList>) -> Self {
        Self { kinds: kinds.map(|k| k.0), countries: countries.map(|c| c.0) }
    }

    /// Whether `event` is one asked for.
    pub fn takes(&self, event: &Event) -> bool {
        let kind = self.kinds.as_ref().is_none_or(|kinds| kinds.iter().any(|k| k == event.kind().name()));
        let named = event.countries();
        let country = named.is_empty() || self.countries.as_ref().is_none_or(|countries| named.iter().any(|c| countries.contains(c)));
        kind && country
    }

    /// A kind it names that is no event's.
    pub fn unknown_kind(&self) -> Option<&str> {
        self.kinds.as_ref()?.iter().find(|k| Kind::named(k).is_none()).map(String::as_str)
    }
}
