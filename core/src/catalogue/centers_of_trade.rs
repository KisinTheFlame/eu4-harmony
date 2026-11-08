//! Centers of trade: the province view's center of trade button.
use eu4_harmony_remote::centers_of_trade::{CenterOfTradeAnswer, CenterOfTradeOptions, CenterOfTradeRequest};
use eu4_harmony_remote::centers_of_trade::args;

layer! {
    run CenterOfTradeRequest => CenterOfTradeAnswer;
    Options => fn center_of_trade_options -> CenterOfTradeOptions = crate::game::centers_of_trade::options;
    fn change_center_of_trade(province: u32, up: bool) -> () = session crate::game::centers_of_trade::change;
    Upgrade { province } => game.change_center_of_trade(province, true);
    Downgrade { province } => game.change_center_of_trade(province, false);
}
