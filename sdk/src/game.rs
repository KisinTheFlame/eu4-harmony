//! The game as an extension sees it during a callback.
use crate::abi;
use serde::{de::DeserializeOwned, Serialize};

/// Full 64-bit army ID; survives save and load.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize)]
pub struct ArmyId(pub u64);

/// Country identity, stable within a session; compare for equality only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize)]
#[serde(transparent)]
pub struct CountryId(pub u64);

pub type ProvinceId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    NotInGame,
    NotReady,
    NotFound,
    Rejected,
    InvalidArgument,
    Unavailable,
    WrongContext,
    Internal,
}

impl Status {
    pub fn from_code(code: i32) -> Self {
        match code {
            abi::NOT_IN_GAME => Self::NotInGame,
            abi::NOT_READY => Self::NotReady,
            abi::NOT_FOUND => Self::NotFound,
            abi::REJECTED => Self::Rejected,
            abi::INVALID_ARGUMENT => Self::InvalidArgument,
            abi::UNAVAILABLE => Self::Unavailable,
            abi::WRONG_CONTEXT => Self::WrongContext,
            _ => Self::Internal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub status: Status,
    pub message: String,
}

impl Error {
    pub fn new(status: Status, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.status, self.message)
    }
}
impl std::error::Error for Error {}

/// A refusal of the framework's own checks, as the core gives it; the fake
/// game refuses through the same checks.
impl From<eu4_harmony_checks::Refusal> for Error {
    fn from(refusal: eu4_harmony_checks::Refusal) -> Self {
        use eu4_harmony_checks::Status as Checked;
        let status = match refusal.status {
            Checked::NotReady => Status::NotReady,
            Checked::NotFound => Status::NotFound,
            Checked::Rejected => Status::Rejected,
            Checked::InvalidArgument => Status::InvalidArgument,
        };
        Self::new(status, refusal.message)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// Changes whenever a different campaign state is loaded or started.
    pub id: u64,
    /// The same value throughout one game day.
    pub date: i32,
    pub paused: bool,
    /// Visibility (fog of war) reflects the current date.
    pub visibility_current: bool,
    pub player: CountryId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Province {
    pub id: ProvinceId,
    pub land: bool,
    pub lake: bool,
    pub impassable: bool,
    /// Not under fog of war; false while visibility is not current.
    pub visible: bool,
    pub owner: Option<CountryId>,
    pub controller: Option<CountryId>,
}

impl Province {
    /// Land an army can stand on.
    pub fn passable_land(&self) -> bool {
        self.id > 0 && self.land && !self.lake && !self.impassable
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Neighbor {
    pub to: ProvinceId,
    pub strait: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteSource {
    #[default]
    None,
    /// The player or the game.
    External,
    /// This extension, unchanged since.
    Own,
    OtherExtension,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Army {
    pub id: ArmyId,
    pub owner: CountryId,
    pub province: ProvinceId,
    pub land: bool,
    pub in_battle: bool,
    pub sieging: bool,
    pub retreating: bool,
    pub exiled: bool,
    pub movement_locked: bool,
    pub attached: bool,
    pub has_followers: bool,
    /// Running a native task, for example hunting rebels.
    pub busy: bool,
    pub regiments: u32,
    pub route_len: u32,
    pub destination: Option<ProvinceId>,
    pub route_source: RouteSource,
}

impl Army {
    /// Nothing about its state stops the game from taking a new order.
    pub fn can_take_orders(&self) -> bool {
        self.land
            && self.regiments > 0
            && !(self.in_battle || self.sieging || self.retreating || self.exiled || self.movement_locked || self.attached || self.has_followers || self.busy)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Click {
    pub right: bool,
    pub shift: bool,
    pub ctrl: bool,
}

/// A question waiting for the player's answer, while it waits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize)]
#[serde(transparent)]
pub struct QuestionId(pub u32);

/// A question: one of the game's events waiting for the player's answer, as
/// the core writes it (`abi::GameApi::questions`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Question {
    pub id: QuestionId,
    /// The event's id in the game's files, such as `flavor_fra.1`.
    pub event: String,
    pub title: String,
    pub description: String,
    /// The last day to answer, as a game date; after it the game picks an
    /// option itself.
    pub deadline: i32,
    /// The options the player may choose, in the game's order.
    pub options: Vec<QuestionOption>,
}

/// An option of a question the player may choose.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct QuestionOption {
    /// Its number among all the event's options, which answering takes.
    pub index: u32,
    pub text: String,
}

/// An offer waiting for the player's answer, while it waits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize)]
#[serde(transparent)]
pub struct OfferId(pub u32);

/// An offer: another country's proposal waiting for the player's answer, as
/// the core writes it (`abi::GameApi::offers`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Offer {
    pub id: OfferId,
    /// The country that made it.
    pub from: CountryId,
    /// When it was made, as a game date.
    pub date: i32,
    /// What is offered, such as "Royal Marriage".
    pub title: String,
    pub description: String,
}

/// RGBA color of one province on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapColor {
    pub province: ProvinceId,
    pub rgba: [u8; 4],
}

/// Where the map camera looks and how high it is. A map position is a pixel
/// of the province map (`map/provinces.bmp`): `x` from its left edge, `y`
/// from its bottom edge, as `map/positions.txt` writes them; a height is in
/// the same unit, above the map's base plane. Its default is the unmodded
/// game's heights, 50 to 3000, at 1000, looking at the map's corner, as
/// [`FakeGame`](crate::testing::FakeGame)'s camera starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Camera {
    /// The map position at the middle of the view.
    pub x: u32,
    pub y: u32,
    pub height: u32,
    /// The province at that position.
    pub province: Option<ProvinceId>,
    /// The lowest and the highest the camera may be; over high ground the
    /// game keeps it higher than the lowest.
    pub min_height: u32,
    pub max_height: u32,
}

impl Default for Camera {
    fn default() -> Self {
        // `NFrontend.CAMERA_MIN_HEIGHT` and `CAMERA_MAX_HEIGHT` in the
        // unmodded game's `common/defines.lua`.
        Self { x: 0, y: 0, height: 1000, province: None, min_height: 50, max_height: 3000 }
    }
}

impl From<abi::Camera> for Camera {
    fn from(c: abi::Camera) -> Self {
        Self { x: c.x, y: c.y, height: c.height, province: (c.province != 0).then_some(c.province), min_height: c.min_height, max_height: c.max_height }
    }
}

/// All of one province at one moment, as the core writes it
/// (`abi::GameApi::province`).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct ProvinceSnapshot {
    pub id: ProvinceId,
    /// Its name as the game shows it.
    pub name: String,
    pub owner: Option<CountryId>,
    pub controller: Option<CountryId>,
    pub development: Development,
    /// The key of its terrain, such as `farmlands`.
    pub terrain: Option<String>,
    pub fort: Fort,
    /// The keys of its buildings, such as `fort_15th` and `marketplace`.
    pub buildings: Vec<String>,
    /// One for each kind of unit the core knows, in [`UnitKind`]'s order; a
    /// kind added after this SDK comes as [`UnitKind::Unknown`].
    pub recruit: Vec<Recruit>,
}

/// Base development, as the game shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize)]
pub struct Development {
    pub tax: f64,
    pub production: f64,
    pub manpower: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Fort {
    /// The fort level that defends the province now; 0 for none.
    pub level: i32,
    /// Mothballed: only a capital's own level is left.
    pub mothballed: bool,
}

/// A kind of regiment or ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Infantry,
    Cavalry,
    Artillery,
    HeavyShip,
    LightShip,
    Galley,
    Transport,
    /// A kind a newer core names that this SDK does not know (ADR 0018).
    #[serde(other)]
    Unknown,
}

/// Whether the player can recruit one regiment or ship of a kind in a
/// province now.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Recruit {
    pub kind: UnitKind,
    /// The key of the unit type the player recruits of this kind, such as
    /// `western_medieval_infantry`; None when the player has none.
    pub unit_type: Option<String>,
    pub allowed: bool,
    /// Why not, in the game's words, from its localisation for the language
    /// in the game's settings, which can differ from the player language;
    /// empty when it is allowed or the game gives no reason.
    pub reasons: Vec<String>,
}

/// A country at one moment, as the core writes it (`abi::GameApi::country`).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct Country {
    pub id: CountryId,
    /// Ducats, to the thousandth.
    pub treasury: f64,
    /// Men.
    pub manpower: i32,
    pub max_manpower: i32,
    /// From -3 to 3, to the thousandth.
    pub stability: f64,
    /// From -100 to 100, to the thousandth.
    pub prestige: f64,
    pub power: Power,
    /// The wars going on that it takes part in, on either side.
    pub wars: Vec<War>,
    /// Its allies, its subjects, its overlord and the countries it rivals,
    /// in that order; a relation of a kind added after this SDK comes as
    /// [`RelationKind::Unknown`].
    pub relations: Vec<Relation>,
}

impl Country {
    /// `other` is this to the country. [`RelationKind::Unknown`] asks whether
    /// it is anything this SDK does not know.
    pub fn has_relation(&self, kind: RelationKind, other: CountryId) -> bool {
        self.relations.iter().any(|r| r.kind == kind && r.country == other)
    }

