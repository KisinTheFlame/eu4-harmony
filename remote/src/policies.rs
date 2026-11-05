//! Policies: those the policies view offers the player, and enabling and
//! disabling them as its buttons do.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The player's policies.
    "policy": PolicyRequest, PolicyAnswer;

    /// The policies the policies view offers the player, by content key: those
    /// whose conditions for being offered hold, and those the player has
    /// enabled, with whether the player can enable each now and why not, in
    /// the game's words as its tooltip gives them: the conditions that do not
    /// hold, that the player has it already, that every slot of its kind is
    /// taken.
    game List "list" {} -> Vec<Policy>, "the policies: policy (its content key), name, category, enabled, can, reasons, cost";

    /// Enables a policy, as the policies view's button does. `rejected` with
    /// the game's reasons when the game would not let the player enable it
    /// now; `not_found` for a key that names no policy.
    game Enable "enable" {
        /// The policy's content key, such as `the_spy_discovery_act`.
        policy: String,
    } -> (), "null once the game has enabled it";

    /// Disables a policy the player has enabled, as the policies view's
    /// button does. `rejected` when the player has not enabled it.
    game Disable "disable" {
        /// The policy's content key.
        policy: String,
    } -> (), "null once the game has disabled it";
}

/// A policy the policies view offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Policy {
    /// Its content key, which the game's or a mod's policy files give it.
    pub policy: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// Its kind: `adm`, `dip` or `mil`, the slots it takes.
    pub category: String,
    /// The player has enabled it.
    pub enabled: bool,
    /// Whether the player can enable it now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
}
