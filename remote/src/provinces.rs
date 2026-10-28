//! Provinces and their neighbors.
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
