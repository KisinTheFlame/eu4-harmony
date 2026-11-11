//! Monuments: the great projects in the player's provinces, monuments and
//! canals, as the monuments window lists them, and what its buttons do:
//! upgrading, building, moving, cancelling and boosting.
use crate::{choices::Choice, Date, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The monuments window: the player's monuments and canals.
    "monument": MonumentRequest, MonumentAnswer;

    /// The great projects in the player's provinces, monuments and canals,
    /// in the order the game keeps them: where each stands, its tier, and
    /// what upgrading it offers now, with whether the player can, the
    /// game's reasons why not and its cost in ducats.
    game List "list" {} -> Vec<MonumentOption>, "the monuments: monument (its content key), name, kind (monument or canal), province, tier, can, reasons, cost";

    /// Starts upgrading a monument in one of the player's provinces to its
    /// next tier, as the monuments window's button does, paying its cost in
    /// ducats. `rejected` with the game's reasons when the game would not
    /// let the player upgrade it now, as `monument.list` says beforehand;
    /// `not_found` for a key that names no great project in the player's
    /// provinces.
    game Upgrade "upgrade" {
        /// The great project's content key, such as `great_wall_of_china`.
        monument: String,
    } -> (), "null once the upgrade has begun";

    /// The great projects the world does not have yet that the monuments
    /// window offers to build in a province, such as a canal at its site:
    /// each one whose site is that province or which may stand anywhere,
    /// with whether the player can build it there now, the game's reasons
    /// why not and its cost in ducats.
    game Buildable "buildable" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Buildable>, "the great projects: monument (its content key), name, kind, can, reasons, cost";

    /// Starts building a great project the world does not have yet in a
    /// province, as the monuments window's button does, paying its cost in
    /// ducats. `rejected` with the game's reasons, as `monument.buildable`
    /// gives them beforehand; `not_found` for a key that names no great
    /// project, or one the world has.
    game Build "build" {
        /// The great project's content key, such as `kiel_canal`.
        monument: String,
        /// The province to build it in: its site, for one that has one.
        province: ProvinceId,
    } -> (), "null once the construction has begun";

    /// Whether the player can move one of their monuments to a province
    /// now, as the monuments window's move button says: the game's reasons
    /// why not (too big to move, already moving, no route there, not the
    /// player's province) and the cost in ducats, by the distance.
    game EvaluateMove "evaluate_move" {
        /// The great project's content key.
        monument: String,
        /// The province to move it to.
        province: ProvinceId,
    } -> Choice, "can, reasons, cost";

    /// Starts moving one of the player's monuments to a province, as the
    /// monuments window's move button does, paying its cost in ducats.
    /// `rejected` with the game's reasons, as `monument.evaluate_move`
    /// gives them beforehand; `not_found` for a key that names no great
    /// project in the player's provinces.
    game Move "move" {
        /// The great project's content key.
        monument: String,
        /// The province to move it to.
        province: ProvinceId,
    } -> (), "null once the move has begun";

    /// The player's great projects under construction (being built,
    /// upgraded or moved), each with the day it ends, and what the window's
    /// buttons offer for it: cancelling it, and bringing it forward with
    /// manpower or with ducats, each with whether the player can, why not
    /// and its cost.
    game Constructions "constructions" {} -> Vec<MonumentConstruction>, "the constructions: monument, name, province, start, end, cancel, boost_manpower, boost_ducats";

    /// Cancels a great project's construction of the player's, as the
    /// window's cancel button does; the game returns part of what it cost.
    /// `not_found` for a key that names none of the player's constructions.
    game Cancel "cancel" {
        /// The great project's content key.
        monument: String,
    } -> (), "null once it is cancelled";

    /// Brings a great project's construction of the player's forward, as
    /// the window's boost buttons do: with manpower or with ducats, paying
    /// the game's amount for the days the game gives. `rejected` with the
    /// game's reasons, as `monument.constructions` gives them beforehand.
    game Boost "boost" {
        /// The great project's content key.
        monument: String,
        /// What to pay with: `manpower` or `ducats`.
        with: String,
    } -> (), "null once it is brought forward";
}

/// A great project the world does not have yet, and building it in a
/// province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Buildable {
    /// Its content key.
    pub monument: String,
    /// Its name, in the player language.
    pub name: String,
    pub kind: MonumentKind,
    /// Whether the player can build it in the province now, why not, and
    /// what it costs.
    #[serde(flatten)]
    pub choice: Choice,
}

/// A great project's construction of the player's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct MonumentConstruction {
    /// The great project's content key.
    pub monument: String,
    /// Its name, in the player language.
    pub name: String,
    /// The province the construction is in.
    pub province: ProvinceId,
    pub start: Date,
    /// The day it ends, as boosting brings it forward.
    pub end: Date,
    /// Cancelling it.
    pub cancel: Choice,
    /// Bringing it forward with manpower: the cost is the manpower, as
    /// `country.get` counts it.
    pub boost_manpower: Choice,
    /// Bringing it forward with ducats.
    pub boost_ducats: Choice,
}

/// A great project of the player's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct MonumentOption {
    /// Its content key, which the game's or a mod's great project files
    /// give it.
    pub monument: String,
    /// Its name, as the window shows it, in the player language.
    pub name: String,
    pub kind: MonumentKind,
    /// The province it stands in.
    pub province: ProvinceId,
    /// Its tier, from 0 to 3.
    pub tier: u32,
    /// Whether the player can upgrade it now, why not, and what it costs.
    #[serde(flatten)]
    pub choice: Choice,
}

/// What kind of great project it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum MonumentKind {
    Monument,
    Canal,
}
