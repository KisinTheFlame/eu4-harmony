//! Advisors: the player's advisors and those the court view offers to hire,
//! and hiring, firing and promoting them as its buttons do.
use crate::choices::{Choice, PowerKind};
use serde::{Deserialize, Serialize};

group! {
    /// The player's advisors: hiring, firing and promoting them.
    "advisor": AdvisorRequest, AdvisorAnswer;

    /// The player's advisors, one for each kind of monarch power at most, as
    /// the court view shows them, each with whether the player can fire it
    /// and promote it now, why not as the court view's tooltips say, and
    /// what promoting it costs.
    game List "list" {} -> Vec<HiredAdvisor>, "the advisors: id, name, kind, power, skill, culture, religion, salary, fire (can, reasons, cost), promote (can, reasons, cost)";
    /// The advisors the court view offers the player to hire now, each with
    /// whether the player can hire it, why not, and what hiring it costs in
    /// ducats; hiring one takes the place of the player's advisor of its
    /// kind of monarch power.
    game Options "options" {} -> Vec<AdvisorOption>, "the advisors to hire: id, name, kind, power, skill, culture, religion, salary, can, reasons, cost";
    /// Hires an advisor the court view offers, as its button does, paying
    /// its cost and firing the player's advisor of its kind of monarch power.
    /// Refused with the reasons `advisor.options` gives when the player
    /// cannot; `not_found` for an advisor the court view does not offer.
    game Hire "hire" {
        /// The advisor, by its id in advisor.options.
        advisor: u32,
    } -> (), "null once the game has hired it";
    /// Fires one of the player's advisors, as the court view's button does
    /// once the player confirms. Refused with the game's reasons when the
    /// player cannot fire it yet.
    game Fire "fire" {
        /// The advisor, by its id in advisor.list.
        advisor: u32,
    } -> (), "null once the game has fired it";
    /// Promotes one of the player's advisors by one level of skill, as the
    /// court view's button does once the player confirms, paying its cost in
    /// ducats; its salary grows with its skill. Refused with the game's
    /// reasons when the player cannot.
    game Promote "promote" {
        /// The advisor, by its id in advisor.list.
        advisor: u32,
    } -> (), "null once the game has promoted it";
}

/// An advisor, the player's or one offered to hire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Advisor {
    /// Its id, which the other capabilities of the group take.
    pub id: u32,
    /// Its name, as the court view shows it.
    pub name: String,
    /// Its type's content key, such as `philosopher`.
    pub kind: String,
    /// The kind of monarch power it gives, whose seat it takes.
    pub power: PowerKind,
    /// Its skill, from 1.
    pub skill: u32,
    /// Its culture's and its religion's content keys; none when it has none.
    pub culture: Option<String>,
    pub religion: Option<String>,
    /// What it costs the player each month, in ducats, as the court view
    /// shows it.
    pub salary: f64,
}

/// One of the player's advisors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct HiredAdvisor {
    #[serde(flatten)]
    pub advisor: Advisor,
    /// Whether the player can fire it now, and why not.
    pub fire: Choice,
    /// Whether the player can promote it now, why not, and what it costs.
    pub promote: Choice,
}

/// An advisor the court view offers to hire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct AdvisorOption {
    #[serde(flatten)]
    pub advisor: Advisor,
    /// Whether the player can hire it now, why not, and what it costs.
    #[serde(flatten)]
    pub choice: Choice,
}
