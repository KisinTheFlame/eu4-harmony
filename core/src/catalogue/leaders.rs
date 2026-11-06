//! Leaders: recruiting, assigning, removing and dismissing them.
use eu4_harmony_remote::leaders::{Leader, LeaderAnswer, LeaderOption, LeaderRequest};

layer! {
    run LeaderRequest => LeaderAnswer;
    Options => fn leader_options() -> Vec<LeaderOption> = session crate::game::leaders::options;
    Recruit => fn recruit_leader(kind: String) -> Leader = caller crate::game::leaders::recruit;
    List => fn leaders() -> Vec<Leader> = session crate::game::leaders::list;
    Assign => fn assign_leader(leader: u32, army: u64) -> () = session crate::game::leaders::assign;
    Remove => fn remove_leader(army: u64) -> () = session crate::game::leaders::remove;
    Dismiss => fn dismiss_leader(leader: u32) -> () = session crate::game::leaders::dismiss;
}
