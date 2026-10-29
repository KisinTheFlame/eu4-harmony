//! The game as an extension sees it during a callback.
use crate::{abi, backend::Backend};
use serde::{de::DeserializeOwned, Serialize};

/// Full 64-bit army ID; survives save and load.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize)]
pub struct ArmyId(pub u64);

/// Country identity, stable within a session; compare for equality only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// RGBA color of one province on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapColor {
    pub province: ProvinceId,
    pub rgba: [u8; 4],
}

/// Where the map camera looks and how high it is. A map position is a pixel
/// of the province map (`map/provinces.bmp`): `x` from its left edge, `y`
/// from its bottom edge, as `map/positions.txt` writes them; a height is in
/// the same unit, above the map's base plane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

impl From<abi::Camera> for Camera {
    fn from(c: abi::Camera) -> Self {
        Self { x: c.x, y: c.y, height: c.height, province: (c.province != 0).then_some(c.province), min_height: c.min_height, max_height: c.max_height }
    }
}

/// All of one province at one moment.
#[derive(Clone, Debug, Default, PartialEq)]
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

/// [`ProvinceSnapshot`] as the core writes it (`abi::GameApi::province`).
#[derive(serde::Deserialize)]
pub(crate) struct ProvinceJson {
    id: ProvinceId,
    name: String,
    owner: Option<u64>,
    controller: Option<u64>,
    development: Development,
    terrain: Option<String>,
    fort: Fort,
    buildings: Vec<String>,
    recruit: Vec<Recruit>,
}

