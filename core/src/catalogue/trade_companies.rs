//! Trade companies: the province view's trade company button and the
//! investments of the province's area.
use eu4_harmony_remote::trade_companies::{TradeCompanyAnswer, TradeCompanyOptions, TradeCompanyRequest};
use eu4_harmony_remote::trade_companies::args;

layer! {
    run TradeCompanyRequest => TradeCompanyAnswer;
    Options => fn trade_company_options -> TradeCompanyOptions = crate::game::trade_companies::options;
    fn change_trade_company(province: u32, add: bool) -> () = session crate::game::trade_companies::change;
    fn invest_in_trade_company(province: u32, investment: &str) -> () = session crate::game::trade_companies::invest;
    Add { province } => game.change_trade_company(province, true);
    Remove { province } => game.change_trade_company(province, false);
    Invest { province, investment } => game.invest_in_trade_company(province, investment.as_str());
}
