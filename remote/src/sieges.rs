//! Sieges: what the siege view offers the player against a fort the
//! player's armies besiege, and its buttons: the artillery barrage, the
//! naval barrage and the assault.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The siege view's buttons, for a siege of the player's.
    "siege": SiegeRequest, SiegeAnswer;

    /// What the siege view offers for the siege in a province: the
    /// artillery barrage, the naval barrage and the assault, each with
    /// whether the player can, why not as the button's tooltip says, and
    /// what it costs. In the player's view a province fog of war hides has
    /// no siege.
    game Options "options" {
        /// The besieged province.
        province: ProvinceId,
    } -> Vec<SiegeAction>, "one for each, in the order bombard, naval_bombard, assault: action, can, reasons, cost";
    /// Orders an artillery barrage of the besieged fort, as the siege view's
    /// button does, at its cost in military points: it makes a breach.
    game Bombard "bombard" {
        /// The besieged province.
        province: ProvinceId,
    } -> (), "nothing";
    /// Orders a naval barrage of the besieged fort by the player's fleets
    /// next to it, as the siege view's button does, at its cost in military
    /// points: it makes a breach.
    game NavalBombard "naval_bombard" {
        /// The besieged province.
        province: ProvinceId,
    } -> (), "nothing";
    /// Orders an assault of the besieged fort through its breach, as the
    /// siege view's button does, at its cost in military points.
    game Assault "assault" {
        /// The besieged province.
        province: ProvinceId,
    } -> (), "nothing";
}

/// One of the siege view's buttons.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct SiegeAction {
    /// bombard, naval_bombard or assault: the capability that presses it.
    pub action: String,
    #[serde(flatten)]
    pub choice: Choice,
}
