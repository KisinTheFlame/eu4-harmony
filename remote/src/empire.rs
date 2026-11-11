//! The Holy Roman Empire: its reforms as the Empire's view lists them, and
//! passing and revoking them as its buttons do.
use crate::choices::Choice;
use crate::CountryTag;
use serde::{Deserialize, Serialize};

group! {
    /// The Holy Roman Empire's view: its reforms.
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
