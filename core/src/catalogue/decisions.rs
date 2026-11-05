//! Decisions: what the decisions view lists for the player, and taking one.
use eu4_harmony_remote::{
    decisions::{Decision, DecisionAnswer, DecisionRequest},
    Error,
};

layer! {
    fn decisions() -> Vec<Decision> = crate::game::decisions::list;
    fn take_decision(decision: String) -> () = crate::game::decisions::take;
}

pub fn run(request: DecisionRequest, game: &mut (impl Layer + ?Sized)) -> Result<DecisionAnswer, Error> {
    Ok(match request {
        DecisionRequest::List {} => DecisionAnswer::List(game.decisions()?),
        DecisionRequest::Take { decision } => DecisionAnswer::Take(game.take_decision(decision)?),
    })
}
