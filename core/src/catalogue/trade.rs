//! Trade: the trade nodes, and one node as its view shows it.
use eu4_harmony_remote::{
    trade::{NodeValue, TradeAnswer, TradeNode, TradeRequest},
    Error,
};

layer! {
    fn trade_nodes() -> Vec<NodeValue> = crate::game::trade::list;
    fn trade_node(node: String) -> TradeNode = crate::game::trade::node;
}

pub fn run(request: TradeRequest, game: &mut (impl Layer + ?Sized)) -> Result<TradeAnswer, Error> {
    Ok(match request {
        TradeRequest::Nodes {} => TradeAnswer::Nodes(game.trade_nodes()?),
        TradeRequest::Node { node } => TradeAnswer::Node(game.trade_node(node)?),
    })
}
