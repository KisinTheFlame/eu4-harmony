//! Merchants: what the trade view offers the player in a trade node, sending
//! a merchant there to collect or to steer trade, recalling it, and choosing
//! where a steering merchant steers. Trade nodes are named by their content
//! key, such as `english_channel`.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The player's merchants.
    "merchant": MerchantRequest, MerchantAnswer;

    /// The player's merchants, how many are free, and where the others are:
    /// each node the player has a merchant in, with what it does there.
    game List "list" {} -> Merchants, "merchants: the player's; free: those not sent; placed: node, name, mission (collect or steer), arriving (on its way), steer_to (the node it steers to, or null)";
    /// What the trade view offers the player in a trade node now: sending a
    /// merchant there to collect, or to steer trade (changing the mission of
    /// the one there), recalling the player's merchant, and, for a merchant
    /// that steers, each node downstream it can steer to; each with whether
    /// the player can, why not, and the cost.
    game Options "options" {
        /// The trade node's content key, such as `english_channel`.
        node: String,
    } -> Box<MerchantOptions>, "node, name; mission: what the player's merchant there does, or null; arriving: it is on its way; collect, steer and recall, each can, reasons, cost; steer_to: node, name, current, can, reasons, cost for each node downstream";
    /// Sends a merchant to a trade node to collect trade there, as the trade
    /// view's button does, or has the player's merchant there collect
    /// instead of steering. Refused with the game's reasons when the game
    /// would not let the player now.
    game Collect "collect" {
        /// The trade node's content key.
        node: String,
    } -> (), "null once the merchant is on its way, or collects";
    /// Sends a merchant to a trade node to steer trade downstream, as the
    /// trade view's button does, or has the player's merchant there steer
    /// instead of collecting. Refused with the game's reasons when the game
    /// would not let the player now.
    game Steer "steer" {
        /// The trade node's content key.
        node: String,
    } -> (), "null once the merchant is on its way, or steers";
    /// Recalls the player's merchant from a trade node, as the trade view's
    /// button does, once it is there.
    game Recall "recall" {
        /// The trade node's content key.
        node: String,
    } -> (), "null once the merchant is recalled";
    /// Has the player's merchant steering in a trade node steer trade to one
    /// of the nodes downstream, as choosing it in the trade view does.
    /// Refused with the game's reasons when the game would not let the
    /// player now.
    game SteerTo "steer_to" {
        /// The trade node the merchant is in.
        node: String,
        /// The node downstream to steer to.
        to: String,
    } -> (), "null once the merchant steers there";
}

/// What a merchant does in a trade node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Mission {
    /// It collects the trade there for the player.
    Collect,
    /// It steers the trade there downstream.
    Steer,
}

/// The player's merchants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Merchants {
    /// How many merchants the player has.
    pub merchants: u32,
    /// How many of them are free to send.
    pub free: u32,
    /// The nodes the player has a merchant in.
    pub placed: Vec<Placed>,
}

/// A trade node the player has a merchant in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Placed {
    /// The node's content key.
    pub node: String,
    /// Its name, in the player language.
    pub name: String,
    pub mission: Mission,
    /// It is still on its way there.
    pub arriving: bool,
    /// The node downstream it steers trade to; null when it collects or is
    /// on its way.
    pub steer_to: Option<String>,
}

/// What the trade view offers in a trade node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct MerchantOptions {
    /// The node's content key.
    pub node: String,
    /// Its name, in the player language.
    pub name: String,
    /// What the player's merchant there does; null when the player has none
    /// there.
    pub mission: Option<Mission>,
    /// The player's merchant is still on its way there.
    pub arriving: bool,
    /// Sending a merchant to collect, or having the one there collect.
    pub collect: Choice,
    /// Sending a merchant to steer, or having the one there steer.
    pub steer: Choice,
    /// Recalling the player's merchant.
    pub recall: Choice,
    /// The nodes downstream a merchant steering there can steer to.
    pub steer_to: Vec<Downstream>,
}

/// A node downstream a steering merchant can steer trade to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Downstream {
    /// Its content key.
    pub node: String,
    /// Its name, in the player language.
    pub name: String,
    /// The player's merchant steers there now.
    pub current: bool,
    /// Whether the player can have the merchant steer there now.
    #[serde(flatten)]
    pub choice: Choice,
}
