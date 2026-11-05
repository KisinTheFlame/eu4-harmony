//! The national focus: which kind of monarch power the player's country
//! favors, as the court view's three focus buttons set it.
use crate::choices::{Choice, PowerKind};
use serde::{Deserialize, Serialize};

group! {
    /// The national focus, in the court view.
    "focus": FocusRequest, FocusAnswer;

    /// The player's national focus and what the court view's focus buttons
    /// offer now: choosing each kind of power, and cancelling the focus
    /// set. The game lets the focus change only once in a while; until then
    /// every one is refused with the game's line saying when.
    game Options "options" {} -> Focus, "focus (adm, dip, mil or null), choose (one for each of adm, dip, mil: focus, can, reasons, cost), cancel (can, reasons, cost)";

    /// Sets the national focus on a kind of monarch power, as its button in
    /// the court view does. Refused with the game's reasons when the focus
    /// cannot change yet, and when it is already on that kind.
    game Choose "choose" {
        /// The kind of power: adm, dip or mil.
        focus: PowerKind,
    } -> (), "null once the focus is set";

    /// Removes the national focus, as pressing the button of the focus set
    /// does. Refused with the game's reasons when the focus cannot change
    /// yet, and when none is set.
    game Cancel "cancel" {} -> (), "null once the focus is removed";
}

/// The national focus and what the focus buttons offer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Focus {
    /// The kind of power the focus is on; null when none is set.
    pub focus: Option<PowerKind>,
    /// Setting the focus on each kind of power, in the order adm, dip, mil.
    pub choose: Vec<FocusChoice>,
    /// Removing the focus set.
    pub cancel: Choice,
}

/// Setting the national focus on one kind of power.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct FocusChoice {
    pub focus: PowerKind,
    #[serde(flatten)]
    pub choice: Choice,
}
