//! Forts: the player's forts, mothballing and reopening them.
use eu4_harmony_remote::forts::{FortAnswer, FortRequest, PlayerFort};
use eu4_harmony_remote::forts::args;

layer! {
    run FortRequest => FortAnswer;
    List => fn forts -> Vec<PlayerFort> = crate::game::forts::list;
    Mothball => fn mothball_fort -> () = crate::game::forts::mothball;
    Reopen => fn reopen_fort -> () = crate::game::forts::reopen;
}
