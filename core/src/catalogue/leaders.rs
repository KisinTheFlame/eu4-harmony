//! Leaders: recruiting, assigning, removing and dismissing them.
use eu4_harmony_remote::leaders::{Leader, LeaderAnswer, LeaderOption, LeaderRequest};
use eu4_harmony_remote::leaders::args;

layer! {
    run LeaderRequest => LeaderAnswer;
    Options => fn leader_options -> Vec<LeaderOption> = crate::game::leaders::options;
    Recruit => fn recruit_leader -> Leader = caller crate::game::leaders::recruit;
    List => fn leaders -> Vec<Leader> = crate::game::leaders::list;
    Assign => fn assign_leader -> () = crate::game::leaders::assign;
    Remove => fn remove_leader -> () = crate::game::leaders::remove;
    Dismiss => fn dismiss_leader -> () = crate::game::leaders::dismiss;
}
