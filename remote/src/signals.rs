//! The signals remote control keeps: the ones extensions hear, numbered, the
//! latest [`KEPT`] of them.
use crate::shape::{Field, Shape, Shaped};
use crate::{ArmyId, CountryList, CountryTag, Date, ProvinceId, Words};
use serde::{Deserialize, Serialize};

/// How many signals are kept.
pub const KEPT: usize = 10_000;
/// The longest a read may wait, in milliseconds, as `wait_ms` states it.
pub const LONGEST_WAIT: u32 = 60_000;

group! {
    /// The signals remote control keeps.
    "signals": SignalsRequest, SignalsAnswer;

    /// The signals numbered above `since`, oldest first, of the kinds asked
    /// for and naming the countries asked for. With `wait_ms`, when there is
    /// none yet, the answer waits for the first one, up to that
    /// long from when the game takes the request up. The game takes
    /// requests up and answers them only at its frames, and it has none at
    /// startup until the main menu shows, nor while a save loads: a request
    /// sent then is taken up only after that, and an answer due then comes
    /// late, after it, however long that takes.
    remote_anywhere Read "" {
        /// The number of the last signal already seen; 0 or absent for every
        /// signal kept.
        since: Option<u64>,
        /// How many milliseconds to wait for a signal, at most 60000; absent
        /// to answer at once.
        wait_ms: Option<u32>,
        /// The process since came from, as an earlier answer gave it. When
        /// it is not the one answering, since is from before the game
        /// started again: every signal kept is read, with missed.
        process: Option<u64>,
        /// Only signals of these kinds (`day`, `month`, …); every kind when
        /// absent. A kind no extension listens to is worked out only once a
        /// remote client has read it, or every kind, in this game process.
        kinds: Option<Words>,
        /// Only signals that name one of these countries, and those that
        /// name none (`day`); every signal when absent.
        countries: Option<CountryList>,
    } -> Signals, "the process answering, the signals, the number of the last one, and whether some after since are gone unread: dropped, or since is from before the game started again (another process, or beyond the last one)";
}

/// Signals read from remote control.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Signals {
    /// When the game process answering started, in milliseconds since 1970
    /// (as menu answers it). Signals are numbered from 1 again in a new
    /// process: pass it back with since.
    pub process: u64,
    pub signals: Vec<Numbered>,
    /// The number of the latest signal; 0 before the first. Read from here
    /// next time.
    pub last: u64,
    /// Some signals after `since` are gone unread: they were dropped before
    /// this read, or `since` is from before the game started again (its
    /// process is another, or it is beyond `last`) and the signals that
    /// followed it then are gone.
    pub missed: bool,
}

/// A signal and its number, which increases by one with each signal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Numbered {
    pub number: u64,
    #[serde(flatten)]
    pub signal: Signal,
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

