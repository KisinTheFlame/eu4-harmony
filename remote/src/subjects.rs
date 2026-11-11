//! Subjects: the interactions the subjects view offers the player toward one
//! of the player's subjects, each named by the game's key for it, such as
//! `scutage`, and carrying one out, as the view's button does. Annexing and
//! integrating a subject are the diplomacy panel's (`diplomacy.annex`,
//! `diplomacy.integrate`).
use crate::choices::Choice;
use crate::{CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The interactions with the player's subjects.
    "subject": SubjectRequest, SubjectAnswer;

    /// The interactions the game has for the player toward a subject: those
    /// the player's country may use at all, each with whether the player can
    /// now and, when not, the reasons its tooltip gives; one going on is
    /// `active`, and doing it again stops it. A colonial subject whose type
    /// the view lets the player release has `release_colony` among them,
    /// the view's "Give Independence to Colony".
    game Options "options" {
        /// The subject.
        subject: CountryTag,
    } -> Vec<SubjectInteraction>, "the interactions: interaction (its key), name, active, can, reasons, cost";
    /// Carries out an interaction with a subject, or stops one going on, as
    /// the subjects view's button does. `release_colony` gives the colony its
    /// independence, as a free nation. `rejected` when the game does not let
    /// the player, with its reasons; `invalid_argument` for an interaction
    /// the game has not.
    game Interact "interact" {
        /// The subject.
        subject: CountryTag,
        /// The interaction's key, as `subject.options` lists it, such as
        /// `scutage`.
        interaction: String,
        /// The third country some interactions are about.
        third_party: Option<CountryTag>,
        /// The province some interactions are about, as their window has the
        /// player pick it: the one given (`grant_province`), claimed
        /// (`grant_core_claim`), given back (`return_land`), fortified
        /// (`fortify_march`) or taken (`seize_territory`).
        province: Option<ProvinceId>,
    } -> (), "nothing: the game carried it out";
}

/// An interaction with a subject.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct SubjectInteraction {
    /// The game's key for it, such as `scutage` or `place_relative_on_throne`.
    pub interaction: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// It is going on with this subject, and doing it again stops it.
    pub active: bool,
    #[serde(flatten)]
    pub choice: Choice,
}
