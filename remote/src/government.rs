//! Government: the player's government reforms and rank, as the government
//! view shows them, and adopting reforms and raising the rank as its buttons
//! do.
use crate::choices::Choice;
use serde::{Deserialize, Serialize};

group! {
    /// The player's government: its reforms and its rank.
    "government": GovernmentRequest, GovernmentAnswer;

    /// The player's government as the government view shows it: its type,
    /// its rank (1 a duchy, 2 a kingdom, 3 an empire in the unmodded game),
    /// the reform progress, and whether the player can raise the rank now,
    /// with the game's reasons when not.
    game Options "options" {} -> Government, "government (the type's content key), rank, reform_progress, reforms_adopted, raise_rank: can, reasons, cost";

    /// The government reforms the government view offers, tier by tier: in
    /// each tier the reforms whose conditions for being offered hold and the
    /// one the player has adopted there, with whether the player can adopt
    /// each now, why not, and its cost in reform progress. Adopting one in a
    /// tier the player has reached changes the reform there; the first tier
    /// not reached is the one the player can enact next.
    game Reforms "reforms" {} -> Vec<ReformTier>, "the tiers: tier (its content key), reached, reforms: reform (its content key), name, adopted, can, reasons, cost";

    /// Adopts a government reform, as its button in the government view
    /// does: enacting it in the next tier, or changing the reform of a tier
    /// already reached to it, paying its reform progress. `rejected` with
    /// the game's reasons when the game would not let the player adopt it
    /// now; `not_found` for a key that names no reform of the player's
    /// government.
    game AdoptReform "adopt_reform" {
        /// The reform's content key, such as `centralize_reform`.
        reform: String,
    } -> (), "null once the game has adopted it";

    /// Raises the government's rank by one, as the government view's button
    /// does (to a kingdom, to an empire), with the prestige the game gives
    /// for it. `rejected` with the game's reasons when the game would not
    /// let the player raise it now.
    game RaiseRank "raise_rank" {} -> (), "null once the game has raised it";
}

/// The player's government.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Government {
    /// The government type's content key, such as `monarchy`.
    pub government: String,
    /// The government's rank: 1 a duchy, 2 a kingdom, 3 an empire in the
    /// unmodded game.
    pub rank: u32,
    /// The reform progress, which adopting reforms spends.
    pub reform_progress: f64,
    /// The reforms the player has, by content key, the government's own
    /// mechanics among them, in the order the game keeps them.
    pub reforms_adopted: Vec<String>,
    /// Whether the player can raise the rank now, and why not.
    pub raise_rank: Choice,
}

/// A tier of reforms.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ReformTier {
    /// Its content key, which the government type's file gives it.
    pub tier: String,
    /// The player has adopted a reform in it.
    pub reached: bool,
    pub reforms: Vec<Reform>,
}

/// A reform the government view offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Reform {
    /// Its content key, which the game's or a mod's reform files give it.
    pub reform: String,
    /// Its name, as the view shows it, in the player language.
    pub name: String,
    /// The player has adopted it.
    pub adopted: bool,
    /// Whether the player can adopt it now, why not, and its cost.
    #[serde(flatten)]
    pub choice: Choice,
}
