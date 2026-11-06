//! Decisions: what the decisions view lists for the player, and taking one.
use eu4_harmony_remote::decisions::{Decision, DecisionAnswer, DecisionRequest};

layer! {
    run DecisionRequest => DecisionAnswer;
    List => fn decisions() -> Vec<Decision> = caller crate::game::decisions::list;
    Take => fn take_decision(decision: String) -> () = caller crate::game::decisions::take;
}
