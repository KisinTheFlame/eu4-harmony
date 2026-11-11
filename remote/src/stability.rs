//! Stability: raising it with administrative points and reducing war
//! exhaustion with diplomatic points, as the stability and expansion view's
//! buttons do, and treating rebels harshly with military points, as its
//! rebels' buttons do.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The stability and expansion view: raising stability, reducing war
    /// exhaustion, and harsh treatment of the rebels.
    "stability": StabilityRequest, StabilityAnswer;

    /// What the view's buttons offer now: raising stability by one,
    /// reducing war exhaustion, and harsh treatment of each rebel faction
    /// against the player; whether the player can, and at what cost in
    /// administrative, diplomatic or military points; the reasons are the
    /// button's tooltip when it cannot.
    game Options "options" {} -> Vec<StabilityOption>, "raise, reduce_war_exhaustion, then harsh_treatment for each rebel faction: action, faction (the faction's number), rebels (its rebel type's key) and name, null but for harsh_treatment; can, reasons, cost";

    /// Raises stability by one, as the view's button does, paying its cost
    /// in administrative points. Refused with the game's reasons when the
    /// player cannot now, as `stability.options` says beforehand.
    game Raise "raise" {} -> (), "null once stability is raised";

    /// Reduces war exhaustion, as the view's button does, paying its cost in
    /// diplomatic points. Refused with the game's reasons when the player
    /// cannot now, as `stability.options` says beforehand: with no war
    /// exhaustion, or too few points.
    game ReduceWarExhaustion "reduce_war_exhaustion" {} -> (), "null once war exhaustion is reduced";

    /// Treats a rebel faction against the player harshly, as the view's
    /// rebels button does once the player confirms, paying its cost in
    /// military points: the faction loses progress. Refused with the game's
    /// reasons when the player cannot now, as `stability.options` says
    /// beforehand.
    game HarshTreatment "harsh_treatment" {
        /// The faction's number, as stability.options gives it.
        faction: u32,
    } -> (), "null once done";

    /// Sets the policy the player's colonists follow toward natives, as the
    /// view's native policy buttons do. Refused with the game's reasons when
    /// the player cannot change it now, as `stability.options` says
    /// beforehand, and for the policy already set; `invalid_argument` for a
    /// policy the game has not.
    game SetNativePolicy "set_native_policy" {
        /// The policy: `native_policy_coexist`, `native_policy_trade` or
        /// `native_policy_hostile`.
        policy: String,
    } -> (), "null once the policy is set";
}

/// What the stability view lets the player do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum StabilityAction {
    /// Raising stability by one.
    Raise,
    /// Reducing war exhaustion.
    ReduceWarExhaustion,
    /// Treating a rebel faction harshly.
    HarshTreatment,
    /// Setting the native policy.
    SetNativePolicy,
}

/// One thing the stability view lets the player do, and whether it can now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct StabilityOption {
    pub action: StabilityAction,
    /// For harsh treatment, the rebel faction's number, which
    /// `stability.harsh_treatment` takes; null for the others.
    pub faction: Option<u32>,
    /// The faction's rebel type, by its content key, such as
    /// `nationalist_rebels`; null for the others.
    pub rebels: Option<String>,
    /// The faction's name, as the view shows it; null for the others.
    pub name: Option<String>,
    /// For set_native_policy, the policy, as `stability.set_native_policy`
    /// takes it, such as `native_policy_trade`; null for the others.
    pub policy: Option<String>,
    #[serde(flatten)]
    pub choice: Choice,
}
