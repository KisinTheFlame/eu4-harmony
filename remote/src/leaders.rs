//! Leaders: what the military view offers the player to recruit, the
//! player's leaders, and giving one to an army or a fleet, as the army and
//! fleet panels do.
use crate::{choices::Choice, ArmyId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's leaders: recruiting, assigning and dismissing them.
    "leader": LeaderRequest, LeaderAnswer;

    /// What the military view offers the player to recruit now: a general,
    /// an admiral, a conquistador and an explorer, each with whether the
    /// player can, why not as its tooltip says, and what it costs.
    game Options "options" {} -> Vec<LeaderOption>, "one for each kind, in the order general, admiral, conquistador, explorer: kind, can, reasons, cost";
    /// Recruits a leader of a kind, as the military view's button does, at
    /// its cost, with a name the game picks. Refused with the game's
    /// reasons when leader.options says the player cannot.
    game Recruit "recruit" {
        /// The kind: general, admiral, conquistador or explorer.
        kind: String,
    } -> Leader, "the new leader, as leader.list answers it";
    /// The player's leaders.
    game List "list" {} -> Vec<Leader>, "the leaders: id, name, kind, fire, shock, maneuver, siege, army (the army or fleet it leads; null for none)";
    /// Gives one of the player's leaders to one of the player's armies or
    /// fleets, as the panel's leader list does: a general or a conquistador
    /// to an army, an admiral or an explorer to a fleet. The leader it had
    /// before is left without one, and the army the leader led before
    /// without a leader.
    game Assign "assign" {
        /// The leader, by its id in leader.list.
        leader: u32,
        /// The army or fleet.
        army: ArmyId,
    } -> (), "nothing";
    /// Takes the leader from one of the player's armies or fleets, as the
    /// panel's leader button does once the player confirms; the leader stays
    /// the player's.
    game Remove "remove" {
        /// The army or fleet.
        army: ArmyId,
    } -> (), "nothing";
    /// Dismisses one of the player's leaders that leads no army or fleet, as
    /// the military view's button does once the player confirms.
    game Dismiss "dismiss" {
        /// The leader, by its id in leader.list.
        leader: u32,
    } -> (), "nothing";
}

/// A kind of leader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum LeaderKind {
    General,
    Admiral,
    Conquistador,
    Explorer,
}

/// A kind of leader the military view offers to recruit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct LeaderOption {
    pub kind: LeaderKind,
    #[serde(flatten)]
    pub choice: Choice,
}

/// One of the player's leaders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Leader {
    /// Its id, which leader.assign and leader.dismiss take.
    pub id: u32,
    /// Its name, as the game gives it.
    pub name: String,
    pub kind: LeaderKind,
    /// Its pips: fire, shock, maneuver and siege.
    pub fire: i32,
    pub shock: i32,
    pub maneuver: i32,
    pub siege: i32,
    /// The army or fleet it leads; none when it leads none.
    pub army: Option<ArmyId>,
}
