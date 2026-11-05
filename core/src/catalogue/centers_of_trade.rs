//! Centers of trade: the province view's center of trade button.
use eu4_harmony_remote::{
    centers_of_trade::{CenterOfTradeAnswer, CenterOfTradeOptions, CenterOfTradeRequest},
    Error,
};

layer! {
    fn center_of_trade_options(province: u32) -> CenterOfTradeOptions = crate::game::centers_of_trade::options;
    fn change_center_of_trade(province: u32, up: bool) -> () = crate::game::centers_of_trade::change;
}

pub fn run(request: CenterOfTradeRequest, game: &mut (impl Layer + ?Sized)) -> Result<CenterOfTradeAnswer, Error> {
    Ok(match request {
        CenterOfTradeRequest::Options { province } => CenterOfTradeAnswer::Options(game.center_of_trade_options(province.0)?),
        CenterOfTradeRequest::Upgrade { province } => CenterOfTradeAnswer::Upgrade(game.change_center_of_trade(province.0, true)?),
        CenterOfTradeRequest::Downgrade { province } => CenterOfTradeAnswer::Downgrade(game.change_center_of_trade(province.0, false)?),
    })
}
