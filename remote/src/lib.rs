//! Remote control's requests and responses
//! (ADR remote-control-engine-and-transports), shared by the core, which
//! answers them, and the command line tool, which sends them.
//!
//! A request names one capability and carries its arguments. In the core it
//! is a [`Request`], one variant per group with one variant per capability
//! inside; in JSON it is one flat object, the capability's full name under
//! `"capability"` and the arguments beside it:
//!
//! ```json
//! {"capability": "army.move", "army": "4294967377", "goal": 151}
//! ```
//!
//! A full name is the group's name, a dot and the capability's own name
//! (`"army.move"`, `"country.hostile"`), or the group's name alone when the
//! capability's own name is empty (`"session"`, `"events"`). A country is
//! its tag (`"FRA"`), a province its number, an army its ID as a decimal
//! string (a 64-bit integer does not survive a JavaScript number), a date
//! `"1444.11.11"`.
//!
//! A response is the capability's [`Answer`] or an [`Error`]; in JSON
//! `{"ok": true, "result": ...}` or `{"ok": false, "error": {"code": ...,
//! "message": ...}}` ([`response_json`]). Bytes in an answer, such as a
//! screenshot, are a [`Binary`]: `{"media_type": ..., "base64": ...}`.
//!
//! Every capability is declared once, in its group's file: the request, the
//! answer and the description [`capabilities`] lists come from that one
//! declaration, so a client can be built from the descriptions alone.
// derive(Shaped) names the crate by its own name, here too.
extern crate self as eu4_harmony_remote;

use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use shape::{Shape, Shaped};
use std::fmt;

/// The version of the capability catalogue (ADR one-capability-catalogue),
/// which `describe` answers: it changes when a capability that exists changes
/// what it means or how it is shaped, not when one is added.
pub const API_VERSION: &str = "0.3";

/// The named pipe the game serves remote control on
/// (ADR remote-control-engine-and-transports), one JSON request per line in and
/// one response per line out.
pub const PIPE: &str = r"\\.\pipe\eu4-harmony";

/// The pipe a test game serves remote control on, the game started with
/// `-harmony_test_game=<name>` by a developer build that has the test games'
/// pipes (docs/plans/test-games.md): [`PIPE`] and `-<name>`; [`PIPE`] itself
/// without a name. A name is letters, digits, `.`, `_` and `-`, 64 at most.
pub fn pipe_of(test_game: Option<&str>) -> Result<String, String> {
    match test_game {
        None => Ok(PIPE.to_string()),
        Some(name) if test_game_name(name) => Ok(format!("{PIPE}-{name}")),
        Some(name) => Err(format!("{name:?} is not a test game's name: letters, digits, '.', '_' and '-', 64 at most")),
    }
}

