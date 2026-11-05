//! Missions: the player's missions as the missions view shows them, and
//! completing one as its button does.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The player's missions.
    "mission": MissionRequest, MissionAnswer;

    /// The player's missions, by content key, column by column as the
    /// missions view shows them, each with whether it is completed and
    /// whether the player can complete it now, and why not: its conditions
    /// that do not hold, as its tooltip lists them, and the missions before
    /// it not completed.
    game List "list" {} -> Vec<Mission>, "the missions: mission (its content key), name, column, completed, can, reasons, cost";

    /// Completes a mission, as its button in the missions view does, which
    /// has the game carry out its effects. `rejected` with the game's
    /// reasons when the game would not let the player complete it now;
    /// `not_found` for a key that names none of the player's missions.
    game Complete "complete" {
        /// The mission's content key.
        mission: String,
    } -> (), "null once the game has completed it";
}

/// A mission of the player's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Mission {
    /// Its content key, which the game's or a mod's mission files give it.
    pub mission: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The column of the missions view it is in, from 0 at the left.
    pub column: u32,
    /// The player has completed it.
    pub completed: bool,
    /// Whether the player can complete it now, and why not.
    #[serde(flatten)]
    pub choice: Choice,
}
