//! Colonists: the province view's colonize and settlement growth buttons.
use eu4_harmony_remote::colonists::{ColonistAnswer, ColonistOptions, ColonistRequest};

layer! {
    run ColonistRequest => ColonistAnswer;
    Options => fn colonist_options(province: u32) -> Box<ColonistOptions> = session crate::game::colonists::options;
    Send => fn send_colonist(province: u32) -> () = session crate::game::colonists::send;
    Recall => fn recall_colonist(province: u32) -> () = session crate::game::colonists::recall;
    ExpelMinorities => fn expel_minorities(province: u32, from: u32) -> () = session crate::game::colonists::expel;
    fn change_settlement_growth(province: u32, start: bool) -> () = session crate::game::colonists::change_growth;
    StartSettlementGrowth { province } => game.change_settlement_growth(province, true);
    StopSettlementGrowth { province } => game.change_settlement_growth(province, false);
}
