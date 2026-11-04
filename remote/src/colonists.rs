//! Colonists: sending one to an empty province to found a colony there and
//! recalling it, as the province view's colonize button does.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's colonists.
    "colonist": ColonistRequest, ColonistAnswer;

    /// What the province view offers the player for colonizing a province
    /// now: sending a colonist there, and recalling the player's colonist
    /// from there, each with whether the player can, why not, and the cost:
    /// a colonist, and the monthly upkeep in ducats while it works.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> ColonistOptions, "province; working: the player's colonist works there; size: the colony's settlers, null when there is no colony; send and recall, each can, reasons, cost";
    /// Sends one of the player's colonists to a province, to found a colony
    /// there or grow the player's colony, as the province view's button
    /// does. Refused with the game's reasons when the game would not let the
    /// player send one there now (`colonist.options` says so beforehand).
    game Send "send" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the colonist is on its way";
    /// Recalls the player's colonist from a province, as the province view's
    /// button does once one works there. The game takes it back at its next
    /// day: until then `colonist.options` still reads it working there.
    game Recall "recall" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the colonist is recalled";
}

/// What the province view offers for colonizing a province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ColonistOptions {
    pub province: ProvinceId,
    /// One of the player's colonists works there.
    pub working: bool,
    /// The colony's settlers, as the province view shows them; null when the
    /// province is no colony.
    pub size: Option<f64>,
    /// Sending one of the player's colonists there.
    pub send: Choice,
    /// Recalling the player's colonist from there.
    pub recall: Choice,
}
