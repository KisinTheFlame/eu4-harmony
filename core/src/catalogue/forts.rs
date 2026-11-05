//! Forts: the player's forts, mothballing and reopening them.
use eu4_harmony_remote::forts::{Fort, FortAnswer, FortRequest};

layer! {
    run FortRequest => FortAnswer;
    List => fn forts() -> Vec<Fort> = crate::game::forts::list;
    Mothball => fn mothball_fort(province: u32) -> () = crate::game::forts::mothball;
    Reopen => fn reopen_fort(province: u32) -> () = crate::game::forts::reopen;
}
