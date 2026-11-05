//! Technology: what the technology view offers the player, the next level of
//! each technology, and researching it with monarch points.
use crate::choices::{Choice, PowerKind};
use serde::{Deserialize, Serialize};

group! {
    /// The technology view: researching technology.
    "technology": TechnologyRequest, TechnologyAnswer;

    /// The player's three technologies, administrative, diplomatic and
    /// military, each with its level and whether the player can research
    /// the next level now with the view's research button, at what cost in
    /// its kind of monarch power. When the player has too few points the
    /// reason is the view's own line of when the player will have them.
    game Options "options" {} -> Vec<Technology>, "one for each of adm, dip, mil: technology, level, can, reasons, cost";

    /// Researches the next level of a technology, as the research button in
    /// the technology view does, paying its cost in monarch points. Refused
    /// with the game's reasons when the player cannot research it now, as
    /// `technology.options` says beforehand.
    game Research "research" {
        /// The technology: adm, dip or mil.
        technology: PowerKind,
    } -> (), "null once the level is researched";
}

/// One of the player's technologies, as the technology view shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Technology {
    /// Which: adm, dip or mil.
    pub technology: PowerKind,
    /// The level the player has.
    pub level: u32,
    /// Whether the player can research the next level now, and its cost;
    /// at the last level the player cannot, and it costs nothing.
    #[serde(flatten)]
    pub choice: Choice,
}
