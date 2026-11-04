//! Trade companies: adding a province of the player's to its trade company
//! and removing it, as the province view's trade company button does, and
//! the trade company investments the province view offers in its area.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's trade companies.
    "trade_company": TradeCompanyRequest, TradeCompanyAnswer;

    /// What the province view offers for a province's trade company now:
    /// adding it to the trade company of its region and removing it from
    /// it, each with whether the player can, why not, and the cost; and in
    /// its area, each trade company investment, whether the player made it,
    /// whether the player can make it, why not, and the cost.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> TradeCompanyOptions, "province; in_trade_company; add and remove, each can, reasons, cost; investments: investment (its content key), name, made, can, reasons, cost";
    /// Adds a province of the player's to the trade company of its region,
    /// as the province view's button does, confirming the game's question.
    /// Refused with the game's reasons when the game would not let the
    /// player now.
    game Add "add" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once it is in the trade company";
    /// Removes a province of the player's from its trade company, as the
    /// province view's button does, confirming the game's question.
    game Remove "remove" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once it is out of the trade company";
    /// Makes a trade company investment in the area of a province of the
    /// player's, as the province view's investment button does, confirming
    /// the game's question, at the game's cost. Refused with the game's
    /// reasons when the game would not let the player now.
    game Invest "invest" {
        /// The province.
        province: ProvinceId,
        /// The investment's content key, such as `local_quarter`.
        investment: String,
    } -> (), "null once the investment is made";
}

/// What the province view offers for a province's trade company.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct TradeCompanyOptions {
    pub province: ProvinceId,
    /// The province is in a trade company.
    pub in_trade_company: bool,
    /// Adding it to the trade company of its region.
    pub add: Choice,
    /// Removing it from its trade company.
    pub remove: Choice,
    /// The trade company investments of the province's area, in the game's
    /// order.
    pub investments: Vec<Investment>,
}

/// A trade company investment in an area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Investment {
    /// Its content key, which the game's or a mod's investment files give it.
    pub investment: String,
    /// Its name, in the player language.
    pub name: String,
    /// The player made it in the area.
    pub made: bool,
    /// Whether the player can make it there now.
    #[serde(flatten)]
    pub choice: Choice,
}
