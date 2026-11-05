//! Subjects: the interactions with the player's subjects.
use super::countries;
use crate::game::{subjects as layer, Tag};
use eu4_harmony_remote::{
    subjects::{SubjectAnswer, SubjectInteraction, SubjectRequest},
    Error,
};

layer! {
    fn interactions(subject: Tag) -> Vec<SubjectInteraction> = layer::options;
    fn interact(subject: Tag, interaction: &str, third_party: Option<Tag>) -> () = layer::interact;
}

pub fn run(request: SubjectRequest, game: &mut (impl Layer + countries::Layer + ?Sized)) -> Result<SubjectAnswer, Error> {
    Ok(match request {
        SubjectRequest::Options { subject } => {
            let subject = game.country(subject)?;
            SubjectAnswer::Options(game.interactions(subject)?)
        }
        SubjectRequest::Interact { subject, interaction, third_party } => {
            let subject = game.country(subject)?;
            let third_party = third_party.map(|t| game.country(t)).transpose()?;
            game.interact(subject, &interaction, third_party)?;
            SubjectAnswer::Interact(())
        }
    })
}
