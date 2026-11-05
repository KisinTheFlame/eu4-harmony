//! Colonists: the province view's colonize and settlement growth buttons.
use eu4_harmony_remote::{
    colonists::{ColonistAnswer, ColonistOptions, ColonistRequest},
    Error,
};

layer! {
    fn colonist_options(province: u32) -> Box<ColonistOptions> = crate::game::colonists::options;
    fn send_colonist(province: u32) -> () = crate::game::colonists::send;
    fn recall_colonist(province: u32) -> () = crate::game::colonists::recall;
    fn expel_minorities(province: u32, from: u32) -> () = crate::game::colonists::expel;
    fn change_settlement_growth(province: u32, start: bool) -> () = crate::game::colonists::change_growth;
}

pub fn run(request: ColonistRequest, game: &mut (impl Layer + ?Sized)) -> Result<ColonistAnswer, Error> {
    Ok(match request {
        ColonistRequest::Options { province } => ColonistAnswer::Options(game.colonist_options(province.0)?),
        ColonistRequest::Send { province } => ColonistAnswer::Send(game.send_colonist(province.0)?),
        ColonistRequest::Recall { province } => ColonistAnswer::Recall(game.recall_colonist(province.0)?),
        ColonistRequest::ExpelMinorities { province, from } => ColonistAnswer::ExpelMinorities(game.expel_minorities(province.0, from.0)?),
        ColonistRequest::StartSettlementGrowth { province } => ColonistAnswer::StartSettlementGrowth(game.change_settlement_growth(province.0, true)?),
        ColonistRequest::StopSettlementGrowth { province } => ColonistAnswer::StopSettlementGrowth(game.change_settlement_growth(province.0, false)?),
    })
}
