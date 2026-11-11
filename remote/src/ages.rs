//! The ages: the age the world is in, the player's splendor, and the age's
//! abilities as the ages view offers them, taking one as its button does.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The ages view: the age, splendor and the age's abilities.
    "age": AgeRequest, AgeAnswer;

    /// The age the world is in, and the player's splendor, as the ages
    /// view shows them.
    game Get "get" {} -> Age, "age (its content key), name, splendor";

    /// The abilities of the age the world is in, in the order the game
    /// keeps them: whether the player has taken each, and what taking it
    /// offers now, with whether the player can, the game's reasons why not
    /// and its cost in splendor. One taken already offers nothing more.
    game Abilities "abilities" {} -> Vec<AgeAbilityOption>, "the abilities: ability (its content key), name, taken, can, reasons, cost";

    /// Takes an ability of the age, as the ages view's button does, paying
    /// its cost in splendor. `rejected` with the game's reasons when the
    /// game would not let the player take it now, as `age.abilities` says
    /// beforehand, or when it is not an ability of the age the world is in;
    /// `not_found` for a key that names no ability.
    game TakeAbility "take_ability" {
        /// The ability's content key, such as `ab_justified_wars`.
        ability: String,
    } -> (), "null once the game has taken it";
}

/// The age the world is in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Age {
    /// Its content key, such as `age_of_discovery`.
    pub age: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The player's splendor.
    pub splendor: f64,
}

/// An ability of the age, as the ages view offers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct AgeAbilityOption {
    /// Its content key, which the game's or a mod's age files give it.
    pub ability: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The player has taken it.
    pub taken: bool,
    /// Whether the player can take it now, why not, and what it costs.
    #[serde(flatten)]
    pub choice: Choice,
}