    /// The two are on opposite sides of one of the country's wars.
    pub fn at_war_with(&self, other: CountryId) -> bool {
        self.wars.iter().any(|w| {
            let (ours, theirs) = if w.attackers.contains(&self.id) { (&w.attackers, &w.defenders) } else { (&w.defenders, &w.attackers) };
            ours.contains(&self.id) && theirs.contains(&other)
        })
    }
}

/// Monarch points of each kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Power {
    pub adm: i32,
    pub dip: i32,
    pub mil: i32,
}

/// A war going on.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct War {
    /// Its name, as the game shows it.
    pub name: String,
    /// Their leader first.
    pub attackers: Vec<CountryId>,
    /// Their leader first.
    pub defenders: Vec<CountryId>,
}

/// What another country is to a country.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Relation {
    pub kind: RelationKind,
    pub country: CountryId,
    /// For a subject or an overlord, the kind of subject the subject is: the
    /// key `common/subject_types` gives it (`vassal`, `march`,
    /// `personal_union`, `tributary_state`, or one a mod adds). None for the
    /// other kinds, and from a core older than subject types.
    #[serde(default)]
    pub subject_type: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Ally,
    /// The other is the country's subject, of the relation's subject type.
    Subject,
    /// The other is the country's overlord, and the country is the
    /// relation's subject type to it.
    Overlord,
    /// The country has chosen the other as a rival.
    Rival,
    /// A kind a newer core names that this SDK does not know (ADR 0018).
    #[serde(other)]
    Unknown,
}

