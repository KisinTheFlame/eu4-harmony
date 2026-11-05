//! Policies: those the policies view offers, and enabling and disabling
//! them.
use eu4_harmony_remote::{
    policies::{Policy, PolicyAnswer, PolicyRequest},
    Error,
};

layer! {
    fn policies() -> Vec<Policy> = crate::game::policies::list;
    fn enable_policy(policy: String) -> () = crate::game::policies::enable;
    fn disable_policy(policy: String) -> () = crate::game::policies::disable;
}

pub fn run(request: PolicyRequest, game: &mut (impl Layer + ?Sized)) -> Result<PolicyAnswer, Error> {
    Ok(match request {
        PolicyRequest::List {} => PolicyAnswer::List(game.policies()?),
        PolicyRequest::Enable { policy } => PolicyAnswer::Enable(game.enable_policy(policy)?),
        PolicyRequest::Disable { policy } => PolicyAnswer::Disable(game.disable_policy(policy)?),
    })
}
