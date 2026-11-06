//! Forts: the player's forts, mothballing and reopening them.
use eu4_harmony_remote::forts::{FortAnswer, FortRequest, PlayerFort};

layer! {
    run FortRequest => FortAnswer;
    List => fn forts() -> Vec<PlayerFort> = caller crate::game::forts::list;
    Mothball => fn mothball_fort(province: u32) -> () = caller crate::game::forts::mothball;
    Reopen => fn reopen_fort(province: u32) -> () = caller crate::game::forts::reopen;
}
