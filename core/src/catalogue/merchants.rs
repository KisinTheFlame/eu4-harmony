//! Merchants: the trade view's merchant buttons.
use eu4_harmony_remote::merchants::{MerchantAnswer, MerchantOptions, MerchantRequest, Merchants, Mission, TradingPolicies};

layer! {
    run MerchantRequest => MerchantAnswer;
    List => fn merchants() -> Merchants = caller crate::game::merchants::list;
    fn merchant_options(node: &str) -> Box<MerchantOptions> = caller crate::game::merchants::options;
    fn send_merchant(node: &str, mission: Mission) -> () = caller crate::game::merchants::send;
    fn recall_merchant(node: &str) -> () = caller crate::game::merchants::recall;
    fn steer_to(node: &str, to: &str) -> () = caller crate::game::merchants::steer_to;
    Options { node } => game.merchant_options(node.as_str());
    Collect { node } => game.send_merchant(node.as_str(), Mission::Collect);
    Steer { node } => game.send_merchant(node.as_str(), Mission::Steer);
    Recall { node } => game.recall_merchant(node.as_str());
    SteerTo { node, to } => game.steer_to(node.as_str(), to.as_str());
    fn trading_policies(node: &str) -> TradingPolicies = caller crate::game::merchants::policies;
    fn set_trading_policy(node: &str, policy: &str) -> () = caller crate::game::merchants::set_policy;
    Policies { node } => game.trading_policies(node.as_str());
    SetPolicy { node, policy } => game.set_trading_policy(node.as_str(), policy.as_str());
}
