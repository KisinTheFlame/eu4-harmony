//! Provinces: every province, a province's neighbors, and one province's
//! snapshot.
use crate::{CountryTag, Date, ProvinceId, ProvinceList};
use serde::{Deserialize, Serialize};

group! {
    /// Provinces and their neighbors.
    "province": ProvinceRequest, ProvinceAnswer;

    /// Every province, or those named.
    game List "list" {
        /// Only these provinces, in their order; a number that names none is
        /// left out. Every province when it is left out.
        provinces: Option<ProvinceList>,
    } -> Vec<Province>, "the provinces, by number: id, kind (land, sea, lake or wasteland), visible (to the player, not under fog of war), owner and controller (null when none)";
    /// The provinces an army can go to from a province over land, possibly
    /// across a strait; none for a province an army cannot stand on.
    game Neighbors "neighbors" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Neighbor>, "its neighbors: province, and strait, whether it is across a strait";
    /// One province's snapshot: its name, owner and controller, development,
    /// terrain, fort and buildings, local autonomy, culture, religion,
    /// unrest, trade good, trade node, center of trade, trade company, state
    /// or territory and the state's edict, cores and claims, what is being
    /// built or done there, and the colony's size. In the player's view a
    /// province another country owns leaves out its unrest and what is
    /// being done there, as the province view does. What the player can
    /// recruit there is recruit.options'.
    game Get "get" {
        /// The province.
        province: ProvinceId,
    } -> Box<ProvinceSnapshot>, "its snapshot: id, name, owner, controller, development, terrain, fort, buildings, autonomy (percent), culture, religion and trade_good (their keys), unrest, trade_node (its key), center_of_trade (its level, 0 for none), trade_company, state (state or territory, null when it has no owner), edict (the state edict's key), cores, claims, constructions (each its kind, the game's key such as building_construction, the country, progress from 0 to 1, start and end dates, and the building or unit type it makes), missionary (a missionary's progress in percent, null when none works there) and colony_size (null for no colony); in the player's view unrest, constructions and missionary are null for a province another country owns";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Province {
    pub id: ProvinceId,
    pub kind: ProvinceKind,
    /// Not under fog of war; false while visibility is not current.
    pub visible: bool,
    pub owner: Option<CountryTag>,
    pub controller: Option<CountryTag>,
}

/// What a province is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum ProvinceKind {
    /// Land an army can stand on.
    Land,
    Sea,
    Lake,
    /// Land no army can enter.
    Wasteland,
}

/// A province next to another over land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Neighbor {
    pub province: ProvinceId,
    /// Across a strait.
    pub strait: bool,
}

/// All of one province at one moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ProvinceSnapshot {
    pub id: ProvinceId,
    /// Its name as the game shows it, in its own characters (the game keeps
    /// it as game text).
    pub name: String,
    pub owner: Option<CountryTag>,
    pub controller: Option<CountryTag>,
    pub development: Development,
    /// The key of its terrain, such as `farmlands`; null when it has none.
    pub terrain: Option<String>,
    pub fort: Fort,
    /// The keys of its buildings, such as `fort_15th` and `marketplace`.
    pub buildings: Vec<String>,
    /// Local autonomy as the province view shows it, in percent.
    pub autonomy: f64,
    /// The keys of its culture, religion and trade good, such as
    /// `castillian`, `catholic` and `wine`; null when it has none.
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub trade_good: Option<String>,
    /// Unrest as the province view shows it; null in the player's view for
    /// another country's province.
    pub unrest: Option<f64>,
    /// The key of its trade node, such as `sevilla`.
    pub trade_node: Option<String>,
    /// The level of its center of trade, 0 for none.
    pub center_of_trade: i32,
    /// It is in a trade company.
    pub trade_company: bool,
    /// Whether it is in a state of its owner's or a territory; null without
    /// an owner.
    pub state: Option<StateKind>,
    /// The key of the edict its state has, such as `edict_advancement_effort`.
    pub edict: Option<String>,
    /// The countries with a core on it.
    pub cores: Vec<CountryTag>,
    /// The countries with a claim on it, permanent or not.
    pub claims: Vec<CountryTag>,
    /// What is being built or done there; null in the player's view for
    /// another country's province, as the missionary.
    pub constructions: Option<Vec<Construction>>,
    /// The missionary's progress converting it, in percent; null when none
    /// works there.
    pub missionary: Option<f64>,
    /// The colony's size in settlers; null when it is no colony.
    pub colony_size: Option<f64>,
}

/// Whether a province is in a state or a territory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    State,
    Territory,
}

/// Something being built or done in a province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Construction {
    /// The game's key for what it is: `building_construction`,
    /// `military_construction` (regiments or ships), `missionary_construction`,
    /// `colony_construction`, `build_core_construction`,
    /// `change_culture_construction` and the others the game has.
    pub kind: String,
    /// Whose it is.
    pub country: Option<CountryTag>,
    /// From 0 to 1.
    pub progress: f64,
    pub start: Date,
    pub end: Date,
    /// For a building, its key; for regiments or ships, their unit type's.
    pub makes: Option<String>,
}

/// Base development, as the game shows it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Development {
    pub tax: f64,
    pub production: f64,
    pub manpower: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Fort {
    /// The fort level that defends the province now; 0 for none.
    pub level: i32,
    /// Its fort is mothballed: then only a capital's own level is left.
    pub mothballed: bool,
}

