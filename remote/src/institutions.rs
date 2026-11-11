//! Institutions: those that have appeared in the world, as the institutions
//! view lists them, and embracing one as its button does.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The institutions view: embracing institutions.
    "institution": InstitutionRequest, InstitutionAnswer;

    /// The institutions that have appeared in the world, in the order the
    /// game keeps them: whether the player has embraced each, how much of
    /// the player's development has it, and what embracing it offers now,
    /// with whether the player can, the game's reasons why not and its cost
    /// in ducats. One embraced already offers nothing more.
    game List "list" {} -> Vec<InstitutionOption>, "the institutions: institution (its content key), name, embraced, share, can, reasons, cost";

    /// Embraces an institution, as the institutions view's button does,
    /// paying its cost in ducats. `rejected` with the game's reasons when
    /// the game would not let the player embrace it now, as
    /// `institution.list` says beforehand, or when the player has embraced
    /// it already; `not_found` for a key that names no institution.
    game Embrace "embrace" {
        /// The institution's content key, such as `renaissance`.
        institution: String,
    } -> (), "null once the game has embraced it";
}

/// An institution of the institutions view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct InstitutionOption {
    /// Its content key, which the game's or a mod's institution files give
    /// it.
    pub institution: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The player has embraced it.
    pub embraced: bool,
    /// How much of the player's development has it, in percent, as the view
    /// counts it toward embracing it.
    pub share: f64,
    /// Whether the player can embrace it now, why not, and what it costs.
    #[serde(flatten)]
    pub choice: Choice,
}
