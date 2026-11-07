//! Estates: the player's estates and the crown land's interactions, granting
//! and revoking privileges, and the interactions.
use eu4_harmony_remote::estates::{Estate, EstateAnswer, EstateInteraction, EstateRequest};
use eu4_harmony_remote::estates::args;

layer! {
    run EstateRequest => EstateAnswer;
    List => fn estates -> Vec<Estate> = crate::game::estates::list;
    Interactions => fn estate_interactions -> Vec<EstateInteraction> = crate::game::estates::interactions_list;
    GrantPrivilege => fn grant_privilege -> () = crate::game::estates::grant_privilege;
    RevokePrivilege => fn revoke_privilege -> () = crate::game::estates::revoke_privilege;
    Interact => fn estate_interact -> () = crate::game::estates::interact;
}
