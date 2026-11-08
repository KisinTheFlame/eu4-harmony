//! Merchants: the trade view's merchant buttons.
use eu4_harmony_remote::merchants::{MerchantAnswer, MerchantOptions, MerchantRequest, Merchants, Mission, TradingPolicies};
use eu4_harmony_remote::merchants::args;

layer! {
    run MerchantRequest => MerchantAnswer;
    List => fn merchants -> Merchants = crate::game::merchants::list;
    fn merchant_options(node: &str) -> Box<MerchantOptions> = session crate::game::merchants::options;
    fn send_merchant(node: &str, mission: Mission) -> () = session crate::game::merchants::send;
    fn recall_merchant(node: &str) -> () = session crate::game::merchants::recall;
    Options { node } => game.merchant_options(node.as_str());
    Collect { node } => game.send_merchant(node.as_str(), Mission::Collect);
    Steer { node } => game.send_merchant(node.as_str(), Mission::Steer);
    Recall { node } => game.recall_merchant(node.as_str());
    SteerTo => fn steer_to -> () = crate::game::merchants::steer_to;
    fn trading_policies(node: &str) -> TradingPolicies = session crate::game::merchants::policies;
    Policies { node } => game.trading_policies(node.as_str());
    SetPolicy => fn set_trading_policy -> () = crate::game::merchants::set_policy;
}