/// A war goal the game offers the player for declaring war on a country, as
/// the declare war window lists it ([`Game::war_goals`]), and as the core
/// writes it (`abi::GameApi::war_goals`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct WarGoal {
    /// Its casus belli's key, such as `cb_claim`; None for declaring war
    /// without one, which the core names [`abi::NO_CASUS_BELLI`].
    #[serde(deserialize_with = "casus_belli")]
    pub casus_belli: Option<String>,
    /// Its type's key, such as `take_claim`.
    pub war_goal: Option<String>,
    /// The province it is about, if any.
    pub province: Option<ProvinceId>,
    /// The country it is about, if any.
    pub target: Option<CountryId>,
    /// Its name, as the window shows it.
    pub name: String,
}

/// How the core names a declaration of war without a casus belli.
pub(crate) use abi::NO_CASUS_BELLI;

/// A war goal's casus belli as the core writes it: its key, or
/// [`NO_CASUS_BELLI`] for none.
fn casus_belli<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<String>, D::Error> {
    let key = <String as serde::Deserialize>::deserialize(deserializer)?;
    Ok((key != NO_CASUS_BELLI).then_some(key))
}

/// A diplomatic action sent, for [`Game::diplomacy_answer`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticket(pub u64);

/// Which of the player's allies a declaration of war calls to arms
/// ([`Game::declare_war`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CallAllies {
    /// Each ally the declare war window would let the player call.
    #[default]
    All,
    /// None.
    None,
    /// These allies, each one the window must let the player call.
    Only(Vec<CountryId>),
}

/// A declaration of war sent ([`Game::declare_war`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declaration {
    pub ticket: Ticket,
    /// The allies it called to arms.
    pub called: Vec<CountryId>,
}

