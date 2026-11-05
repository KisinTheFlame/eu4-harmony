//! Centers of trade: upgrading and downgrading the one in a province, as the
//! province view's center of trade button does.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// Centers of trade in the player's provinces.
    "center_of_trade": CenterOfTradeRequest, CenterOfTradeAnswer;

    /// The center of trade in a province, and what the province view offers
    /// for it now: upgrading it a level, and downgrading it a level, each
    /// with whether the player can, why not, and the cost.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> CenterOfTradeOptions, "province; level: 0 for none, else 1 to 3; upgrade and downgrade, each can, reasons, cost";
    /// Upgrades the center of trade in a province of the player's a level,
    /// as clicking the province view's button and confirming does, at the
    /// game's cost. Refused with the game's reasons when the game would not
    /// let the player now.
    game Upgrade "upgrade" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once it is upgraded";
    /// Downgrades the center of trade in a province of the player's a level,
    /// as right-clicking the province view's button and confirming does, at
    /// the game's cost. Refused with the game's reasons when the game would
    /// not let the player now.
    game Downgrade "downgrade" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once it is downgraded";
}

/// The center of trade in a province, and what can be done with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct CenterOfTradeOptions {
    pub province: ProvinceId,
    /// Its level, 1 to 3; 0 when the province has none.
    pub level: u32,
    /// Upgrading it a level.
    pub upgrade: Choice,
    /// Downgrading it a level.
    pub downgrade: Choice,
}
