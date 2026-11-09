//! Subjects: the interactions with the player's subjects.
use eu4_harmony_remote::subjects::args;
use eu4_harmony_remote::subjects::{SubjectAnswer, SubjectInteraction, SubjectRequest};

layer! {
    run SubjectRequest => SubjectAnswer;
    Options => fn subject_interactions -> Vec<SubjectInteraction> = crate::game::subjects::options;
    Interact => fn interact -> () = crate::game::subjects::interact;
}
