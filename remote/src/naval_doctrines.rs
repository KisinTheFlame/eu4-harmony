//! Naval doctrines: the doctrines the military view's naval doctrine list
//! offers, and choosing one as its buttons do.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The military view's naval doctrine: the doctrines and choosing one.
    "naval_doctrine": NavalDoctrineRequest, NavalDoctrineAnswer;

    /// The naval doctrines of the game's database, in its order, as the
    /// military view's list offers them: whether each is the player's, and
    /// whether the player can choose it now, with the game's reasons when
    /// not (the naval force limit under the least the list opens at, the
    /// doctrine the player has already, its conditions, the treasury) and
    /// its cost in ducats, which grows with the sailors of the player's
    /// navy.
    game Options "options" {} -> Vec<NavalDoctrineOption>, "the doctrines: doctrine (its content key), name, current, can, reasons, cost";

    /// Chooses a naval doctrine, as its button in the military view's list
    /// does, paying its cost in ducats; the doctrine the player had is
    /// lost. `rejected` with the game's reasons when the player cannot now,
    /// as `naval_doctrine.options` says beforehand; `not_found` for a key
    /// that names no doctrine.
    game Choose "choose" {
        /// The doctrine's content key, such as `merchant_navy`.
        doctrine: String,
    } -> (), "null once the game has set the doctrine";
}

/// A naval doctrine of the military view's list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct NavalDoctrineOption {
    /// Its content key, which the game's or a mod's
    /// `common/naval_doctrines` gives it.
    pub doctrine: String,
    /// Its name, in the player language.
    pub name: String,
    /// It is the player's doctrine now.
    pub current: bool,
    /// Whether the player can choose it now, why not, and what it costs.
    #[serde(flatten)]
    pub choice: Choice,
}
