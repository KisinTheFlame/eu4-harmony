//! Policies: those the policies view offers, and enabling and disabling
//! them.
use eu4_harmony_remote::policies::{Policy, PolicyAnswer, PolicyRequest};

layer! {
    run PolicyRequest => PolicyAnswer;
    List => fn policies() -> Vec<Policy> = session crate::game::policies::list;
    Enable => fn enable_policy(policy: String) -> () = session crate::game::policies::enable;
    Disable => fn disable_policy(policy: String) -> () = session crate::game::policies::disable;
}
