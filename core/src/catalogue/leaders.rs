//! Leaders: recruiting, assigning, removing and dismissing them.
use eu4_harmony_remote::{
    leaders::{Leader, LeaderAnswer, LeaderOption, LeaderRequest},
    Error,
};

layer! {
    fn leader_options() -> Vec<LeaderOption> = crate::game::leaders::options;
    fn recruit_leader(kind: String) -> Leader = crate::game::leaders::recruit;
    fn leaders() -> Vec<Leader> = crate::game::leaders::list;
    fn assign_leader(leader: u32, army: u64) -> () = crate::game::leaders::assign;
    fn remove_leader(army: u64) -> () = crate::game::leaders::remove;
    fn dismiss_leader(leader: u32) -> () = crate::game::leaders::dismiss;
}

pub fn run(request: LeaderRequest, game: &mut (impl Layer + ?Sized)) -> Result<LeaderAnswer, Error> {
    Ok(match request {
        LeaderRequest::Options {} => LeaderAnswer::Options(game.leader_options()?),
        LeaderRequest::Recruit { kind } => LeaderAnswer::Recruit(game.recruit_leader(kind)?),
        LeaderRequest::List {} => LeaderAnswer::List(game.leaders()?),
        LeaderRequest::Assign { leader, army } => LeaderAnswer::Assign(game.assign_leader(leader, army.0)?),
        LeaderRequest::Remove { army } => LeaderAnswer::Remove(game.remove_leader(army.0)?),
        LeaderRequest::Dismiss { leader } => LeaderAnswer::Dismiss(game.dismiss_leader(leader)?),
    })
}
