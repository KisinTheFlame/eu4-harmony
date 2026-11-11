//! What the game's panels say of each thing the player could do there
//! (docs/plans/player-actions.md): whether the player can, why not, what it costs,
//! and whether another country would accept it. Shared by every group whose
//! `options` or `list` read answers them.
use serde::{Deserialize, Serialize};

/// Whether the player can do one thing a panel offers now, why not, and what
/// it costs. A group's entry puts these fields beside its own
/// (`#[serde(flatten)]`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Choice {
    /// The game would let the player do it now.
    pub can: bool,
    /// Why not, in the game's own words as its tooltip gives them, in the
    /// player language, one a line, without color codes or bullets; empty
    /// when the player can, or when the game gives no reason.
    pub reasons: Vec<String>,
    /// What it costs, as the panel shows it; empty when it costs nothing.
    pub cost: Vec<Amount>,
}

impl Choice {
    /// One the player can make, at `cost`.
    pub fn can(cost: Vec<Amount>) -> Self {
        Self { can: true, reasons: Vec::new(), cost }
    }

    /// One the player cannot make, for `reasons`, which would cost `cost`.
    pub fn cannot(reasons: Vec<String>, cost: Vec<Amount>) -> Self {
        Self { can: false, reasons, cost }
    }
}

/// An amount of something the game spends or gives.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Amount {
    pub resource: Resource,
    /// How much, to the thousandth where the game keeps fractions.
    pub amount: f64,
}

impl Amount {
    pub fn new(resource: Resource, amount: f64) -> Self {
        Self { resource, amount }
    }
}

/// What the game spends or gives, as its panels name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Ducats,
    /// Administrative points.
    Adm,
    /// Diplomatic points.
    Dip,
    /// Military points.
    Mil,
    Manpower,
    Sailors,
    Prestige,
    Legitimacy,
    Stability,
    /// Favors with the other country.
    Favors,
    /// A diplomat, sent for a while.
    Diplomat,
    /// A merchant, sent for good.
    Merchant,
    Missionary,
    Colonist,
    /// War score, as the peace view counts it.
    WarScore,
    /// Points of an estate's loyalty.
    Loyalty,
    /// Points of an estate's influence.
    Influence,
    /// Army tradition.
    ArmyTradition,
    /// Navy tradition.
    NavyTradition,
    /// Monthly upkeep in ducats, for as long as it lasts.
    MonthlyDucats,
    /// Reform progress, which government reforms spend.
    ReformProgress,
    /// Points of one of a government mechanic's powers, which its
    /// interactions spend; the entry says which.
    MechanicPower,
    /// Splendor, which the age's abilities cost.
    Splendor,
}

/// A kind of monarch power, as the game's panels name them: what a
/// technology is researched with, what a national focus favors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum PowerKind {
    /// Administrative.
    Adm,
    /// Diplomatic.
    Dip,
    /// Military.
    Mil,
}

impl PowerKind {
    pub const ALL: [PowerKind; 3] = [Self::Adm, Self::Dip, Self::Mil];

    /// The resource a cost in this kind of power is counted in.
    pub fn resource(self) -> Resource {
        match self {
            Self::Adm => Resource::Adm,
            Self::Dip => Resource::Dip,
            Self::Mil => Resource::Mil,
        }
    }
}

/// Whether another country would accept a proposal, as the game's interface
/// tells the player: the verdict, the sum, and each factor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Acceptance {
    /// They would accept it.
    pub accepts: bool,
    /// The sum of the factors, as the interface shows it.
    pub total: f64,
    /// Each factor for or against, in the interface's order.
    pub factors: Vec<Factor>,
}

/// One factor for or against a proposal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Factor {
    /// The factor, as the interface shows it, in the player language,
    /// without color codes.
    pub text: String,
    /// How much it counts for (above 0) or against (below 0).
    pub value: f64,
}
