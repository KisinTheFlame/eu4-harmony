//! Colonists: the province view's colonize and settlement growth buttons.
use eu4_harmony_remote::colonists::{ColonistAnswer, ColonistOptions, ColonistRequest};
use eu4_harmony_remote::colonists::args;

layer! {
    run ColonistRequest => ColonistAnswer;
    Options => fn colonist_options -> Box<ColonistOptions> = crate::game::colonists::options;
    Send => fn send_colonist -> () = crate::game::colonists::send;
    Recall => fn recall_colonist -> () = crate::game::colonists::recall;
    ExpelMinorities => fn expel_minorities -> () = crate::game::colonists::expel;
    fn change_settlement_growth(province: u32, start: bool) -> () = session crate::game::colonists::change_growth;
    StartSettlementGrowth { province } => game.change_settlement_growth(province, true);
    StopSettlementGrowth { province } => game.change_settlement_growth(province, false);
}
