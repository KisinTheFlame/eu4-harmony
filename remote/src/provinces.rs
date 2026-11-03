//! Provinces: every province, a province's neighbors, and one province's
//! snapshot.
use crate::{CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// Provinces and their neighbors.
    "province": ProvinceRequest, ProvinceAnswer;

    /// Every province.
    game List "list" {} -> Vec<Province>, "every province, by number: id, kind (land, sea, lake or wasteland), visible (to the player, not under fog of war), owner and controller (null when none)";
    /// The provinces an army can go to from a province over land, possibly
    /// across a strait; none for a province an army cannot stand on.
    game Neighbors "neighbors" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Neighbor>, "its neighbors: province, and strait, whether it is across a strait";
    /// One province's snapshot: its name, owner and controller, development,
    /// terrain, fort and buildings. What the player can recruit there is
    /// recruit.options'.
    game Get "get" {
        /// The province.
        province: ProvinceId,
    } -> ProvinceSnapshot, "its snapshot: id, name, owner, controller, development, terrain, fort, buildings";
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