/// What came of a diplomatic action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiplomacyAnswer {
    /// Not yet: the game has not carried it out, or the other country has
    /// not answered.
    Pending,
    /// The other country accepted the proposal.
    Accepted,
    /// The other country declined it.
    Declined,
    /// The war began.
    Declared,
    /// An answer a newer core gives that this SDK does not know.
    Unknown(u32),
}

impl DiplomacyAnswer {
    pub fn from_code(code: u32) -> Self {
        match code {
            abi::DIPLOMACY_PENDING => Self::Pending,
            abi::DIPLOMACY_ACCEPTED => Self::Accepted,
            abi::DIPLOMACY_DECLINED => Self::Declined,
            abi::DIPLOMACY_DECLARED => Self::Declared,
            other => Self::Unknown(other),
        }
    }
}

impl From<abi::Session> for Session {
    fn from(s: abi::Session) -> Self {
        Self { id: s.session, date: s.date, paused: s.paused != 0, visibility_current: s.visibility_current != 0, player: CountryId(s.player) }
    }
}

impl From<abi::Province> for Province {
    fn from(p: abi::Province) -> Self {
        Self {
            id: p.id,
            land: p.flags & abi::PROVINCE_LAND != 0,
            lake: p.flags & abi::PROVINCE_LAKE != 0,
            impassable: p.flags & abi::PROVINCE_IMPASSABLE != 0,
            visible: p.flags & abi::PROVINCE_VISIBLE != 0,
            owner: (p.owner != 0).then_some(CountryId(p.owner)),
            controller: (p.controller != 0).then_some(CountryId(p.controller)),
        }
    }
}

impl From<abi::Neighbor> for Neighbor {
    fn from(n: abi::Neighbor) -> Self {
        Self { to: n.to, strait: n.flags & abi::NEIGHBOR_STRAIT != 0 }
    }
}

impl From<abi::Army> for Army {
    fn from(a: abi::Army) -> Self {
        let f = |bit| a.flags & bit != 0;
        Self {
            id: ArmyId(a.id),
            owner: CountryId(a.owner),
            province: a.province,
            land: f(abi::ARMY_LAND),
            in_battle: f(abi::ARMY_IN_BATTLE),
            sieging: f(abi::ARMY_SIEGING),
            retreating: f(abi::ARMY_RETREATING),
            exiled: f(abi::ARMY_EXILED),
            movement_locked: f(abi::ARMY_MOVEMENT_LOCKED),
            attached: f(abi::ARMY_ATTACHED),
            has_followers: f(abi::ARMY_HAS_FOLLOWERS),
            busy: f(abi::ARMY_BUSY),
            regiments: a.regiments,
            route_len: a.route_len,
            destination: (a.destination != 0).then_some(a.destination),
            route_source: match a.route_source {
                abi::ROUTE_EXTERNAL => RouteSource::External,
                abi::ROUTE_OWN => RouteSource::Own,
                abi::ROUTE_OTHER_EXTENSION => RouteSource::OtherExtension,
                _ => RouteSource::None,
            },
        }
    }
}

/// Access to the game during one callback.
pub struct Game<'a> {
    backend: &'a mut dyn Backend,
}

pub(crate) mod sealed {
    /// Only this crate's backends implement [`Backend`](super::Backend), so
    /// that a capability added to it changes no extension (ADR 0014).
    pub trait Sealed {}
}

