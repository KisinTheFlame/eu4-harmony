//! The Holy Roman Empire: its reforms and the imperial incident in progress
//! as the Empire's view lists them, and passing and revoking reforms and
//! picking an incident's option as its buttons do.
use crate::choices::Choice;
use crate::{CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The Holy Roman Empire's view: its reforms and its incidents.
    "empire": EmpireRequest, EmpireAnswer;

    /// The Empire as its view shows it: its emperor, its imperial
    /// authority, and the reforms the view lists, those whose conditions for
    /// being offered hold, each with whether it is passed and whether the
    /// player can pass it now, the game's reasons when not and its cost in
    /// imperial authority: all of it, which must be at least the least a
    /// reform is passed at; and whether the player can revoke the last
    /// reform passed.
    game Reforms "reforms" {} -> EmpireReforms, "emperor, authority, reforms: reform (its content key), name, branch, passed, can, reasons, cost; revoke: can, reasons, cost";

    /// Passes one of the Empire's reforms, as its button in the Empire's
    /// view does, which takes all the imperial authority. Only the emperor
    /// can.
    /// `rejected` with the game's reasons when the game would not let the
    /// player pass it now; `not_found` for a key that names no reform the
    /// view lists.
    game PassReform "pass_reform" {
        /// The reform's content key, such as `emperor_reichsreform`.
        reform: String,
    } -> (), "null once the reform is passed";

    /// Revokes the last reform the Empire passed, as the view's button
    /// does, paying imperial authority; an emperor may revoke once in his
    /// life. `rejected` with the game's reasons when the player cannot now.
    game RevokeReform "revoke_reform" {} -> (), "null once the reform is revoked";

    /// The imperial incident in progress, as the Empire's view shows it:
    /// the incident, the day the emperor's choice comes into effect, how many
    /// princes there are, and its options, each with how many princes
    /// support it, whether the emperor favors it, whether it is the player's
    /// own pick, and whether the player can pick it now, with the game's
    /// reasons when not. None but the emperor and the Empire's members can
    /// pick. With no incident in progress, `incident` is null and there are
    /// no options.
    game Incident "incident" {} -> ImperialIncident, "incident (its content key), name, event, ends, princes, options: option, name, votes, favored, picked, can, reasons, cost";

    /// Picks an option of the imperial incident in progress, as its button
    /// in the Empire's view does: a member supports it, the emperor favors
    /// it, and either may change the pick while the incident lasts.
    /// `rejected` with the game's reasons when the player cannot pick it now;
    /// `not_found` for an option the incident does not have, or with no
    /// incident in progress.
    game PickIncidentOption "pick_incident_option" {
        /// The option's number, from empire.incident.
        option: u32,
    } -> (), "null once the option is picked";
}

/// The Empire's view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct EmpireReforms {
    /// The emperor; none once the Empire is gone.
    pub emperor: Option<CountryTag>,
    /// The imperial authority, from 0 to 100.
    pub authority: f64,
    /// The reforms the view lists, in the order the game loads them.
    pub reforms: Vec<ImperialReform>,
    /// Whether the player can revoke the last reform passed now.
    pub revoke: Choice,
}

/// One of an empire's reforms, as its view lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ImperialReform {
    /// Its content key, which the game's or a mod's imperial reform files
    /// give it.
    pub reform: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The branch of the view it is in, as its `gui_container` names it
    /// (`mainline`, `left_branch`, `right_branch`); none for one without.
    pub branch: Option<String>,
    /// The empire has passed it.
    pub passed: bool,
    /// Whether the player can pass it now; the reasons are the game's.
    #[serde(flatten)]
    pub choice: Choice,
}

/// The imperial incident in progress, as the Empire's view shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ImperialIncident {
    /// Its content key, which the game's or a mod's imperial incident files
    /// give it; none while no incident is in progress.
    pub incident: Option<String>,
    /// Its name, as the view shows it, in the player language; empty with
    /// none.
    pub name: String,
    /// The id of the event whose options are the incident's, fired when it
    /// ends, such as `incidents_bur_inheritance.2`; none with no incident.
    pub event: Option<String>,
    /// The day the emperor's choice comes into effect; none with no
    /// incident.
    pub ends: Option<Date>,
    /// How many princes the Empire has, of whom each option's votes are.
    pub princes: u32,
    /// Its options, in the order the view lists them.
    pub options: Vec<IncidentOption>,
}

/// One of an imperial incident's options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct IncidentOption {
    /// What empire.pick_incident_option takes.
    pub option: u32,
    /// Its text, as the view shows it, in the player language.
    pub name: String,
    /// How many of the Empire's princes support it.
    pub votes: u32,
    /// The emperor favors it.
    pub favored: bool,
    /// It is the player's own pick: the option the player supports, or as
    /// the emperor favors.
    pub picked: bool,
    /// Whether the player can pick it now; the reasons are the game's.
    #[serde(flatten)]
    pub choice: Choice,
}
