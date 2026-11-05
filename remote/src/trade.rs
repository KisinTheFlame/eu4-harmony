//! Trade: the trade nodes, and one node as the trade node view shows it.
use crate::CountryTag;
use serde::{Deserialize, Serialize};

group! {
    /// Trade nodes, as the trade map mode and the trade node view show them.
    "trade": TradeRequest, TradeAnswer;

    /// Every trade node, with the value in it. In the player's view only
    /// the nodes with a province the player has discovered, as the trade
    /// map shows them.
    game Nodes "nodes" {} -> Vec<NodeValue>, "the nodes: node (its key, such as sevilla), name, value (ducats)";
    /// One trade node as its view shows it: its value, the value its own
    /// provinces make, the value coming in and going out on each link, and
    /// the countries with trade power there, each with its share and its
    /// merchant. The trade node view shows every country's, so the player's
    /// view leaves nothing out of a node the player has discovered, and
    /// refuses one none of whose provinces the player has discovered
    /// (not_found).
    game Node "node" {
        /// The node's key, such as `sevilla`.
        node: String,
    } -> TradeNode, "the node: node, name, value, local_value, incoming (each from, a node's key, and ducats), outgoing (each to and ducats), and countries (each the country, its trade power, its share in percent, whether it collects there, whether it is its home node, and its merchant: collect, or steer with the node it steers toward; null for none), the most powerful first";
}

/// A trade node and its value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct NodeValue {
    /// Its key, which common/tradenodes gives it.
    pub node: String,
    /// Its name, as the game shows it.
    pub name: String,
    /// The value of trade in it, in ducats.
    pub value: f64,
}

/// One trade node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct TradeNode {
    pub node: String,
    pub name: String,
    /// The value of trade in it, in ducats.
    pub value: f64,
    /// The part its own provinces make.
    pub local_value: f64,
    /// What comes in from each node upstream.
    pub incoming: Vec<Flow>,
    /// What goes out to each node downstream, in the order of its links.
    pub outgoing: Vec<Flow>,
    /// The countries with trade power in it.
    pub countries: Vec<TradePower>,
}

/// Value moving along one link between two nodes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Flow {
    /// The node at the link's other end.
    pub node: String,
    pub ducats: f64,
}

/// A country's place in a trade node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct TradePower {
    pub country: CountryTag,
    /// Its trade power there, as the view shows it.
    pub power: f64,
    /// Its share of the node's trade power, in percent.
    pub share: f64,
    /// It collects from the node: its merchant collects there, or it is its
    /// home node and no merchant of its steers there.
    pub collects: bool,
    /// The node is its home node, where its trade port is.
    pub home: bool,
    /// Its merchant there; null for none.
    pub merchant: Option<Merchant>,
}

/// What a merchant does in a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Merchant {
    pub action: MerchantAction,
    /// For one that steers, the key of the node it steers toward.
    pub toward: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum MerchantAction {
    Collect,
    Steer,
}