/// Whether `name` may name a test game.
pub fn test_game_name(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Whether a capability runs only while a game is going on.
macro_rules! in_game {
    (game) => {
        true
    };
    (world) => {
        true
    };
    (anywhere) => {
        false
    };
    (remote_game) => {
        true
    };
    (remote_world) => {
        true
    };
    (remote_anywhere) => {
        false
    };
}

/// Whether a capability acts as a country: the one the player plays, or in
/// observe mode the country in view (ADR
/// observe-mode-acts-as-the-country-in-view).
macro_rules! country {
    (game) => {
        true
    };
    (remote_game) => {
        true
    };
    ($place:ident) => {
        false
    };
}

/// Whether extensions have a capability too, or only remote control
/// (ADR remote-control-uses-capabilities).
macro_rules! for_extensions {
    (game) => {
        true
    };
    (world) => {
        true
    };
    (anywhere) => {
        true
    };
    (remote_game) => {
        false
    };
    (remote_world) => {
        false
    };
    (remote_anywhere) => {
        false
    };
}

/// Where a capability runs, as a type: what the core opens for it before
/// its capability runs, a session with a country to act as ([`place::Game`]),
/// one with the world alone ([`place::World`]), or nothing
/// ([`place::Anywhere`]).
pub mod place {
    /// A game going on, acting as a country.
    pub struct Game;
    /// A game going on, acting as no country.
    pub struct World;
    /// Also at the main menu.
    pub struct Anywhere;
}

/// Where a capability runs, as a type of [`place`].
macro_rules! place {
    (game) => {
        $crate::place::Game
    };
    (remote_game) => {
        $crate::place::Game
    };
    (world) => {
        $crate::place::World
    };
    (remote_world) => {
        $crate::place::World
    };
    (anywhere) => {
        $crate::place::Anywhere
    };
    (remote_anywhere) => {
        $crate::place::Anywhere
    };
}

/// One capability's arguments, each by its name, as its group's `args`
/// module declares them: what the core's capability layer takes, so that two
/// of one type cannot reach it the other way round.
pub trait Arguments {
    /// Where the capability runs, a type of [`place`].
    type Place;
    /// How many arguments it has.
    const COUNT: usize;
}

/// Declares one group of capabilities: its request enum, its answer enum and
/// their descriptions, from one list. Each capability is written as
///
/// ```text
/// /// What it does.
/// game Variant "name" { /// What the argument is. argument: Type, ... } -> Result, "what the result is";
/// ```
///
/// `game` for a capability that needs a game going on and acts as a country
/// (the one the player plays, or the country in view), `world` for one that
/// needs a game going on and acts as no country, `anywhere` for one that needs
/// no game, each with `remote_` before it for one only remote control has; `"name"` empty for one the group's name alone names.
macro_rules! group {
    (
        $(#[doc = $about:literal])*
        $group:literal: $Request:ident, $Answer:ident;
        $(
            $(#[doc = $doc:literal])*
            $place:ident $Variant:ident $action:literal {
                $( $(#[doc = $arg_doc:literal])* $arg:ident: $Ty:ty ),* $(,)?
            } -> $Result:ty, $result:literal;
        )*
    ) => {
        /// The group's name, the first part of its capabilities' full names.
        pub const GROUP: &str = $group;
        /// What the group is about.
        pub const ABOUT: &str = concat!($($about),*);

        $(#[doc = $about])*
        #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        #[serde(tag = "capability", deny_unknown_fields)]
        pub enum $Request {
            $(
                $(#[doc = $doc])*
                #[serde(rename = $action)]
                $Variant {
                    $( $(#[doc = $arg_doc])* #[serde(skip_serializing_if = "crate::ArgKind::absent")] $arg: $Ty, )*
                },
            )*
        }

        /// The answer to each capability of the group, by capability.
        #[derive(Clone, Debug, PartialEq, serde::Serialize)]
        #[serde(untagged)]
        pub enum $Answer {
            $( $Variant($Result), )*
        }

        /// Each capability's arguments, by name, as the core's capability
        /// layer takes them ([`Arguments`](crate::Arguments)).
        pub mod args {
            use super::*;

            $(
                #[doc = concat!("The arguments of `", $group, ".", $action, "`.")]
                #[derive(Clone, Debug, PartialEq)]
                pub struct $Variant {
                    $( $(#[doc = $arg_doc])* pub $arg: $Ty, )*
                }

                impl $crate::Arguments for $Variant {
                    type Place = place!($place);
                    const COUNT: usize = <[&str]>::len(&[$(stringify!($arg)),*]);
                }

                impl From<$Variant> for $Request {
                    fn from(args: $Variant) -> Self {
                        let $Variant { $($arg),* } = args;
                        $Request::$Variant { $($arg),* }
                    }
                }
            )*

            /// A request of the group, as its capability's arguments.
            pub enum Args {
                $( $Variant($Variant), )*
            }
        }

        impl $Request {
            /// The request as its capability's arguments.
            pub fn into_args(self) -> args::Args {
                match self {
                    $( Self::$Variant { $($arg),* } => args::Args::$Variant(args::$Variant { $($arg),* }), )*
                }
            }
        }

        impl $crate::GroupRequest for $Request {
            fn full_name(&self) -> ::std::string::String {
                $crate::full_name(GROUP, self.action())
            }
        }

        impl $Request {
            /// The capability's own name in its group.
            pub fn action(&self) -> &'static str {
                match self {
                    $( Self::$Variant { .. } => $action, )*
                }
            }

            /// Whether it runs only while a game is going on.
            pub fn in_game(&self) -> bool {
                match self {
                    $( Self::$Variant { .. } => in_game!($place), )*
                }
            }

            /// Whether it acts as a country, and fails with no country in
            /// view.
            pub fn country(&self) -> bool {
                match self {
                    $( Self::$Variant { .. } => country!($place), )*
                }
            }

            /// [`Self::country`] of the capability whose variant is named
            /// `variant`, for the core's test that its binding opens a
            /// session with a country to act as exactly then.
            #[doc(hidden)]
            pub fn country_of(variant: &str) -> Option<bool> {
                match variant {
                    $( stringify!($Variant) => Some(country!($place)), )*
                    _ => None,
                }
            }
        }

        /// The shape of each capability's result, by its full name.
        pub fn result_shapes() -> ::std::vec::Vec<(::std::string::String, $crate::shape::Shape)> {
            ::std::vec![ $( ($crate::full_name($group, $action), <$Result as $crate::shape::Shaped>::shape()), )* ]
        }

        /// The group's capabilities, described.
        pub const CAPABILITIES: &[$crate::Capability] = &[
            $(
                $crate::Capability {
                    group: $group,
                    action: $action,
                    about: concat!($($doc),*),
                    in_game: in_game!($place),
                    country: country!($place),
                    extensions: for_extensions!($place),
                    args: &[
                        $(
                            $crate::Parameter {
                                name: stringify!($arg),
                                kind: <$Ty as $crate::ArgKind>::KIND,
                                optional: <$Ty as $crate::ArgKind>::OPTIONAL,
                                about: concat!($($arg_doc),*),
                            },
                        )*
                    ],
                    result: $result,
                },
            )*
        ];
    };
}

/// Declares the groups: [`Request`] and [`Answer`] with a variant for each,
/// the JSON form of a request, and [`capabilities`]. A group with
/// `#[cfg(...)]` before it is a capability only in builds that meet it; its
/// module, the types of its requests and answers, is in every build, for the
/// clients that ask it of a game that has it.
macro_rules! groups {
    ($( $(#[cfg($cfg:meta)])? $(#[doc = $doc:literal])* $Group:ident($module:ident::$Request:ident, $Answer:ident), )*) => {
        $( pub mod $module; )*

        /// A request to remote control: one capability and its arguments.
        #[derive(Clone, Debug, PartialEq)]
        pub enum Request {
            $( $(#[cfg($cfg)])? $(#[doc = $doc])* $Group($module::$Request), )*
        }

        /// What a capability answers, by group and capability.
        #[derive(Clone, Debug, PartialEq, Serialize)]
        #[serde(untagged)]
        pub enum Answer {
            $( $(#[cfg($cfg)])? $Group($module::$Answer), )*
        }

        impl Request {
            /// The capability's full name, such as `army.move`.
            pub fn name(&self) -> String {
                match self {
                    $( $(#[cfg($cfg)])? Self::$Group(r) => full_name($module::GROUP, r.action()), )*
                }
            }

            /// Whether it runs only while a game is going on.
            pub fn in_game(&self) -> bool {
                match self {
                    $( $(#[cfg($cfg)])? Self::$Group(r) => r.in_game(), )*
                }
            }

            /// Whether it acts as a country, and fails with no country in
            /// view.
            pub fn country(&self) -> bool {
                match self {
                    $( $(#[cfg($cfg)])? Self::$Group(r) => r.country(), )*
                }
            }

            /// The request's arguments and its `"capability"` as the group sees it.
            fn inner(&self) -> serde_json::Result<Value> {
                match self {
                    $( $(#[cfg($cfg)])? Self::$Group(r) => serde_json::to_value(r), )*
                }
            }

            /// The request of `group` from its arguments and its own name.
            fn from_group<E: de::Error>(group: &str, fields: Map<String, Value>) -> Result<Self, E> {
                match group {
                    $( $(#[cfg($cfg)])? $module::GROUP => serde_json::from_value(Value::Object(fields)).map(Self::$Group).map_err(E::custom), )*
                    _ => Err(E::custom(format!("no group {group}"))),
                }
            }
        }

        /// Every capability remote control has, described, group by group.
        pub fn capabilities() -> Vec<Capability> {
            let mut all = Vec::new();
            $( $(#[cfg($cfg)])? all.extend_from_slice($module::CAPABILITIES); )*
            all
        }

        /// The shape of the result of the capability named `name`, as Lua
        /// has it.
        pub fn result_shape(name: &str) -> Option<shape::Shape> {
            let mut all = Vec::new();
            $( $(#[cfg($cfg)])? all.extend($module::result_shapes()); )*
            all.into_iter().find(|(n, _)| *n == name).map(|(_, s)| s)
        }

        /// Every group, by name and what it is about.
        #[expect(clippy::vec_init_then_push, reason = "a group may be left out of the build, which vec! cannot do")]
        pub fn groups() -> Vec<Group> {
            let mut all = Vec::new();
            $( $(#[cfg($cfg)])? all.push(Group { name: $module::GROUP, about: $module::ABOUT }); )*
            all
        }
    };
}

impl Request {
    /// Whether it runs only where it is known whether a game is going on:
    /// what needs a game, and what does one thing in the main menu and
    /// another in a game.
    pub fn needs_place(&self) -> bool {
        self.in_game() || matches!(self, Self::Menu(r) if r.needs_place())
    }
}

groups! {
    /// The campaign being played.
    Session(session::SessionRequest, SessionAnswer),
    /// Time: the date, pausing and resuming, the speed, and advancing time.
    Time(time::TimeRequest, TimeAnswer),
    /// Countries.
    Country(countries::CountryRequest, CountryAnswer),
    /// Provinces and their neighbors.
    Province(provinces::ProvinceRequest, ProvinceAnswer),
    /// The player's armies and the armies they can see.
    Army(armies::ArmyRequest, ArmyAnswer),
    /// Texts and the player language.
    Text(texts::TextRequest, TextAnswer),
    /// The frame the player sees, as an image.
    Screenshot(screenshot::ScreenshotRequest, ScreenshotAnswer),
    /// The game console.
    Console(console::ConsoleRequest, ConsoleAnswer),
    /// The game's events waiting for the player's answer.
    Question(questions::QuestionRequest, QuestionAnswer),
    /// What other countries proposed to the player, waiting for an answer.
    Offer(offers::OfferRequest, OfferAnswer),
    /// The events remote control keeps.
    Events(events::EventsRequest, EventsAnswer),
    /// The capabilities, described.
    Describe(describe::DescribeRequest, DescribeAnswer),
    /// The map camera.
    Camera(camera::CameraRequest, CameraAnswer),
    /// Recruiting regiments and ships.
    Recruit(recruiting::RecruitRequest, RecruitAnswer),
    /// The main menu: where the game is, the saves, loading one, starting a
    /// game and returning to the main menu.
    Menu(menu::MenuRequest, MenuAnswer),
    /// The player's diplomatic actions.
    Diplomacy(diplomacy::DiplomacyRequest, DiplomacyAnswer),
    /// The mods' extensions: reloading one in the running game.
    Extension(extensions::ExtensionRequest, ExtensionAnswer),
    #[cfg(feature = "dev-memory")]
    /// What the game holds in memory, and what the heap census found: a
    /// developer build's (docs/plans/leak-finding.md).
    Memory(memory::MemoryRequest, MemoryAnswer),
    /// Mouse and keyboard input at screen positions, as the player gives it.
    Input(input::InputRequest, InputAnswer),
    /// The game on the player's desktop: a hidden start, showing and hiding it.
    Desktop(desktop::DesktopRequest, DesktopAnswer),
    /// The decisions the player can see in the decisions view.
    Decision(decisions::DecisionRequest, DecisionAnswer),
    /// The interactions with the player's subjects.
    Subject(subjects::SubjectRequest, SubjectAnswer),
    /// The technology view: researching technology.
    Technology(technology::TechnologyRequest, TechnologyAnswer),
    /// The national focus, in the court view.
    Focus(focus::FocusRequest, FocusAnswer),
    /// The stability and expansion view: raising stability.
    Stability(stability::StabilityRequest, StabilityAnswer),
    /// The treasury's monarch-point spending and bank loans.
    Economy(economy::EconomyRequest, EconomyAnswer),
    /// The player's loans, and repaying them.
    Loan(loans::LoanRequest, LoanAnswer),
    /// The peace view: terms demanded of a country the player is at war
    /// with, or offered to it.
    Peace(peace::PeaceRequest, PeaceAnswer),
    /// The transfer occupation window: handing a province the player
    /// occupies over to an ally.
    Occupation(occupation::OccupationRequest, OccupationAnswer),
    /// The player's idea groups and ideas.
    Idea(ideas::IdeaRequest, IdeaAnswer),
    /// The player's policies.
    Policy(policies::PolicyRequest, PolicyAnswer),
    /// The player's government: its reforms and its rank.
    Government(government::GovernmentRequest, GovernmentAnswer),
    /// The player's estates.
    Estate(estates::EstateRequest, EstateAnswer),
    /// The player's parliament.
    Parliament(parliament::ParliamentRequest, ParliamentAnswer),
    /// The player's missions.
    Mission(missions::MissionRequest, MissionAnswer),
    /// The buildings of the player's provinces.
    Building(buildings::BuildingRequest, BuildingAnswer),
    /// The player's leaders: recruiting, assigning and dismissing them.
    Leader(leaders::LeaderRequest, LeaderAnswer),
    /// The mercenary companies the player can hire.
    Mercenary(mercenaries::MercenaryRequest, MercenaryAnswer),
    /// The player's forts.
    Fort(forts::FortRequest, FortAnswer),
    /// The siege view's buttons, for a siege of the player's.
    Siege(sieges::SiegeRequest, SiegeAnswer),
    /// The player's missionaries.
    Missionary(missionaries::MissionaryRequest, MissionaryAnswer),
    /// The player's colonists.
    Colonist(colonists::ColonistRequest, ColonistAnswer),
    /// The player's trade companies.
    TradeCompany(trade_companies::TradeCompanyRequest, TradeCompanyAnswer),
    /// Centers of trade in the player's provinces.
    CenterOfTrade(centers_of_trade::CenterOfTradeRequest, CenterOfTradeAnswer),
    /// The player's merchants.
    Merchant(merchants::MerchantRequest, MerchantAnswer),
    /// Trade nodes, as the trade map mode and the trade node view show them.
    Trade(trade::TradeRequest, TradeAnswer),
}

pub mod binary;
pub mod choices;
pub use binary::Binary;
pub mod lua;
pub mod shape;

fn full_name(group: &str, action: &str) -> String {
    if action.is_empty() {
        group.to_string()
    } else {
        format!("{group}.{action}")
    }
}

/// A request a client can send: [`Request`], or a request of one group, such
/// as [`menu::MenuRequest`], which knows its capability's full name.
pub trait GroupRequest: Serialize {
    /// The capability's full name, such as `menu.load`.
    fn full_name(&self) -> String;
}

impl GroupRequest for Request {
    fn full_name(&self) -> String {
        self.name()
    }
}

/// The JSON form of a request, its capability by its full name.
pub fn request_json<R: GroupRequest>(request: &R) -> Value {
    let mut value = serde_json::to_value(request).expect("a request is JSON");
    value["capability"] = Value::String(request.full_name());
    value
}

impl Serialize for Request {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use ser::SerializeMap;
        let Value::Object(fields) = self.inner().map_err(ser::Error::custom)? else {
            return Err(ser::Error::custom("a request is not an object"));
        };
        // The capability first, then the arguments.
        let mut map = serializer.serialize_map(Some(fields.len()))?;
        map.serialize_entry("capability", &self.name())?;
        for (key, value) in fields.iter().filter(|(key, _)| *key != "capability") {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_fields(Map::deserialize(deserializer)?)
    }
}

impl Request {
    /// The request the JSON object `fields` writes, its capability by its
    /// full name in `capability`.
    fn from_fields<E: de::Error>(mut fields: Map<String, Value>) -> Result<Self, E> {
        let known = match fields.get("capability") {
            Some(Value::String(name)) => capability(name).ok_or_else(|| E::custom(format!("no capability {name}")))?,
            Some(_) => return Err(E::custom("the capability is not a text")),
            None => return Err(E::custom("no capability named")),
        };
        fields.insert("capability".into(), Value::String(known.action.into()));
        Self::from_group(known.group, fields)
    }
}

/// The capability named `name` (such as `army.move`), from an index made once.
pub fn capability(name: &str) -> Option<&'static Capability> {
    static INDEX: std::sync::OnceLock<std::collections::HashMap<String, Capability>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| capabilities().into_iter().map(|c| (c.name(), c)).collect()).get(name)
}

/// Declares every kind of argument once, one line each: its name as a
/// description writes it, an example as text, the Rust types that are
/// arguments of it, the shape Lua passes it in, and how text such as a
/// command line's becomes its JSON. [`Kind`], its methods, [`Kind::ALL`] and
/// each type's [`ArgKind`] come from these lines.
macro_rules! kinds {
    ($( $(#[doc = $doc:literal])* $Kind:ident $name:literal $example:literal ($($ty:ty),*) => $shape:expr, $json:expr; )*) => {
        /// What kind of value an argument is, which says how to write it in JSON.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        pub enum Kind {
            $( $(#[doc = $doc])* #[serde(rename = $name)] $Kind, )*
        }

        impl Kind {
            /// Every kind.
            pub const ALL: &[Kind] = &[$(Kind::$Kind),*];

            /// Its name, as a description writes it.
            pub fn name(self) -> &'static str {
                match self {
                    $( Self::$Kind => $name, )*
                }
            }

            /// A value of the kind, as text.
            pub fn example(self) -> &'static str {
                match self {
                    $( Self::$Kind => $example, )*
                }
            }

            /// A value of the kind written as text, such as on a command line, in
            /// JSON: a number for a province or an integer, `true` or `false` for a
            /// boolean, the text itself for the rest. None when the text cannot be
            /// one; whether it names something that exists is the request's to say.
            pub fn json(self, text: &str) -> Option<Value> {
                match self {
                    $( Self::$Kind => {
                        let json: fn(&str) -> Option<Value> = $json;
                        json(text)
                    } )*
                }
            }

            /// A value of the kind as Lua passes it, which the editor's types write:
            /// the shape of the argument's own type where it has one, so that a
            /// term's fields, a unit kind's words, are its type's.
            pub fn shape(self) -> Shape {
                match self {
                    $( Self::$Kind => $shape, )*
                }
            }
        }

        $( $( impl ArgKind for $ty { const KIND: Kind = Kind::$Kind; } )* )*
    };
}

kinds! {
    /// A country tag, such as `"FRA"`.
    Country "country" "FRA" (CountryTag) => Shape::Text, text;
    /// A province number.
    Province "province" "151" (ProvinceId) => Shape::Integer, integer;
    /// An army ID as a decimal string.
    Army "army" "4294967377" (ArmyId) => Shape::Integer, text;
    /// A date, such as `"1444.11.11"`.
    Date "date" "1444.11.11" (Date) => Shape::Either(vec![Shape::Date, Shape::Text]), text;
    Integer "integer" "3" (u32, u64, i32) => Shape::Integer, integer;
    Boolean "boolean" "true" (bool) => Shape::Boolean, |text| text.parse().ok().map(Value::Bool);
    Text "text" "text" (String) => Shape::Text, text;
    /// A kind of regiment or ship, such as `"infantry"` (see
    /// [`recruiting::UnitKind`]).
    UnitKind "unit_kind" "infantry" (recruiting::UnitKind) => recruiting::UnitKind::shape(), word::<recruiting::UnitKind>;
    /// Allies to call to arms: `"all"`, `"none"` or a list of tags, such as
    /// `["DUL", "CND"]`, which a command line writes `DUL,CND` (see
    /// [`diplomacy::CallAllies`]).
    Allies "allies" "DUL,CND" (diplomacy::CallAllies) => Shape::Either(vec![Shape::Words(vec!["all", "none"]), list(Shape::Text)]),
        |text| diplomacy::CallAllies::parse(text).ok().and_then(|allies| serde_json::to_value(allies).ok());
    /// Province numbers, such as `[151, 152]`, which a command line writes
    /// `151,152` (see [`ProvinceList`]).
    Provinces "provinces" "151,152" (ProvinceList) => list(Shape::Integer), |text| listed(text, |word| word.parse::<u32>().ok().map(Value::from));
    /// A mouse button: `"left"`, `"right"` or `"middle"` (see
    /// [`input::Button`]).
    Button "button" "left" (input::Button) => input::Button::shape(), word::<input::Button>;
    /// Which way the mouse wheel turns: `"up"`, `"down"`, `"left"` or
    /// `"right"` (see [`input::Direction`]).
    Direction "direction" "up" (input::Direction) => input::Direction::shape(), word::<input::Direction>;
    /// A key, or keys pressed together joined with `+`, such as `"ctrl+s"`
    /// (see [`input::Keys`]).
    Keys "keys" "ctrl+s" (input::Keys) => Shape::Text, |text| input::Keys::parse(text).ok().map(|keys| Value::String(keys.to_string()));
    /// Country tags, such as `["FRA", "ENG"]`, which a command line writes
    /// `FRA,ENG` (see [`CountryList`]).
    Countries "countries" "FRA,ENG" (CountryList) => list(Shape::Text), |text| listed(text, |word| CountryTag::parse(word).map(|tag| Value::String(tag.to_string())));
    /// Words, such as content keys or the names of events, `["day",
    /// "month"]`, which a command line writes `day,month` (see [`Words`]).
    Words "words" "day,month" (Words) => list(Shape::Text), |text| listed(text, |word| Some(word).filter(|w| !w.is_empty()).map(|w| Value::String(w.into())));
    /// A kind of monarch power: `"adm"`, `"dip"` or `"mil"` (see
    /// [`choices::PowerKind`]).
    PowerKind "power_kind" "adm" (choices::PowerKind) => choices::PowerKind::shape(), word::<choices::PowerKind>;
    /// Army IDs, such as `["4294967377", "4294967378"]`, which a command
    /// line writes `4294967377,4294967378` (see [`ArmyList`]).
    Armies "armies" "4294967377,4294967378" (ArmyList) => list(Shape::Integer),
        |text| listed(text, |word| Some(word).filter(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit())).map(|w| Value::String(w.into())));
    /// Peace treaty terms, a list of objects such as
    /// `[{"term":"demand_provinces","province":236}]`, which a command line
    /// writes as that JSON text (see [`peace::Terms`]).
    Terms "terms" r#"[{"term":"humiliate"}]"# (peace::Terms) => Vec::<peace::Term>::shape(),
        |text| peace::Terms::parse(text).ok().and_then(|terms| serde_json::to_value(terms).ok());
}

/// The text itself, as JSON.
fn text(text: &str) -> Option<Value> {
    Some(Value::String(text.into()))
}

/// A whole number, as JSON.
fn integer(text: &str) -> Option<Value> {
    serde_json::from_str::<serde_json::Number>(text).ok().filter(|n| n.is_i64() || n.is_u64()).map(Value::Number)
}

/// The text, when it is one of the words `T` is written as.
fn word<T: de::DeserializeOwned>(text: &str) -> Option<Value> {
    serde_json::from_value::<T>(Value::String(text.into())).ok().map(|_| Value::String(text.into()))
}

/// Words joined with commas, each as `each` writes it.
fn listed(text: &str, each: impl Fn(&str) -> Option<Value>) -> Option<Value> {
    text.split(',').map(|word| each(word.trim())).collect::<Option<Vec<_>>>().map(Value::Array)
}

/// A list of `inner`.
fn list(inner: Shape) -> Shape {
    Shape::List(Box::new(inner))
}

/// The kind of a Rust type used as an argument.
pub trait ArgKind {
    const KIND: Kind;
    const OPTIONAL: bool = false;
    /// Whether the value is an optional argument left out.
    fn absent(&self) -> bool {
        false
    }
}

impl<T: ArgKind> ArgKind for Option<T> {
    const KIND: Kind = T::KIND;
    const OPTIONAL: bool = true;
    fn absent(&self) -> bool {
        self.is_none()
    }
}

/// One capability, described.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub group: &'static str,
    /// Its own name in the group; empty when the group's name alone names it.
    pub action: &'static str,
    pub about: &'static str,
    /// It runs only while a game is going on.
    pub in_game: bool,
    /// It acts as a country: the one the player plays, or in observe mode
    /// the country in view; it fails with no_country while none is in view.
    pub country: bool,
    /// Extensions have it too, not only remote control.
    pub extensions: bool,
    pub args: &'static [Parameter],
    /// What a successful call answers.
    pub result: &'static str,
}

impl Capability {
    /// Its full name, such as `army.move`.
    pub fn name(&self) -> String {
        full_name(self.group, self.action)
    }
}

/// A group of capabilities, described.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: &'static str,
    pub about: &'static str,
}

/// One argument of a capability, described.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: &'static str,
    pub kind: Kind,
    pub optional: bool,
    pub about: &'static str,
}

/// Documentation comments, written one line at a time, as one text, such as
/// a [`Capability`]'s or [`Parameter`]'s `about`.
pub fn prose(doc: &str) -> String {
    doc.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Serialize for Capability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Capability", 7)?;
        s.serialize_field("capability", &self.name())?;
        s.serialize_field("about", &prose(self.about))?;
        s.serialize_field("in_game", &self.in_game)?;
        s.serialize_field("country", &self.country)?;
        s.serialize_field("extensions", &self.extensions)?;
        s.serialize_field("args", self.args)?;
        s.serialize_field("result", self.result)?;
        s.end()
    }
}

impl Serialize for Parameter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Parameter", 4)?;
        s.serialize_field("name", self.name)?;
        s.serialize_field("kind", &self.kind)?;
        s.serialize_field("optional", &self.optional)?;
        s.serialize_field("about", &prose(self.about))?;
        s.end()
    }
}

// ---------- identifiers ----------

/// A country, by its tag: three capital letters or digits, such as `FRA` or
/// `D01`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CountryTag(pub [u8; 3]);

impl CountryTag {
    /// The tag written in `text`, in either case.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes: [u8; 3] = text.as_bytes().try_into().ok()?;
        bytes.iter().all(u8::is_ascii_alphanumeric).then(|| Self(bytes.map(|b| b.to_ascii_uppercase())))
    }
}

impl fmt::Display for CountryTag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

impl fmt::Debug for CountryTag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Serialize for CountryTag {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CountryTag {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| de::Error::custom(format!("{text:?} is not a country tag (three letters or digits, such as FRA)")))
    }
}

/// A province, by its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProvinceId(pub u32);

/// Province numbers, as an argument: a list, `[151, 152]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProvinceList(pub Vec<ProvinceId>);

/// Countries, as an argument: a list of tags, `["FRA", "ENG"]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CountryList(pub Vec<CountryTag>);

/// Words, as an argument: a list of texts, such as content keys or the
/// names of events, `["day", "month"]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Words(pub Vec<String>);

/// Armies or fleets, as an argument: a list of IDs, `["4294967377",
/// "4294967378"]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArmyList(pub Vec<ArmyId>);

/// An army or a fleet, by its 64-bit ID, which survives saving and loading;
/// in JSON a decimal string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArmyId(pub u64);

/// A decimal string, or in Lua's form (`eu4_harmony_checks::form`) the
/// integer itself.
impl Serialize for ArmyId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if eu4_harmony_checks::form::lua() {
            return serializer.serialize_u64(self.0);
        }
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ArmyId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = ArmyId;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an army ID as a decimal string")
            }
            fn visit_str<E: de::Error>(self, text: &str) -> Result<ArmyId, E> {
                match text.parse() {
                    Ok(id) if text.bytes().all(|b| b.is_ascii_digit()) => Ok(ArmyId(id)),
                    _ => Err(E::custom(format!("{text:?} is not an army ID (a decimal string)"))),
                }
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

/// A game date (`eu4_harmony_checks::Date`), in JSON `"year.month.day"`,
/// such as `"1444.11.11"`.
pub use eu4_harmony_checks::Date;

// ---------- responses ----------

/// Why a request failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    /// No game is going on: the main menu is open. While a game is being
    /// set up or a save loads, a request that needs a game waits for one
    /// instead.
    NotInGame,
    /// It acts as a country, and the player observes with no country in
    /// view (ADR observe-mode-acts-as-the-country-in-view).
    NoCountry,
    /// The game state cannot be read right now; it may work a little later.
    NotReady,
    NotFound,
    /// A command was refused; the message says why.
    Rejected,
    InvalidArgument,
    /// Not available in this state or to remote control.
    Unavailable,
    /// The request could not be read: not JSON, no such capability, or
    /// arguments missing or of the wrong kind.
    BadRequest,
    /// It needs a game, and the session it was sent in ended before it ran
    /// (a save was loaded or another game started): it did not run. Read
    /// the session again before asking again.
    SessionChanged,
    /// Something went wrong in the framework or the game.
    Internal,
}

impl Code {
    /// Every code a client may get, which its help and the documentation
    /// explain.
    pub const ALL: [Code; 10] = [
        Self::NotInGame,
        Self::NoCountry,
        Self::NotReady,
        Self::NotFound,
        Self::Rejected,
        Self::InvalidArgument,
        Self::Unavailable,
        Self::BadRequest,
        Self::SessionChanged,
        Self::Internal,
    ];

    /// Its name, as JSON writes it.
    pub fn name(self) -> &'static str {
        match self {
            Self::NotInGame => "not_in_game",
            Self::NoCountry => "no_country",
            Self::NotReady => "not_ready",
            Self::NotFound => "not_found",
            Self::Rejected => "rejected",
            Self::InvalidArgument => "invalid_argument",
            Self::Unavailable => "unavailable",
            Self::BadRequest => "bad_request",
            Self::SessionChanged => "session_changed",
            Self::Internal => "internal",
        }
    }

    /// What it means, and what a client does about it.
    pub fn about(self) -> &'static str {
        match self {
            Self::NotInGame => "no game is going on: the main menu is open; menu says where the game is",
            Self::NoCountry => "the player observes with no country in view, so there is no country to act as; one is put in view as the player does, Ctrl and a click on the map",
            Self::NotReady => "the game cannot answer it right now, such as before visibility has caught up with the date; ask again shortly",
            Self::NotFound => "no such thing: an army, a province, a save, a route",
            Self::Rejected => "the game refused it, as it would refuse the player; the message gives the game's reasons, one a line",
            Self::InvalidArgument => "a value that cannot be right, such as a speed of 9",
            Self::Unavailable => "not here or not now, or not for this caller",
            Self::BadRequest => "a request the game could not read: not JSON, no such capability, or arguments missing or of the wrong kind",
            Self::SessionChanged => "another game was loaded or started before it ran, so it did not run; read the session again",
            Self::Internal => "a fault in the framework or the game; the framework log says more",
        }
    }
}

/// A failed request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: Code,
    pub message: String,
}

impl Error {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code.name(), self.message)
    }
}

/// A refusal of the framework's own checks, as remote clients get it.
impl From<eu4_harmony_checks::Refusal> for Error {
    fn from(refusal: eu4_harmony_checks::Refusal) -> Self {
        use eu4_harmony_checks::Status;
        let code = match refusal.status {
            Status::NotReady => Code::NotReady,
            Status::NotFound => Code::NotFound,
            Status::Rejected => Code::Rejected,
            Status::InvalidArgument => Code::InvalidArgument,
        };
        Self::new(code, refusal.message)
    }
}

/// What a request comes to.
pub type Response = Result<Answer, Error>;

/// A request from its JSON form; one that cannot be read is a
/// [`Code::BadRequest`] error.
pub fn request_from_json(text: &str) -> Result<Request, Error> {
    serde_json::from_str(text).map_err(|e| Error::new(Code::BadRequest, e.to_string()))
}

/// The request a JSON object already read writes, as [`request_from_json`]
/// takes it from text.
pub fn request_from_fields(fields: Map<String, Value>) -> Result<Request, Error> {
    Request::from_fields(fields).map_err(|e: serde_json::Error| Error::new(Code::BadRequest, e.to_string()))
}

/// The JSON form of a response: `{"ok": true, "result": ...}` or
/// `{"ok": false, "error": {"code": ..., "message": ...}}`.
pub fn response_json(response: &Response) -> Value {
    match response {
        Ok(answer) => match serde_json::to_value(answer) {
            Ok(result) => serde_json::json!({ "ok": true, "result": result }),
            Err(e) => response_json(&Err(Error::new(Code::Internal, format!("the answer cannot be written as JSON: {e}")))),
        },
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

#[cfg(test)]
mod tests;
