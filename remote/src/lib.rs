//! Remote control's requests and responses (ADR 0016), shared by the core,
//! which answers them, and the command line tool, which sends them.
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
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::fmt;

/// The named pipe the game serves remote control on (ADR 0016), one JSON
/// request per line in and one response per line out.
pub const PIPE: &str = r"\\.\pipe\eu4-harmony";

/// Whether a capability runs only while a game is going on.
macro_rules! in_game {
    (game) => {
        true
    };
    (anywhere) => {
        false
    };
}

/// Declares one group of capabilities: its request enum, its answer enum and
/// their descriptions, from one list. Each capability is written as
///
/// ```text
/// /// What it does.
/// game Variant "name" { /// What the argument is. argument: Type, ... } -> Result, "what the result is";
/// ```
///
/// `game` for a capability that needs a game going on, `anywhere` for one
/// that does not; `"name"` empty for one the group's name alone names.
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
        }

        /// The group's capabilities, described.
        pub const CAPABILITIES: &[$crate::Capability] = &[
            $(
                $crate::Capability {
                    group: $group,
                    action: $action,
                    about: concat!($($doc),*),
                    in_game: in_game!($place),
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
/// the JSON form of a request, and [`capabilities`].
macro_rules! groups {
    ($( $(#[doc = $doc:literal])* $Group:ident($module:ident::$Request:ident, $Answer:ident), )*) => {
        $( pub mod $module; )*

        /// A request to remote control: one capability and its arguments.
        #[derive(Clone, Debug, PartialEq)]
        pub enum Request {
            $( $(#[doc = $doc])* $Group($module::$Request), )*
        }

        /// What a capability answers, by group and capability.
        #[derive(Clone, Debug, PartialEq, Serialize)]
        #[serde(untagged)]
        pub enum Answer {
            $( $Group($module::$Answer), )*
        }

        impl Request {
            /// The capability's full name, such as `army.move`.
            pub fn name(&self) -> String {
                match self {
                    $( Self::$Group(r) => full_name($module::GROUP, r.action()), )*
                }
            }

            /// Whether it runs only while a game is going on.
            pub fn in_game(&self) -> bool {
                match self {
                    $( Self::$Group(r) => r.in_game(), )*
                }
            }

            /// The request's arguments and its `"capability"` as the group sees it.
            fn inner(&self) -> serde_json::Result<Value> {
                match self {
                    $( Self::$Group(r) => serde_json::to_value(r), )*
                }
            }

            /// The request of `group` from its arguments and its own name.
            fn from_group<E: de::Error>(group: &str, fields: Map<String, Value>) -> Result<Self, E> {
                match group {
                    $( $module::GROUP => serde_json::from_value(Value::Object(fields)).map(Self::$Group).map_err(E::custom), )*
                    _ => Err(E::custom(format!("no group {group}"))),
                }
            }
        }

        /// Every capability remote control has, described, group by group.
        pub fn capabilities() -> Vec<Capability> {
            [$( $module::CAPABILITIES ),*].concat()
        }
    };
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
}

pub mod binary;
pub use binary::Binary;

fn full_name(group: &str, action: &str) -> String {
    if action.is_empty() {
        group.to_string()
    } else {
        format!("{group}.{action}")
    }
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
        let mut fields = Map::deserialize(deserializer)?;
        let name = match fields.get("capability") {
            Some(Value::String(name)) => name.clone(),
            Some(_) => return Err(de::Error::custom("the capability is not a text")),
            None => return Err(de::Error::custom("no capability named")),
        };
        let Some(known) = capabilities().into_iter().find(|c| c.name() == name) else {
            return Err(de::Error::custom(format!("no capability {name}")));
        };
        fields.insert("capability".into(), Value::String(known.action.into()));
        Self::from_group(known.group, fields)
    }
}

/// What kind of value an argument is, which says how to write it in JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A country tag, such as `"FRA"`.
    Country,
    /// A province number.
    Province,
    /// An army ID as a decimal string.
    Army,
    /// A date, such as `"1444.11.11"`.
    Date,
    Integer,
    Boolean,
    Text,
    /// A kind of regiment or ship, such as `"infantry"` (see
    /// [`provinces::UnitKind`]).
    UnitKind,
    /// Allies to call to arms: `"all"`, `"none"` or tags separated by
    /// commas, such as `"DUL,CND"` (see [`diplomacy::CallAllies`]).
    Allies,
}

impl Kind {
    /// Its name, as a description writes it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Country => "country",
            Self::Province => "province",
            Self::Army => "army",
            Self::Date => "date",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Text => "text",
            Self::UnitKind => "unit_kind",
            Self::Allies => "allies",
        }
    }

    /// A value of the kind, as text.
    pub fn example(self) -> &'static str {
        match self {
            Self::Country => "FRA",
            Self::Province => "151",
            Self::Army => "4294967377",
            Self::Date => "1444.11.11",
            Self::Integer => "3",
            Self::Boolean => "true",
            Self::Text => "text",
            Self::UnitKind => "infantry",
            Self::Allies => "DUL,CND",
        }
    }

    /// A value of the kind written as text, such as on a command line, in
    /// JSON: a number for a province or an integer, `true` or `false` for a
    /// boolean, the text itself for the rest. None when the text cannot be
    /// one; whether it names something that exists is the request's to say.
    pub fn json(self, text: &str) -> Option<Value> {
        match self {
            Self::Province | Self::Integer => serde_json::from_str::<serde_json::Number>(text).ok().filter(|n| n.is_i64() || n.is_u64()).map(Value::Number),
            Self::Boolean => text.parse().ok().map(Value::Bool),
            Self::Country | Self::Army | Self::Date | Self::Text => Some(Value::String(text.into())),
            Self::UnitKind => serde_json::from_value::<provinces::UnitKind>(Value::String(text.into())).ok().map(|_| Value::String(text.into())),
            Self::Allies => diplomacy::CallAllies::parse(text).ok().map(|_| Value::String(text.into())),
        }
    }
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

macro_rules! arg_kind {
    ($($ty:ty => $kind:ident),* $(,)?) => {
        $( impl ArgKind for $ty { const KIND: Kind = Kind::$kind; } )*
    };
}
arg_kind! {
    CountryTag => Country,
    ProvinceId => Province,
    ArmyId => Army,
    Date => Date,
    u32 => Integer,
    u64 => Integer,
    i32 => Integer,
    bool => Boolean,
    String => Text,
    provinces::UnitKind => UnitKind,
    diplomacy::CallAllies => Allies,
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
        let mut s = serializer.serialize_struct("Capability", 5)?;
        s.serialize_field("capability", &self.name())?;
        s.serialize_field("about", &prose(self.about))?;
        s.serialize_field("in_game", &self.in_game)?;
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

/// An army or a fleet, by its 64-bit ID, which survives saving and loading;
/// in JSON a decimal string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArmyId(pub u64);

impl Serialize for ArmyId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
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
    /// No game is going on to play: the main menu is open, or the player
    /// plays no country (observing). While a game is being set up or a save
    /// loads, a request that needs a game waits for one instead.
    NotInGame,
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
    pub const ALL: [Code; 9] = [
        Self::NotInGame,
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
