//! Stability: raising it with administrative points, as the stability and
//! expansion view's button does.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The stability and expansion view: raising stability.
    "stability": StabilityRequest, StabilityAnswer;

    /// What the view's stability button offers now: raising stability by
    /// one, whether the player can and at what cost in administrative
    /// points; the reasons are the button's tooltip when it cannot.
    game Options "options" {} -> Vec<StabilityOption>, "one entry, raise: action, can, reasons, cost";

    /// Raises stability by one, as the view's button does, paying its cost
    /// in administrative points. Refused with the game's reasons when the
    /// player cannot now, as `stability.options` says beforehand.
    game Raise "raise" {} -> (), "null once stability is raised";
}

/// What the stability view lets the player do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum StabilityAction {
    /// Raising stability by one.
    Raise,
}

/// One thing the stability view lets the player do, and whether it can now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct StabilityOption {
    pub action: StabilityAction,
    #[serde(flatten)]
    pub choice: Choice,
}