/// Declares every capability once. Those in `game` are a [`Backend`] method
/// and the [`Game`] method that calls it, under one name and one
/// description; those in `backend` are only the `Backend` method, under a
/// `Game` method of their own that adds to it.
macro_rules! capabilities {
    (
        game { $( $(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty; )* }
        backend { $( $(#[$bdoc:meta])* fn $bname:ident($($barg:ident: $bty:ty),* $(,)?) -> $bret:ty; )* }
    ) => {
        /// The capabilities behind a [`Game`]: the core's table for the
        /// callback, or [`FakeGame`](crate::testing::FakeGame). Only this
        /// crate implements it.
        pub trait Backend: sealed::Sealed {
            $( $(#[$doc])* fn $name(&mut self, $($arg: $ty),*) -> Result<$ret>; )*
            $( $(#[$bdoc])* fn $bname(&mut self, $($barg: $bty),*) -> Result<$bret>; )*
        }

        impl Game<'_> {
            $( $(#[$doc])* pub fn $name(&mut self, $($arg: $ty),*) -> Result<$ret> { self.backend.$name($($arg),*) } )*
        }
    };
}

capabilities! {
    game {
        fn session() -> Session;
        /// Three-letter tag, for example `CAS`.
        fn country_tag(country: CountryId) -> String;
        /// At war with each other, or either is a rebel faction.
        fn countries_hostile(a: CountryId, b: CountryId) -> bool;
        /// Every province, indexed by ID (index 0 is a placeholder).
        fn provinces() -> Vec<Province>;
        /// The passable land provinces next to a province; none for a
        /// province an army cannot stand on.
        fn province_neighbors(province: ProvinceId) -> Vec<Neighbor>;
        /// One of the player's own armies or fleets.
        fn army(id: ArmyId) -> Army;
        /// All armies and fleets of the player.
        fn own_armies() -> Vec<Army>;
        /// Land armies in a visible province; empty under fog of war.
        /// `NotReady` while visibility is not current.
        fn province_armies(province: ProvinceId) -> Vec<Army>;
        /// The provinces on an own land army's current route, in order.
        fn army_route(id: ArmyId) -> Vec<ProvinceId>;
        /// Whether an own land army may enter a province now: military access
        /// and terrain; forts are not considered.
        fn army_can_enter(id: ArmyId, province: ProvinceId) -> bool;
        /// Whether the strait between two neighboring provinces is open to an
        /// own land army.
        fn army_strait_open(id: ArmyId, from: ProvinceId, to: ProvinceId) -> bool;
        /// Whether an own land army may attack `target`, an army the player
        /// sees in `province`. `NotReady` while visibility is not current;
        /// `NotFound` for a target not seen there.
        fn army_can_attack(id: ArmyId, target: ArmyId, province: ProvinceId) -> bool;
        /// Orders an own land army along the game's route to `goal`. Done
        /// means the route is installed, not that the army has arrived.
        /// `NotReady` while the game is paused; `Rejected` for an army the
        /// game would not let move ([`Army::can_take_orders`]); `NotFound`
        /// when there is no route.
        fn army_move(id: ArmyId, goal: ProvinceId) -> ();
        /// Stops an own land army where it is; refused as
        /// [`army_move`](Self::army_move) is.
        fn army_stop(id: ArmyId) -> ();
        /// Shows or hides an element of this mod's interface.
        fn set_visible(element: &str, visible: bool) -> ();
        /// Enables or disables a button of this mod's interface.
        fn set_enabled(element: &str, enabled: bool) -> ();
        fn set_text(element: &str, text: &str) -> ();
        fn set_tooltip(element: &str, text: &str) -> ();
        /// Colors provinces, replacing this extension's previous coloring.
        fn paint_map(colors: &[MapColor]) -> ();
        fn clear_map() -> ();
        /// The language texts are shown in this session, by its Paradox name
        /// (`english`, `simp_chinese`, ...).
        fn language() -> String;
        /// Where the map camera looks and how high it is.
        fn camera() -> Camera;
        /// Turns the camera to a province, at the position the game looks at
        /// it from, keeping the height; the next frame shows it. `Rejected`
        /// while the camera is locked.
        fn camera_look_at_province(province: ProvinceId) -> Camera;
        /// Turns the camera to a map position, keeping the height.
        fn camera_look_at(x: u32, y: u32) -> Camera;
        /// Raises or lowers the camera, looking at the same position.
        fn camera_zoom(height: u32) -> Camera;
        /// A province's snapshot: its name, development, terrain, fort,
        /// buildings, and what the player can recruit there now.
        fn province(province: ProvinceId) -> ProvinceSnapshot;
        /// Runs a command in the game console, as the player would type it
        /// (`cash 100`), and returns what it printed. The game's own checks
        /// apply: a command only developers may run, almost any command in an
        /// ironman or multiplayer game, one the game console does not know and
        /// one that fails are [`Status::Rejected`], with what was printed as
        /// the message. `InvalidArgument` for an empty command or one with a
        /// `|` or a NUL; `Unavailable` on an EU4 Harmony older than this SDK.
        fn console(command: &str) -> String;
        /// One read of a country: its treasury, manpower, stability, prestige,
        /// monarch points, wars and diplomatic relations.
        fn country(country: CountryId) -> Country;
        /// The game speed, from 1 (slowest) to 5 (fastest). Whether the game
        /// is paused is in the [`session`](Self::session).
        fn speed() -> u32;
        /// Sets the game speed, from 1 to 5, as the player would; the game
        /// stays paused or running.
        fn set_speed(speed: u32) -> ();
        /// Pauses the game with the player's own pause, as the player would,
        /// also when windows hold it paused already, so that it stays paused
        /// once they close; nothing when the player's pause holds it already.
        fn pause() -> ();
        /// Resumes the game as the player would, also when windows hold it
        /// paused (they stay open); nothing when it runs. `Rejected` when the
        /// game stays paused, and once the game is over: past its end date,
        /// unless the player chose to play on, as the player's resuming is
        /// refused.
        fn resume() -> ();
        /// Recruits one regiment or ship of `kind` in a province for the
        /// player, through the game's own recruit command, as the recruit
        /// button in the province view does: the player's unit type of that
        /// kind, at the game's cost. Done once the game has the order; it
        /// carries it out at its next frame, checking it again then, and drops
        /// one that no longer passes with only a line in its own log: several
        /// orders in one callback are each checked against the same treasury,
        /// manpower and sailors, and those the first ones leave too little for
        /// are dropped at that frame. `Rejected` with the game's reasons, one
        /// a line, when the game would not let the player recruit it there
        /// now. The province's [`Recruit`] says so beforehand, but for one rule
        /// of the recruit command's on ships, a restriction it asks of the
        /// player where the snapshot asks it of the province's owner: a ship
        /// the snapshot allows can still be refused, with no reason.
        /// `InvalidArgument` for [`UnitKind::Unknown`].
        fn recruit(province: ProvinceId, kind: UnitKind) -> ();
        /// The questions waiting for the player's answer, oldest first: the
        /// game's events shown in event windows.
        fn questions() -> Vec<Question>;
        /// Answers a question with one of its options, as the player does in
        /// its event window; the game applies it a moment later.
        fn answer_question(question: QuestionId, option: u32) -> ();
        /// The offers waiting for the player's answer, made no more than two
        /// months ago: the game takes no answer to an older one.
        fn offers() -> Vec<Offer>;
        /// The war goals the game offers the player for declaring war on
        /// `country`, as the declare war window lists them: under each casus
        /// belli the player has against it, each war goal it allows, and last,
        /// when the game offers it, declaring war without a casus belli.
        fn war_goals(country: CountryId) -> Vec<WarGoal>;
        /// Declares war on `country` for `goal`, one of those
        /// [`war_goals`](Self::war_goals) lists, as the player does through the
        /// diplomacy panel, so that the game's checks and costs apply. It calls
        /// to arms the allies `call` names, as ticking their call boxes in the
        /// declare war window does, with no land promised; the answer says
        /// which it called. Unlike remote control's answer, it does not list
        /// the allies passed over, or why (`passed_over`): to learn why the
        /// window would not let the player call an ally, name it
        /// ([`CallAllies::Only`]), which is refused with the reasons.
        /// `InvalidArgument` for an ally to call that the window does not list;
        /// `Rejected`, with the game's reasons and nothing sent, for one it
        /// would not let the player call. A costly declaration (no casus belli,
        /// a truce broken) is sent without the window's confirmation, and the
        /// game takes its costs.
        /// [`Status::Rejected`] when the game offers no such war goal or does
        /// not let the player declare war now, with the game's reasons, one a
        /// line. The game carries the declaration out at its next frame;
        /// [`diplomacy_answer`](Self::diplomacy_answer) then says whether the
        /// war began.
        fn declare_war(country: CountryId, goal: &WarGoal, call: &CallAllies) -> Declaration;
        /// Offers `country`, which the player is at war with, a white peace in
        /// the war between them, as the peace view sends one; refused, with the
        /// game's reasons, when the game does not let the player. An AI country
        /// answers a frame or two later: see
        /// [`diplomacy_answer`](Self::diplomacy_answer).
        fn offer_peace(country: CountryId) -> Ticket;
        /// Proposes an alliance to `country`, as
        /// [`offer_peace`](Self::offer_peace) offers peace.
        fn propose_alliance(country: CountryId) -> Ticket;
        /// What came of a diplomatic action sent: pending until the game has
        /// carried it out and the other country has answered, which in a single
        /// player game is a frame or two later. A proposal the game drops as it
        /// carries it out, its own check failing then, stays pending: the game
        /// says nothing of it. `NotFound` for a ticket no longer kept (the
        /// latest 256 are), `NotInGame` once the game it was sent in has ended,
        /// `Rejected` for a declaration the game carried out without a war
        /// beginning.
        fn diplomacy_answer(ticket: Ticket) -> DiplomacyAnswer;
    }
    backend {
        /// The route the game itself would choose for an own land army; empty
        /// when `goal` is the army's province. `NotFound` when there is none,
        /// and for an army that is not the player's own.
        fn find_route(id: ArmyId, goal: ProvinceId) -> Vec<ProvinceId>;
        /// `NotFound` when no own land army is shown.
        fn ui_selected_army() -> ArmyId;
        /// JSON text; empty when there is none.
        fn save_data_get() -> String;
        fn save_data_set(json: &str) -> ();
        /// This mod's text for `key`; `NotFound` when it has none.
        fn text(key: &str) -> String;
        fn answer_offer(offer: OfferId, accept: bool) -> ();
    }
}

impl<'a> Game<'a> {
    pub fn new(backend: &'a mut dyn Backend) -> Self {
        Self { backend }
    }

    /// The route the game would choose; `Ok(None)` when there is none.
    /// `NotFound` for an army that is not one of the player's own.
    pub fn find_route(&mut self, id: ArmyId, goal: ProvinceId) -> Result<Option<Vec<ProvinceId>>> {
        match self.backend.find_route(id, goal) {
            Ok(route) => Ok(Some(route)),
            // The game says NotFound for an army that is not the player's,
            // too: only the player's own army has no route.
            Err(e) if e.status == Status::NotFound => {
                if self.backend.own_armies()?.iter().any(|a| a.id == id) {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }
    /// The own land army in the open single-army panel, if any.
    pub fn selected_army(&mut self) -> Result<Option<ArmyId>> {
        match self.backend.ui_selected_army() {
            Ok(id) => Ok(Some(id)),
            Err(e) if e.status == Status::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    /// This extension's data in the current save: what it set last in this
    /// session, or else what the save it was loaded from held.
    pub fn save_data<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        let text = self.backend.save_data_get()?;
        if text.is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&text).map(Some).map_err(|e| Error::new(Status::InvalidArgument, format!("save data: {e}")))
    }
    /// Replaces this extension's data, which every save from now on carries.
    /// More than 16 MiB of JSON is refused ([`Status::InvalidArgument`]).
    pub fn set_save_data<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let text = serde_json::to_string(value).map_err(|e| Error::new(Status::InvalidArgument, e.to_string()))?;
        self.backend.save_data_set(&text)
    }
    /// This mod's text for `key`, from its `eu4-harmony/localisation/` files,
    /// in the player language or else in English. As in the game, a key
    /// without a text shows as itself.
    pub fn text(&mut self, key: &str) -> String {
        self.backend.text(key).unwrap_or_else(|_| key.to_string())
    }
    /// [`text`](Self::text) with each `$NAME$` replaced by its value.
    pub fn text_with(&mut self, key: &str, args: &[(&str, &str)]) -> String {
        eu4_harmony_loc::fill(&self.text(key), args)
    }
    /// Accepts an offer, as the player does in its window. `Rejected` when the
    /// game no longer takes an answer to it (it was made more than two months
    /// ago, or what it offers is no longer possible), which the game then
    /// lets go, mostly declining it, as when the player opens it.
    pub fn accept_offer(&mut self, offer: OfferId) -> Result<()> {
        self.backend.answer_offer(offer, true)
    }
    /// Declines an offer, as the player does in its window; `Rejected` as
    /// [`Game::accept_offer`] is.
    pub fn decline_offer(&mut self, offer: OfferId) -> Result<()> {
        self.backend.answer_offer(offer, false)
    }
}

/// How a backend refuses [`UnitKind::Unknown`], which no core takes.
pub(crate) fn unknown_unit_kind() -> Error {
    Error::new(Status::InvalidArgument, "a unit kind this SDK does not know cannot be recruited")
}
