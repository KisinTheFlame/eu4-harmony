//! Colonists: sending one to an empty province to found a colony there and
//! recalling it, as the province view's colonize button does; expelling a
//! minority from one of the player's provinces into a new colony; and a
//! colonist promoting settlement growth in one of the player's provinces.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's colonists.
    "colonist": ColonistRequest, ColonistAnswer;

    /// What the province view offers the player for colonizing a province
    /// now: sending a colonist there, expelling a minority there from one
    /// of the player's provinces, recalling the player's colonist from
    /// there, and promoting settlement growth there or stopping it, each
    /// with whether the player can, why not, and the cost: a colonist, and
    /// the colonial upkeep a month it adds while it works.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> Box<ColonistOptions>, "province; working: the player's colonist works there; size: the colony's settlers, null when there is no colony; send and recall, each can, reasons, cost; expel_from: from (a province of the player's), can, reasons, cost for each minority the player could expel there; growing: the player's colonist promotes settlement growth there; start_growth and stop_growth, each can, reasons, cost";
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
    /// Sends one of the player's colonists to found a colony in a province
    /// with the minority it expels from one of the player's provinces, as
    /// choosing that province in the colonize button's list does. Refused
    /// with the game's reasons when the game would not let the player now
    /// (`colonist.options` lists the provinces, and says so beforehand).
    game ExpelMinorities "expel_minorities" {
        /// The province the colony is founded in.
        province: ProvinceId,
        /// The player's province the minority is expelled from.
        from: ProvinceId,
    } -> (), "null once the colonist is on its way";
    /// Has one of the player's colonists promote settlement growth in one of
    /// the player's provinces, as the province view's button does once the
    /// player confirms. Refused with the game's reasons when the game would
    /// not let the player now.
    game StartSettlementGrowth "start_settlement_growth" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the colonist promotes settlement growth there";
    /// Stops the player's colonist promoting settlement growth in a
    /// province, as the province view's button does once the player
    /// confirms.
    game StopSettlementGrowth "stop_settlement_growth" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the colonist is called back";
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
    /// The player's provinces the colonize button lists for expelling their
    /// minority there; empty where the game offers no expelling.
    pub expel_from: Vec<Expulsion>,
    /// One of the player's colonists promotes settlement growth there.
    pub growing: bool,
    /// Having one of the player's colonists promote settlement growth there.
    pub start_growth: Choice,
    /// Stopping the player's colonist promoting settlement growth there.
    pub stop_growth: Choice,
}

/// A province of the player's whose minority the player could expel into a
/// new colony.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Expulsion {
    /// The province the minority is expelled from.
    pub from: ProvinceId,
    /// Whether the player can expel it there now.
    #[serde(flatten)]
    pub choice: Choice,
}
