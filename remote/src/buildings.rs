//! Buildings: what the province view's building panel offers the player in a
//! province, and building, cancelling and destroying one there.
use crate::{choices::Choice, CountryTag, Date, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The buildings of the player's provinces.
    "building": BuildingRequest, BuildingAnswer;

    /// What the building panel offers the player in a province now: for
    /// each building the game has, building it, or destroying it where it
    /// stands, with whether the player can, the game's reasons why not and
    /// the cost; cancelling what is being built; and what is being built.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> BuildingOptions, "province; constructing (building, progress from 0 to 1, finished, builder; null when nothing is being built); options: action (build, cancel or destroy), building, can, reasons, cost";
    /// Starts building a building in the province, for ducats, as the
    /// building panel's button does. Refused with the game's reasons, one a
    /// line, as building.options gives them.
    game Build "build" {
        /// The province.
        province: ProvinceId,
        /// The building's content key, such as `marketplace`.
        building: String,
    } -> (), "null once the building is being built";
    /// Cancels the building the player is building in the province, as the
    /// building panel's button does; the game gives back what it gives back.
    game Cancel "cancel" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once done";
    /// Destroys a building that stands in one of the player's provinces, as
    /// the building panel's button does once the player confirms.
    game Destroy "destroy" {
        /// The province.
        province: ProvinceId,
        /// The building's content key.
        building: String,
    } -> (), "null once done";
}

/// What the building panel offers in one province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct BuildingOptions {
    pub province: ProvinceId,
    /// The building being built there, by whoever builds it; null when none.
    pub constructing: Option<BuildingConstruction>,
    /// One for each building the game has: `build` where it does not stand,
    /// `destroy` where it does; and `cancel` while one is being built.
    pub options: Vec<BuildingOption>,
}

/// A building being built.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct BuildingConstruction {
    /// Its content key.
    pub building: String,
    /// How far it is, from 0 to 1.
    pub progress: f64,
    /// The day it is to be finished.
    pub finished: Date,
    /// Who builds it.
    pub builder: CountryTag,
}

/// One thing the building panel offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct BuildingOption {
    /// The capability of this group that does it: `build`, `cancel` or
    /// `destroy`.
    pub action: String,
    /// The building's content key.
    pub building: String,
    #[serde(flatten)]
    pub choice: Choice,
}
