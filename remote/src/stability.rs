//! Stability: raising it with administrative points, as the stability and
//! expansion view's button does, and treating rebels harshly with military
//! points, as its rebels' buttons do.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The stability and expansion view: raising stability, and harsh
    /// treatment of the rebels.
    "stability": StabilityRequest, StabilityAnswer;

    /// What the view's buttons offer now: raising stability by one, and
    /// harsh treatment of each rebel faction against the player; whether
    /// the player can, and at what cost in administrative or military
    /// points; the reasons are the button's tooltip when it cannot.
    game Options "options" {} -> Vec<StabilityOption>, "raise, then harsh_treatment for each rebel faction: action, faction (the faction's number), rebels (its rebel type's key) and name, null for raise; can, reasons, cost";

    /// Raises stability by one, as the view's button does, paying its cost
    /// in administrative points. Refused with the game's reasons when the
    /// player cannot now, as `stability.options` says beforehand.
    game Raise "raise" {} -> (), "null once stability is raised";

    /// Treats a rebel faction against the player harshly, as the view's
    /// rebels button does once the player confirms, paying its cost in
    /// military points: the faction loses progress. Refused with the game's
    /// reasons when the player cannot now, as `stability.options` says
    /// beforehand.
    game HarshTreatment "harsh_treatment" {
        /// The faction's number, as stability.options gives it.
        faction: u32,
    } -> (), "null once done";
}

/// What the stability view lets the player do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum StabilityAction {
    /// Raising stability by one.
    Raise,
    /// Treating a rebel faction harshly.
    HarshTreatment,
}

/// One thing the stability view lets the player do, and whether it can now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct StabilityOption {
    pub action: StabilityAction,
    /// For harsh treatment, the rebel faction's number, which
    /// `stability.harsh_treatment` takes; null for raise.
    pub faction: Option<u32>,
    /// The faction's rebel type, by its content key, such as
    /// `nationalist_rebels`; null for raise.
    pub rebels: Option<String>,
    /// The faction's name, as the view shows it; null for raise.
    pub name: Option<String>,
    #[serde(flatten)]
    pub choice: Choice,
}
