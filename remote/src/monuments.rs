//! Monuments: the great projects in the player's provinces, monuments and
//! canals, as the monuments window lists them, and upgrading one as its
//! button does.
use crate::{choices::Choice, ProvinceId};
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
