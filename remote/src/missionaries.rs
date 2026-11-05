//! Missionaries: sending one to a province and recalling it, as the
//! province view's missionary button does, and the number of the player's
//! missionaries the game sends by itself, as the religion view's automatic
//! missionaries buttons set it.
use crate::{choices::Choice, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's missionaries.
    "missionary": MissionaryRequest, MissionaryAnswer;

    /// What the province view's missionary button offers the player in a
    /// province now: sending a missionary there, and recalling the one the
    /// player has there, each with whether the player can, why not, and the
    /// cost: a missionary, and the monthly upkeep in ducats while it works.
    /// In the player's view another's missionary's progress in another
    /// country's province is left out, as province.get leaves it out.
    game Options "options" {
        /// The province.
        province: ProvinceId,
    } -> MissionaryOptions, "province; working: the player's missionary works there; progress: the conversion's progress in percent; send and recall, each can, reasons, cost";
    /// Sends one of the player's missionaries to a province to convert it to
    /// the player's religion, as the province view's button does. Refused
    /// with the game's reasons when the game would not let the player send
    /// one there now (`missionary.options` says so beforehand).
    game Send "send" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the missionary is on its way";
    /// Recalls the player's missionary from a province, as the province
    /// view's button does once one works there, confirming the game's
    /// question: the progress made is lost.
    game Recall "recall" {
        /// The province.
        province: ProvinceId,
    } -> (), "null once the missionary is recalled";
    /// The player's missionaries, and how many of them the game sends by
    /// itself, as the religion view shows them, with whether the player can
    /// raise or lower that number.
    game Automatic "automatic" {} -> AutomaticMissionaries, "missionaries: the player's; automatic: how many the game sends by itself; raise and lower, each can, reasons, cost";
    /// Has the game send one more of the player's missionaries by itself, as
    /// the religion view's button does. Refused when it already sends them
    /// all.
    game RaiseAutomatic "raise_automatic" {} -> (), "null once the number is raised";
    /// Has the game send one fewer of the player's missionaries by itself.
    /// Refused when it sends none.
    game LowerAutomatic "lower_automatic" {} -> (), "null once the number is lowered";
}

/// What the province view's missionary button offers in a province.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct MissionaryOptions {
    pub province: ProvinceId,
    /// One of the player's missionaries works there.
    pub working: bool,
    /// How far the conversion has come, in percent, while a missionary
    /// works there, the player's or another country's; null when none does.
    pub progress: Option<f64>,
    /// Sending one of the player's missionaries there.
    pub send: Choice,
    /// Recalling the player's missionary from there.
    pub recall: Choice,
}

/// The player's missionaries, and how many of them the game sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct AutomaticMissionaries {
    /// How many missionaries the player has.
    pub missionaries: u32,
    /// How many of them the game sends by itself.
    pub automatic: u32,
    /// Raising that number by one.
    pub raise: Choice,
    /// Lowering it by one.
    pub lower: Choice,
}
