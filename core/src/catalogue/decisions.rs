//! Decisions: what the decisions view lists for the player.
use eu4_harmony_remote::{
    decisions::{Decision, DecisionAnswer, DecisionRequest},
    Error,
};

layer! {
    fn decisions() -> Vec<Decision> = crate::game::decisions::list;
}

pub fn run(request: DecisionRequest, game: &mut (impl Layer + ?Sized)) -> Result<DecisionAnswer, Error> {
    Ok(match request {
        DecisionRequest::List {} => DecisionAnswer::List(game.decisions()?),
    })
}