impl From<ProvinceJson> for ProvinceSnapshot {
    fn from(p: ProvinceJson) -> Self {
        Self {
            id: p.id,
            name: p.name,
            owner: p.owner.map(CountryId),
            controller: p.controller.map(CountryId),
            development: p.development,
            terrain: p.terrain,
            fort: p.fort,
            buildings: p.buildings,
            recruit: p.recruit,
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

impl<'a> Game<'a> {
    pub fn new(backend: &'a mut dyn Backend) -> Self {
        Self { backend }
    }

    pub fn session(&mut self) -> Result<Session> {
        self.backend.session()
    }
    /// Three-letter tag, for example `CAS`.
    pub fn country_tag(&mut self, country: CountryId) -> Result<String> {
        self.backend.country_tag(country)
    }
    /// At war with each other, or either is a rebel faction.
    pub fn countries_hostile(&mut self, a: CountryId, b: CountryId) -> Result<bool> {
        self.backend.countries_hostile(a, b)
    }
    /// Every province, indexed by ID (index 0 is a placeholder).
    pub fn provinces(&mut self) -> Result<Vec<Province>> {
        self.backend.provinces()
    }
    /// Land neighbors between passable land provinces.
    pub fn province_neighbors(&mut self, province: ProvinceId) -> Result<Vec<Neighbor>> {
        self.backend.province_neighbors(province)
    }
    pub fn army(&mut self, id: ArmyId) -> Result<Army> {
        self.backend.army(id)
    }
    /// All armies and fleets of the player.
    pub fn own_armies(&mut self) -> Result<Vec<Army>> {
        self.backend.own_armies()
    }
    /// Land armies in a visible province; empty under fog of war.
    pub fn province_armies(&mut self, province: ProvinceId) -> Result<Vec<Army>> {
        self.backend.province_armies(province)
    }
    pub fn army_route(&mut self, id: ArmyId) -> Result<Vec<ProvinceId>> {
        self.backend.army_route(id)
    }
    /// Military access and terrain; forts are not considered.
    pub fn army_can_enter(&mut self, id: ArmyId, province: ProvinceId) -> Result<bool> {
        self.backend.army_can_enter(id, province)
    }
    pub fn army_strait_open(&mut self, id: ArmyId, from: ProvinceId, to: ProvinceId) -> Result<bool> {
        self.backend.army_strait_open(id, from, to)
    }
    pub fn army_can_attack(&mut self, id: ArmyId, target: ArmyId, province: ProvinceId) -> Result<bool> {
        self.backend.army_can_attack(id, target, province)
    }
    /// The route the game would choose; `Ok(None)` when there is none.
    pub fn find_route(&mut self, id: ArmyId, goal: ProvinceId) -> Result<Option<Vec<ProvinceId>>> {
        match self.backend.find_route(id, goal) {
            Ok(route) => Ok(Some(route)),
            Err(e) if e.status == Status::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub fn army_move(&mut self, id: ArmyId, goal: ProvinceId) -> Result<()> {
        self.backend.army_move(id, goal)
    }
    pub fn army_stop(&mut self, id: ArmyId) -> Result<()> {
        self.backend.army_stop(id)
    }
    /// The own land army in the open single-army panel, if any.
    pub fn selected_army(&mut self) -> Result<Option<ArmyId>> {
        match self.backend.ui_selected_army() {
            Ok(id) => Ok(Some(id)),
            Err(e) if e.status == Status::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub fn set_visible(&mut self, element: &str, visible: bool) -> Result<()> {
        self.backend.ui_set_visible(element, visible)
    }
    pub fn set_enabled(&mut self, element: &str, enabled: bool) -> Result<()> {
        self.backend.ui_set_enabled(element, enabled)
    }
    pub fn set_text(&mut self, element: &str, text: &str) -> Result<()> {
        self.backend.ui_set_text(element, text)
    }
    pub fn set_tooltip(&mut self, element: &str, text: &str) -> Result<()> {
        self.backend.ui_set_tooltip(element, text)
    }
    /// Colors provinces, replacing this extension's previous coloring.
    pub fn paint_map(&mut self, colors: &[MapColor]) -> Result<()> {
        self.backend.map_paint(colors)
    }
    pub fn clear_map(&mut self) -> Result<()> {
        self.backend.map_clear()
    }
    /// This extension's data stored with the current save.
    pub fn save_data<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        let text = self.backend.save_data_get()?;
        if text.is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&text).map(Some).map_err(|e| Error::new(Status::InvalidArgument, format!("save data: {e}")))
    }
    /// Replaces this extension's data; written with the next save.
    pub fn set_save_data<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let text = serde_json::to_string(value).map_err(|e| Error::new(Status::InvalidArgument, e.to_string()))?;
        self.backend.save_data_set(&text)
    }
    /// The language texts are shown in this session, by its Paradox name
    /// (`english`, `simp_chinese`, ...).
    pub fn language(&mut self) -> Result<String> {
        self.backend.language()
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
    /// Where the map camera looks and how high it is.
    pub fn camera(&mut self) -> Result<Camera> {
        self.backend.camera()
    }
    /// Turns the camera to a province, at the position the game looks at it
    /// from, keeping the height; the next frame shows it. `Rejected` while
    /// the camera is locked.
    pub fn camera_look_at_province(&mut self, province: ProvinceId) -> Result<Camera> {
        self.backend.camera_look_at_province(province)
    }
    /// Turns the camera to a map position, keeping the height.
    pub fn camera_look_at(&mut self, x: u32, y: u32) -> Result<Camera> {
        self.backend.camera_look_at(x, y)
    }
    /// Raises or lowers the camera, looking at the same position.
    pub fn camera_zoom(&mut self, height: u32) -> Result<Camera> {
        self.backend.camera_zoom(height)
    }
    /// A province's snapshot: its name, development, terrain, fort,
    /// buildings, and what the player can recruit there now.
    pub fn province(&mut self, province: ProvinceId) -> Result<ProvinceSnapshot> {
        self.backend.province(province)
    }
    /// Runs a command in the game console, as the player would type it
    /// (`cash 100`), and returns what it printed. The game's own checks
    /// apply: a command only developers may run, almost any command in an
    /// ironman or multiplayer game, one the game console does not know and
    /// one that fails are [`Status::Rejected`], with what was printed as the
    /// message. `InvalidArgument` for an empty command or one with a `|` or a
    /// NUL; `Unavailable` on an EU4 Harmony older than this SDK.
    pub fn console(&mut self, command: &str) -> Result<String> {
        self.backend.console_run(command)
    }
}
