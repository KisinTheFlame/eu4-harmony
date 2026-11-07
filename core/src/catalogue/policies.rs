//! Policies: those the policies view offers, and enabling and disabling
//! them.
use eu4_harmony_remote::policies::{Policy, PolicyAnswer, PolicyRequest};
use eu4_harmony_remote::policies::args;

layer! {
    run PolicyRequest => PolicyAnswer;
    List => fn policies -> Vec<Policy> = crate::game::policies::list;
    Enable => fn enable_policy -> () = crate::game::policies::enable;
    Disable => fn disable_policy -> () = crate::game::policies::disable;
}
