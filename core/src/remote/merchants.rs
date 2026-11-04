//! Merchants: the trade view's merchant buttons.
use eu4_harmony_remote::{
    merchants::{MerchantAnswer, MerchantOptions, MerchantRequest, Merchants, Mission},
    Error,
};

layer! {
    fn merchants() -> Merchants = crate::game::merchants::list;
    fn merchant_options(node: &str) -> Box<MerchantOptions> = crate::game::merchants::options;
    fn send_merchant(node: &str, mission: Mission) -> () = crate::game::merchants::send;
    fn recall_merchant(node: &str) -> () = crate::game::merchants::recall;
    fn steer_to(node: &str, to: &str) -> () = crate::game::merchants::steer_to;
}

pub fn run(request: MerchantRequest, game: &mut (impl Layer + ?Sized)) -> Result<MerchantAnswer, Error> {
    Ok(match request {
        MerchantRequest::List {} => MerchantAnswer::List(game.merchants()?),
        MerchantRequest::Options { node } => MerchantAnswer::Options(game.merchant_options(&node)?),
        MerchantRequest::Collect { node } => MerchantAnswer::Collect(game.send_merchant(&node, Mission::Collect)?),
        MerchantRequest::Steer { node } => MerchantAnswer::Steer(game.send_merchant(&node, Mission::Steer)?),
        MerchantRequest::Recall { node } => MerchantAnswer::Recall(game.recall_merchant(&node)?),
        MerchantRequest::SteerTo { node, to } => MerchantAnswer::SteerTo(game.steer_to(&node, &to)?),
    })
}
