//! Subjects: the interactions with the player's subjects.
use super::countries;
use crate::game::{subjects as layer, Tag};
use eu4_harmony_remote::{
    subjects::{SubjectAnswer, SubjectInteraction, SubjectRequest},
    CountryTag, Error,
};
use eu4_harmony_remote::subjects::args;

layer! {
    run SubjectRequest => SubjectAnswer + countries::Layer;
    fn interactions(subject: Tag) -> Vec<SubjectInteraction> = session layer::options;
    fn interact(subject: Tag, interaction: &str, third_party: Option<Tag>, province: Option<u32>) -> () = session layer::interact;
    Options { subject } => options(game, subject);
    Interact { subject, interaction, third_party, province } => use_interaction(game, subject, &interaction, third_party, province.map(|p| p.0));
}

/// The interactions with the subject, as its subject view offers them.
fn options(game: &mut (impl Layer + countries::Layer + ?Sized), subject: CountryTag) -> Result<Vec<SubjectInteraction>, Error> {
    let subject = game.country(subject)?;
    Ok(game.interactions(subject)?)
}

/// Uses one of the interactions with the subject, on a third country or a
/// province when it takes one.
fn use_interaction(
    game: &mut (impl Layer + countries::Layer + ?Sized),
    subject: CountryTag,
    interaction: &str,
    third_party: Option<CountryTag>,
    province: Option<u32>,
) -> Result<(), Error> {
    let subject = game.country(subject)?;
    let third_party = third_party.map(|t| game.country(t)).transpose()?;
    Ok(game.interact(subject, interaction, third_party, province)?)
}
