//! Ideas: the idea groups of the ideas view, and taking, buying and
//! abandoning them.
use eu4_harmony_remote::ideas::{IdeaAnswer, IdeaGroup, IdeaRequest};

layer! {
    run IdeaRequest => IdeaAnswer;
    List => fn idea_groups() -> Vec<IdeaGroup> = crate::game::ideas::list;
    TakeGroup => fn take_idea_group(group: String) -> () = crate::game::ideas::take_group;
    Buy => fn buy_idea(group: String) -> () = crate::game::ideas::buy;
    AbandonGroup => fn abandon_idea_group(group: String) -> () = crate::game::ideas::abandon_group;
}
