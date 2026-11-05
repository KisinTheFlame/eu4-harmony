//! Decisions: what the decisions view lists for the player, and whether the
//! player can take each now, and taking one as the view's button does.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The decisions the player can see in the decisions view.
    "decision": DecisionRequest, DecisionAnswer;

    /// The decisions the decisions view lists for the player now: those
    /// whose conditions for being shown hold, major ones first as the view
    /// sorts them, each with whether the player can take it and, when not,
    /// the conditions that do not hold, as its tooltip lists them.
    game List "list" {} -> Vec<Decision>, "the decisions: decision (its content key), name, major, can, reasons, cost";

    /// Takes a decision the decisions view lists, as its button does, which
    /// has the game carry out its effects. `rejected` with the conditions
    /// that do not hold when the player cannot take it now; `not_found` for
    /// a key that names no decision the view lists for the player.
    game Take "take" {
        /// The decision's content key, such as `move_capital`.
        decision: String,
    } -> (), "null once the game has taken it";
}

/// A decision the decisions view lists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Decision {
    /// Its content key, which the game's or a mod's decision files give it.
    pub decision: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The view lists it among the major decisions.
    pub major: bool,
    /// Whether the player can take it now; the reasons are the conditions
    /// that do not hold.
    #[serde(flatten)]
    pub choice: Choice,
}
