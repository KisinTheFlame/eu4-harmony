//! Provinces: every province, a province's neighbors, and one province's
//! snapshot.
use crate::{CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// Provinces and their neighbors.
    "province": ProvinceRequest, ProvinceAnswer;

    /// Every province.
    game List "list" {} -> Vec<Province>, "every province, by number";
    /// The provinces an army can go to from a province over land, possibly
    /// across a strait; none for a province an army cannot stand on.
    game Neighbors "neighbors" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Neighbor>, "its neighbors";
    /// One province's snapshot: its name, owner and controller, development,
    /// terrain, fort, buildings, and whether the player can recruit each
    /// kind of regiment and ship there now, and why not.
    game Get "get" {
        /// The province.
        province: ProvinceId,
    } -> ProvinceSnapshot, "its snapshot: id, name, owner, controller, development, terrain, fort, buildings, recruit";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Province {
    pub id: ProvinceId,
    pub land: bool,
    pub lake: bool,
    pub impassable: bool,
    /// Not under fog of war; false while visibility is not current.
    pub visible: bool,
    pub owner: Option<CountryTag>,
    pub controller: Option<CountryTag>,
}

/// A province next to another over land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Neighbor {
    pub province: ProvinceId,
    /// Across a strait.
    pub strait: bool,
}

/// All of one province at one moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// One for each [`UnitKind`], in their order.
    pub recruit: Vec<Recruit>,
}

/// Base development, as the game shows it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Development {
    pub tax: f64,
    pub production: f64,
    pub manpower: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fort {
    /// The fort level that defends the province now; 0 for none.
    pub level: i32,
    /// Its fort is mothballed: then only a capital's own level is left.
    pub mothballed: bool,
}

/// A kind of regiment or ship, as the game sorts unit types: what a recruit
/// order names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Infantry,
    Cavalry,
    Artillery,
    HeavyShip,
    LightShip,
    Galley,
    Transport,
}

/// Whether the player can recruit one regiment or ship of a kind in the
/// province now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recruit {
    pub kind: UnitKind,
    /// The key of the unit type the player recruits of this kind, such as
    /// `western_medieval_infantry`; null when the player has none.
    pub unit_type: Option<String>,
    pub allowed: bool,
    /// Why not, in the game's own words, from its localisation for the
    /// language in the game's settings, which can differ from the player
    /// language; empty when it is allowed or the game gives no reason.
    pub reasons: Vec<String>,
}
