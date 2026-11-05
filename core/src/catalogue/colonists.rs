//! Colonists: the province view's colonize buttons.
use eu4_harmony_remote::colonists::{ColonistAnswer, ColonistOptions, ColonistRequest};

layer! {
    run ColonistRequest => ColonistAnswer;
    Options => fn colonist_options(province: u32) -> ColonistOptions = crate::game::colonists::options;
    Send => fn send_colonist(province: u32) -> () = crate::game::colonists::send;
    Recall => fn recall_colonist(province: u32) -> () = crate::game::colonists::recall;
}
