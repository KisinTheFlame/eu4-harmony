//! Estates: the player's estates and the crown land's interactions, granting
//! and revoking privileges, and the interactions.
use eu4_harmony_remote::estates::{Estate, EstateAnswer, EstateRequest, Interaction};

layer! {
    run EstateRequest => EstateAnswer;
    List => fn estates() -> Vec<Estate> = crate::game::estates::list;
    Interactions => fn estate_interactions() -> Vec<Interaction> = crate::game::estates::interactions_list;
    GrantPrivilege => fn grant_privilege(estate: String, privilege: String) -> () = crate::game::estates::grant_privilege;
    RevokePrivilege => fn revoke_privilege(estate: String, privilege: String) -> () = crate::game::estates::revoke_privilege;
    Interact => fn estate_interact(interaction: String) -> () = crate::game::estates::interact;
}
