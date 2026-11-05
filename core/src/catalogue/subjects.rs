//! Subjects: the interactions with the player's subjects.
use super::countries;
use crate::game::{subjects as layer, Tag};
use eu4_harmony_remote::{
    subjects::{Interaction, SubjectAnswer, SubjectRequest},
    CountryTag, Error,
};

layer! {
    run SubjectRequest => SubjectAnswer + countries::Layer;
    fn interactions(subject: Tag) -> Vec<Interaction> = layer::options;
    fn interact(subject: Tag, interaction: &str, third_party: Option<Tag>) -> () = layer::interact;
    Options { subject } => options(game, subject);
    Interact { subject, interaction, third_party } => use_interaction(game, subject, &interaction, third_party);
}

/// The interactions with the subject, as its subject view offers them.
fn options(game: &mut (impl Layer + countries::Layer + ?Sized), subject: CountryTag) -> Result<Vec<Interaction>, Error> {
    let subject = game.country(subject)?;
    Ok(game.interactions(subject)?)
}

/// Uses one of the interactions with the subject, on a third country when it
/// takes one.
fn use_interaction(
    game: &mut (impl Layer + countries::Layer + ?Sized),
    subject: CountryTag,
    interaction: &str,
    third_party: Option<CountryTag>,
) -> Result<(), Error> {
    let subject = game.country(subject)?;
    let third_party = third_party.map(|t| game.country(t)).transpose()?;
    Ok(game.interact(subject, interaction, third_party)?)
}
