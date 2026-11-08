//! Ideas: the idea groups of the ideas view, and taking, buying and
//! abandoning them.
use eu4_harmony_remote::ideas::{IdeaAnswer, IdeaGroupOption, IdeaRequest};
use eu4_harmony_remote::ideas::args;

layer! {
    run IdeaRequest => IdeaAnswer;
    List => fn idea_groups -> Vec<IdeaGroupOption> = crate::game::ideas::list;
    TakeGroup => fn take_idea_group -> () = crate::game::ideas::take_group;
    Buy => fn buy_idea -> () = crate::game::ideas::buy;
    AbandonGroup => fn abandon_idea_group -> () = crate::game::ideas::abandon_group;
}
