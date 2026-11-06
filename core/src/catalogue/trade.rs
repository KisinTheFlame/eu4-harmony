//! Trade: the trade nodes, and one node as its view shows it.
use eu4_harmony_remote::trade::{NodeValue, TradeAnswer, TradeNode, TradeRequest};

layer! {
    run TradeRequest => TradeAnswer;
    Nodes => fn trade_nodes() -> Vec<NodeValue> = caller crate::game::trade::list;
    Node => fn trade_node(node: String) -> TradeNode = caller crate::game::trade::node;
}
