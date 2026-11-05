//! Estates: the player's estates and the crown land's interactions, granting
//! and revoking privileges, and the interactions.
use eu4_harmony_remote::{
    estates::{Estate, EstateAnswer, EstateInteraction, EstateRequest},
    Error,
};

layer! {
    fn estates() -> Vec<Estate> = crate::game::estates::list;
    fn estate_interactions() -> Vec<EstateInteraction> = crate::game::estates::interactions_list;
    fn grant_privilege(estate: String, privilege: String) -> () = crate::game::estates::grant_privilege;
    fn revoke_privilege(estate: String, privilege: String) -> () = crate::game::estates::revoke_privilege;
    fn estate_interact(interaction: String) -> () = crate::game::estates::interact;
}

pub fn run(request: EstateRequest, game: &mut (impl Layer + ?Sized)) -> Result<EstateAnswer, Error> {
    Ok(match request {
        EstateRequest::List {} => EstateAnswer::List(game.estates()?),
        EstateRequest::Interactions {} => EstateAnswer::Interactions(game.estate_interactions()?),
        EstateRequest::GrantPrivilege { estate, privilege } => EstateAnswer::GrantPrivilege(game.grant_privilege(estate, privilege)?),
        EstateRequest::RevokePrivilege { estate, privilege } => EstateAnswer::RevokePrivilege(game.revoke_privilege(estate, privilege)?),
        EstateRequest::Interact { interaction } => EstateAnswer::Interact(game.estate_interact(interaction)?),
    })
}
