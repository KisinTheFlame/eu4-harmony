//! Government: the player's government reforms and rank, as the government
//! view shows them, and adopting reforms and raising the rank as its buttons
//! do, and its government mechanics: their powers and using their
//! interactions.
use crate::choices::Choice;
use crate::Date;
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

    /// The government mechanics the player's government has, as the
    /// government view shows them: each mechanic's powers, their values and
    /// bounds and the modifiers each gives now, and its interactions the
    /// player is offered, with whether the player can use each now, why
    /// not, what it costs, and the day a cooldown ends. What an interaction
    /// does is the mechanic's script's, and is not given.
    game Mechanics "mechanics" {} -> Vec<Mechanic>, "each mechanic: mechanic (its content key), name, powers (power, name, value, min, max, effects: modifier, value), interactions (interaction, name, can, reasons, cost, cost_power, ready_on)";

    /// Uses an interaction of one of the player's government mechanics, as
    /// its button in the government view does, paying its cost. `rejected`
    /// with the game's reasons when the game would not let the player use it
    /// now, as `government.mechanics` says beforehand; `not_found` for a
    /// mechanic the player's government does not have, or an interaction it
    /// does not offer.
    game Interact "interact" {
        /// The mechanic's content key, such as `mamluk_mechanic`.
        mechanic: String,
        /// The interaction's content key, such as `recruit_from_culture_lands`.
        interaction: String,
    } -> (), "null once the game has carried it out";
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

/// A government mechanic of the player's government.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Mechanic {
    /// Its content key, which the game's or a mod's
    /// `common/government_mechanics` gives it, such as `mamluk_mechanic`.
    pub mechanic: String,
    /// Its name, in the player language.
    pub name: String,
    pub powers: Vec<MechanicPower>,
    /// The interactions it offers the player: those whose conditions for
    /// being offered hold.
    pub interactions: Vec<MechanicInteraction>,
}

/// A mechanic power: a value of the mechanic's between its bounds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct MechanicPower {
    /// Its content key, such as `mamluk_adm_power`.
    pub power: String,
    /// Its name, in the player language.
    pub name: String,
    /// What the player's country has of it now.
    pub value: f64,
    pub min: f64,
    pub max: f64,
    /// What it adds to the country's modifiers now: what its scaled and
    /// range modifiers come to at its value. How the value is turned into
    /// them is the mechanic's script's, and is not given.
    pub effects: Vec<Effect>,
}

/// What something adds to one modifier now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Effect {
    /// The modifier's key, such as `global_unrest`.
    pub modifier: String,
    /// How much it adds, as the game's tooltips total modifiers.
    pub value: f64,
}

/// An interaction a government mechanic offers the player.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct MechanicInteraction {
    /// Its content key, such as `recruit_from_culture_lands`.
    pub interaction: String,
    /// Its name, in the player language.
    pub name: String,
    /// Whether the player can use it now, why not, and what it costs: an
    /// amount of `mechanic_power`, of the power `cost_power` names.
    #[serde(flatten)]
    pub choice: Choice,
    /// The power its cost is paid in, by its content key; none when it
    /// costs none of the mechanic's powers.
    pub cost_power: Option<String>,
    /// The day its cooldown ends, while one runs.
    pub ready_on: Option<Date>,
}
