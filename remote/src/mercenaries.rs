//! Mercenary companies: what the mercenary view offers the player, and
//! hiring one, as the player does there.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The mercenary companies the player can hire.
    "mercenary": MercenaryRequest, MercenaryAnswer;

    /// The companies the mercenary view lists for the player now, each with
    /// whether the player can hire it, why not as its tooltip says, and
    /// what it costs to hire and each month.
    game List "list" {} -> Vec<Company>, "the companies: company (its content key), name, regiments, upkeep (ducats a month), can, reasons, cost";
    /// Hires a company the mercenary view lists, raised in a province the
    /// player picks, as the player does by choosing it in the view and then
    /// a province on the map, at its cost. Refused with the game's reasons
    /// when the player cannot hire it there. The company is raised over
    /// days, as the game raises it, and then shows among the player's
    /// armies.
    game Hire "hire" {
        /// The company, by its content key.
        company: String,
        /// Where to raise it: a province the player controls, within the
        /// company's range.
        province: ProvinceId,
    } -> (), "nothing";
}

/// A mercenary company the mercenary view lists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Company {
    /// Its content key, from the game's or a mod's mercenary companies.
    pub company: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// How many regiments it is raised with.
    pub regiments: u32,
    /// What it costs each month once hired, in ducats.
    pub upkeep: f64,
    /// Whether the player can hire it now, and what hiring costs.
    #[serde(flatten)]
    pub choice: Choice,
}
