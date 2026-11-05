//! Estates: the player's estates as the estates view shows them, their
//! privileges, and the view's interactions over crown land; granting and
//! revoking privileges and the interactions, as its buttons do.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The player's estates.
    "estate": EstateRequest, EstateAnswer;

    /// The player's estates, by content key, as the estates view shows them:
    /// loyalty, influence and land share, and the privileges the view lists
    /// for each (those whose conditions for being valid hold, and those
    /// granted), each with whether the player can grant it now, or revoke it
    /// when granted, and why not, in the game's words as its tooltip gives
    /// them.
    game List "list" {} -> Vec<Estate>, "the estates: estate (its content key), name, loyalty, influence, land_share, privileges: privilege (its content key), name, granted, can, reasons, cost";

    /// The estates view's interactions over crown land, by content key, such
    /// as `seize_land` and `sale_of_titles`, each with whether the player can
    /// use it now and why not: its conditions that do not hold, its cooldown.
    game Interactions "interactions" {} -> Vec<EstateInteraction>, "the interactions: interaction (its content key), name, can, reasons, cost";

    /// Grants an estate a privilege, as the privilege's button in the
    /// estates view does. `rejected` with the game's reasons when the game
    /// would not let the player grant it now; `not_found` for an estate the
    /// player does not have or a privilege that is not that estate's.
    game GrantPrivilege "grant_privilege" {
        /// The estate's content key, such as `estate_nobles`.
        estate: String,
        /// The privilege's content key, such as `estate_nobles_land_rights`.
        privilege: String,
    } -> (), "null once the game has granted it";

    /// Revokes a privilege the player granted an estate, as its button in
    /// the estates view does, confirming the price its dialog asks: what
    /// revoking costs, as the game takes it. `rejected` with the game's
    /// reasons when the game would not let the player revoke it now, or
    /// when it is not granted.
    game RevokePrivilege "revoke_privilege" {
        /// The estate's content key.
        estate: String,
        /// The privilege's content key.
        privilege: String,
    } -> (), "null once the game has revoked it";

    /// Uses one of the estates view's interactions over crown land, as its
    /// button does. `rejected` with the game's reasons when the game would
    /// not let the player use it now; `not_found` for a key that names
    /// none.
    game Interact "interact" {
        /// The interaction's content key, such as `seize_land`.
        interaction: String,
    } -> (), "null once the game has carried it out";
}

/// An estate of the player's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Estate {
    /// Its content key, which the game's or a mod's estate files give it.
    pub estate: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// Its loyalty, 0 to 100.
    pub loyalty: f64,
    /// Its influence, 0 to 100.
    pub influence: f64,
    /// The share of the land it holds, in percent.
    pub land_share: f64,
    pub privileges: Vec<Privilege>,
}

/// A privilege the estates view lists for an estate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Privilege {
    /// Its content key, which the game's or a mod's privilege files give it.
    pub privilege: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The player has granted it.
    pub granted: bool,
    /// Whether the player can grant it now, or revoke it when it is granted,
    /// and why not.
    #[serde(flatten)]
    pub choice: Choice,
}

/// An interaction of the estates view over crown land.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct EstateInteraction {
    /// Its content key, which the game's or a mod's crown land files give
    /// it.
    pub interaction: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// Whether the player can use it now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
}
