//! Trade companies: the province view's trade company button and the
//! investments of the province's area.
use eu4_harmony_remote::{
    trade_companies::{TradeCompanyAnswer, TradeCompanyOptions, TradeCompanyRequest},
    Error,
};

layer! {
    fn trade_company_options(province: u32) -> TradeCompanyOptions = crate::game::trade_companies::options;
    fn change_trade_company(province: u32, add: bool) -> () = crate::game::trade_companies::change;
    fn invest_in_trade_company(province: u32, investment: &str) -> () = crate::game::trade_companies::invest;
}

pub fn run(request: TradeCompanyRequest, game: &mut (impl Layer + ?Sized)) -> Result<TradeCompanyAnswer, Error> {
    Ok(match request {
        TradeCompanyRequest::Options { province } => TradeCompanyAnswer::Options(game.trade_company_options(province.0)?),
        TradeCompanyRequest::Add { province } => TradeCompanyAnswer::Add(game.change_trade_company(province.0, true)?),
        TradeCompanyRequest::Remove { province } => TradeCompanyAnswer::Remove(game.change_trade_company(province.0, false)?),
        TradeCompanyRequest::Invest { province, investment } => TradeCompanyAnswer::Invest(game.invest_in_trade_company(province.0, &investment)?),
    })
}
