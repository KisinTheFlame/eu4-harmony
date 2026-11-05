//! Ideas: the idea groups of the ideas view, and taking, buying and
//! abandoning them.
use eu4_harmony_remote::{
    ideas::{IdeaAnswer, IdeaGroup, IdeaRequest},
    Error,
};

layer! {
    fn idea_groups() -> Vec<IdeaGroup> = crate::game::ideas::list;
    fn take_idea_group(group: String) -> () = crate::game::ideas::take_group;
    fn buy_idea(group: String) -> () = crate::game::ideas::buy;
    fn abandon_idea_group(group: String) -> () = crate::game::ideas::abandon_group;
}

pub fn run(request: IdeaRequest, game: &mut (impl Layer + ?Sized)) -> Result<IdeaAnswer, Error> {
    Ok(match request {
        IdeaRequest::List {} => IdeaAnswer::List(game.idea_groups()?),
        IdeaRequest::TakeGroup { group } => IdeaAnswer::TakeGroup(game.take_idea_group(group)?),
        IdeaRequest::Buy { group } => IdeaAnswer::Buy(game.buy_idea(group)?),
        IdeaRequest::AbandonGroup { group } => IdeaAnswer::AbandonGroup(game.abandon_idea_group(group)?),
    })
}
