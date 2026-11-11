//! The Celestial Empire: its reforms and its emperor's decrees as the
//! Celestial Empire's view lists them, and passing a reform and enacting a
//! decree as its buttons do.
use crate::choices::Choice;
use crate::empire::ImperialReform;
use crate::{CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The Celestial Empire's view: its reforms and decrees.
    "celestial_empire": CelestialEmpireRequest, CelestialEmpireAnswer;

    /// The Celestial Empire as its view shows it: its emperor, the mandate,
    /// and the reforms the view lists, those whose conditions for being
    /// offered hold, each with whether it is passed and whether the player
    /// can pass it now, the game's reasons when not and its cost in mandate
    /// and stability.
    game Reforms "reforms" {} -> CelestialReforms, "emperor, mandate, reforms: reform (its content key), name, branch (null), passed, can, reasons, cost";

    /// Passes one of the Celestial Empire's reforms, as its button in the
    /// view does, paying its mandate and stability. Only the emperor can.
    /// `rejected` with the game's reasons when the game would not let the
    /// player pass it now; `not_found` for a key that names no reform the
    /// view lists.
    game PassReform "pass_reform" {
        /// The reform's content key, such as `seaban_decision`.
        reform: String,
    } -> (), "null once the reform is passed";

    /// The emperor's decrees the view lists, those whose conditions for
    /// being offered hold, each with whether the player can enact it now,
    /// the game's reasons when not and its cost in meritocracy; and the
    /// decree in force, until when.
    game Decrees "decrees" {} -> CelestialDecrees, "in_force, until, decrees: decree (its content key), name, can, reasons, cost";

    /// Enacts one of the emperor's decrees, as its button in the view does,
    /// paying its meritocracy; it is in force for its time, and no other
    /// can be enacted until then. Only the emperor can. `rejected` with the
    /// game's reasons when the game would not let the player enact it now;
    /// `not_found` for a key that names no decree the view lists.
    game EnactDecree "enact_decree" {
        /// The decree's content key, such as `expand_bureaucracy_decree`.
        decree: String,
    } -> (), "null once the decree is in force";
}

/// The Celestial Empire's view: its reforms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct CelestialReforms {
    /// The emperor; none while there is no Celestial Emperor.
    pub emperor: Option<CountryTag>,
    /// The mandate, from 0 to 100.
    pub mandate: f64,
    /// The reforms the view lists, in the order the game loads them.
    pub reforms: Vec<ImperialReform>,
}

/// The Celestial Empire's view: its decrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct CelestialDecrees {
    /// The decree in force, by its content key; none while none is.
    pub in_force: Option<String>,
    /// The day the decree in force ends; none while none is.
    pub until: Option<Date>,
    /// The decrees the view lists.
    pub decrees: Vec<Decree>,
}

/// One of the emperor's decrees, as the view lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Decree {
    /// Its content key, which the game's or a mod's decree files give it.
    pub decree: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// Whether the player can enact it now; the reasons are the game's.
    #[serde(flatten)]
    pub choice: Choice,
}