/// Declares the signals from one list: [`Signal`] with a variant for each, its
/// [`Kind`] with the kind's name, [`SIGNALS`], the countries each names and
/// the handler extensions hear it with (`crate::lua::handlers`). Each signal
/// is written as
///
/// ```text
/// /// What happened, and when it comes.
/// Variant "name" { /// What the field is. field: Type, ... } called, "what extensions alone are told";
/// ```
///
/// `"name"` names it in its JSON's `signal`, in a read's `kinds` and in its
/// handler, `on_<name>`. `called` is what that handler is called with after
/// `game`: `fields`, the signal's fields, one argument each by its name;
/// `table`, the whole signal as one table, named as the signal; `given [...]`,
/// arguments the Lua host gives itself. The countries a signal names are
/// those its fields of countries hold ([`Names`]).
macro_rules! signals {
    ($(
        $(#[doc = $doc:literal])*
        $Kind:ident $name:literal {
            $( $(#[doc = $field_doc:literal])* $field:ident: $Ty:ty ),* $(,)?
        } $called:ident $([ $($given:expr),* $(,)? ])? $(, $note:literal)?;
    )*) => {
        /// Something that happened in the game, as extensions hear it.
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
        #[serde(tag = "signal")]
        pub enum Signal {
            $(
                $(#[doc = $doc])*
                #[serde(rename = $name)]
                $Kind { $( $(#[doc = $field_doc])* $field: $Ty, )* },
            )*
        }

        /// The kind of an [`Signal`], one for each of its variants, which
        /// `kinds`, [`SIGNALS`] and an extension's handler (`on_<kind>`) name
        /// by [`Kind::name`]. Whoever works a signal out only for those who
        /// listen asks by it, so that a kind misspelled does not compile.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Kind {
            $( $Kind, )*
        }

        impl Kind {
            /// Its name, as the signal's `signal` field gives it.
            pub fn name(self) -> &'static str {
                match self {
                    $( Self::$Kind => $name, )*
                }
            }

            /// The handler an extension hears it with, `on_<name>`.
            pub fn handler(self) -> &'static str {
                match self {
                    $( Self::$Kind => concat!("on_", $name), )*
                }
            }

            /// What its handler is called with after `game`.
            pub fn called(self) -> crate::lua::Called {
                match self {
                    $( Self::$Kind => called!($called $([$($given),*])?), )*
                }
            }

            /// What extensions alone are told of it, besides what it is;
            /// empty for nothing.
            pub fn note(self) -> &'static str {
                match self {
                    $( Self::$Kind => concat!("" $(, $note)?), )*
                }
            }

            /// The name of its class in the editor's type definitions, for
            /// a handler's table of it.
            pub fn class(self) -> &'static str {
                match self {
                    $( Self::$Kind => stringify!($Kind), )*
                }
            }
        }

        /// Every kind of signal, with what it is and when it comes, as its
        /// doc comment says it ([`crate::prose`] makes that one line).
        pub const SIGNALS: &[(Kind, &str)] = &[ $( (Kind::$Kind, concat!($($doc),*)), )* ];

        impl Signal {
            /// Its kind.
            pub fn kind(&self) -> Kind {
                match self {
                    $( Self::$Kind { .. } => Kind::$Kind, )*
                }
            }

            /// The countries it names: those its fields of countries hold,
            /// in their order; none for one about the game as a whole or
            /// about the player's own armies.
            pub fn countries(&self) -> Vec<CountryTag> {
                match self {
                    $( Self::$Kind { $($field),* } => Vec::new().into_iter() $( .chain(Names::named($field)) )* .collect(), )*
                }
            }
        }
    };
}

/// What a signal's handler is called with, as [`signals!`] writes it.
macro_rules! called {
    (fields) => {
        crate::lua::Called::Fields
    };
    (table) => {
        crate::lua::Called::Table
    };
    (given [$($given:expr),*]) => {{
        const GIVEN: &[crate::lua::Value] = &[$($given),*];
        crate::lua::Called::Given(GIVEN)
    }};
}

signals! {
    /// A session began: a save was loaded or a game started.
    Session "session" {
        /// Its number, as session.get answers it (number there; here the
        /// signal's own number is number).
        session: u64,
    } given [crate::lua::optional(
        "saved",
        "any",
        "what this mod's save_data returned when the save it began from was written, or, after an extension reload, what the code before it returned; nil for a new game or a save without it",
    )];

    /// The session is being left, while its state is still there to read.
    SessionEnded "session_ended" {
        /// Why: return for the main menu, load for a save loaded from it,
        /// new_game for a new game started from it, country when the
        /// player's country changed (the game console's tag, say).
        reason: SessionEnd,
    } fields, "Its waiting tasks are dropped after this.";

    /// In observe mode, another country was put in view, or the view taken
    /// out, by the player, a remote client or an extension. A new session
    /// starts with none heard: session.get says what is in view.
    CountryInView "country_in_view" {
        /// The country now in view; none once the view is taken out.
        country: Option<CountryTag>,
    } fields, "What acts as a country acts as the one in view.";

    /// An extension showed the player a pop-up; popup.list has its words
    /// and buttons.
    Popup "popup" {
        /// The pop-up's number, as popup.list and popup.answer take it.
        popup: u32,
        /// The id of the mod whose extension showed it.
        from: String,
    } fields;

    /// A new day, once visibility reflects it. With the game running fast,
    /// days in between may be left out.
    Day "day" {
        /// The new day.
        date: Date,
    } fields;

    /// A new month: the first day heard in it, after day. With the game
    /// running fast it may be past the 1st.
    Month "month" {
        /// The first day heard in the month.
        date: Date,
    } fields;

    /// A new year: the first day heard in it, after month.
    Year "year" {
        /// The first day heard in the year.
        date: Date,
    } fields;

    /// The player gave one of their armies an order, by hand or through
    /// remote control.
    ArmyOrdered "army_ordered" {
        /// The army's ID.
        army: ArmyId,
    } fields, "Not for this mod's own orders.";

    /// One of the player's armies lost a battle.
    ArmyDefeated "army_defeated" {
        /// The army's ID.
        army: ArmyId,
    } fields;

    /// A war began, between any two countries.
    WarDeclared "war_declared" {
        /// Its name, as the game shows it.
        war: String,
        /// The attackers' leader.
        attacker: CountryTag,
        /// The defenders' leader.
        defender: CountryTag,
    } fields;

    /// Countries left a war on a day: by a peace, the whole war's or a
    /// separate one, or by ceasing to be.
    PeaceMade "peace_made" {
        /// Its name, as the game shows it.
        war: String,
        /// The attackers that left it.
        attackers: Vec<CountryTag>,
        /// The defenders that left it.
        defenders: Vec<CountryTag>,
        /// Whether the war is over.
        ended: bool,
    } fields;

    /// A province changed owner.
    ProvinceOwnerChanged "province_owner_changed" {
        /// The province's number.
        province: ProvinceId,
        /// The old owner; none when it had none.
        from: Option<CountryTag>,
        /// The new owner; none when it has none.
        to: Option<CountryTag>,
    } fields;

    /// A building was finished in a province, any country's. Worked out
    /// only while some mod has its handler or a remote client reads it.
    BuildingFinished "building_finished" {
        /// The province's number.
        province: ProvinceId,
        /// The building's content key.
        building: String,
        /// The province's owner.
        country: CountryTag,
    } fields;

    /// A land or naval battle was decided: in the player's view one the
    /// player sees or takes part in, in the full view every one.
    Battle "battle" {
        /// Where it was fought, the province's number.
        province: ProvinceId,
        /// Whether it was fought at sea.
        naval: bool,
        /// The countries on the attacking side, the leading one first.
        attackers: Vec<CountryTag>,
        /// The countries on the defending side, the leading one first.
        defenders: Vec<CountryTag>,
        /// Which side won.
        winner: Winner,
        /// What the attackers lost: men in a land battle, ships in a naval
        /// one.
        attacker_losses: i64,
        /// What the defenders lost, as attacker_losses.
        defender_losses: i64,
    } table, "Extensions hear every one.";
}

impl Kind {
    /// The kind named `name`; none for a name no kind has.
    pub fn named(name: &str) -> Option<Self> {
        SIGNALS.iter().map(|(kind, _)| *kind).find(|kind| kind.name() == name)
    }

    /// The fields of its signals, in their order, as [`Signal`]'s shape has
    /// them.
    pub fn fields(self) -> Vec<Field> {
        let Shape::Either(signals) = Signal::shape() else { return Vec::new() };
        let tag = Shape::Words(vec![self.name()]);
        signals
            .into_iter()
            .find_map(|signal| match signal {
                Shape::Object { fields, .. } if fields.first().is_some_and(|first| first.shape == tag) => Some(fields[1..].to_vec()),
                _ => None,
            })
            .unwrap_or_default()
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

/// Which side won a battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Winner {
    Attackers,
    Defenders,
}

/// The countries a signal's field names ([`Signal::countries`]): a country,
/// those an option or a list of them holds, and none for a field of any
/// other type. A new type of field says here which it is.
trait Names {
    fn named(&self) -> Vec<CountryTag> {
        Vec::new()
    }
}

impl Names for CountryTag {
    fn named(&self) -> Vec<CountryTag> {
        vec![*self]
    }
}

impl<T: Names> Names for Option<T> {
    fn named(&self) -> Vec<CountryTag> {
        self.iter().flat_map(Names::named).collect()
    }
}

impl<T: Names> Names for Vec<T> {
    fn named(&self) -> Vec<CountryTag> {
        self.iter().flat_map(Names::named).collect()
    }
}

impl Names for u64 {}
impl Names for u32 {}
impl Names for i64 {}
impl Names for bool {}
impl Names for String {}
impl Names for Date {}
impl Names for ArmyId {}
impl Names for ProvinceId {}
impl Names for SessionEnd {}
impl Names for Winner {}

/// Which signals a read asks for: those of `kinds`, and among them those that
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

    /// Whether `signal` is one asked for.
    pub fn takes(&self, signal: &Signal) -> bool {
        let kind = self.kinds.as_ref().is_none_or(|kinds| kinds.iter().any(|k| k == signal.kind().name()));
        let named = signal.countries();
        let country = named.is_empty() || self.countries.as_ref().is_none_or(|countries| named.iter().any(|c| countries.contains(c)));
        kind && country
    }

    /// A kind it names that is no signal's.
    pub fn unknown_kind(&self) -> Option<&str> {
        self.kinds.as_ref()?.iter().find(|k| Kind::named(k).is_none()).map(String::as_str)
    }
}
