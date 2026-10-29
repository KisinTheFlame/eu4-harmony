//! The player's armies and the armies they can see: reading them, the routes
//! the game would choose, and moving and stopping them. A move or stop is the
//! player's order: it takes the army back from an extension moving it.
use crate::{ArmyId, CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's armies and the armies they can see.
    "army": ArmyRequest, ArmyAnswer;

    /// One of the player's armies or fleets.
    game Get "get" {
        /// The army.
        army: ArmyId,
    } -> Army, "the army";
    /// The player's armies and fleets.
    game List "list" {} -> Vec<Army>, "the armies and fleets";
    /// The land armies in a province the player can see; none under fog of
    /// war. Not ready while visibility is not current.
    game InProvince "in_province" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Army>, "the armies there";
    /// The provinces on the route an army of the player's is following.
    game Route "route" {
        /// The player's land army.
        army: ArmyId,
    } -> Vec<ProvinceId>, "the provinces still to go, in order";
    /// Whether an army of the player's may enter a province now (military
    /// access and terrain; forts are not considered).
    game CanEnter "can_enter" {
        /// The player's land army.
        army: ArmyId,
        /// The province.
        province: ProvinceId,
    } -> bool, "whether it may";
    /// Whether the strait between two neighboring provinces is open to an
    /// army of the player's.
    game StraitOpen "strait_open" {
        /// The player's land army.
        army: ArmyId,
        /// One side of the strait.
        from: ProvinceId,
        /// The other side.
        to: ProvinceId,
    } -> bool, "whether it is open";
    /// Whether an army of the player's may attack an army it can see in a
    /// province.
    game CanAttack "can_attack" {
        /// The player's land army.
        army: ArmyId,
        /// The army to attack.
        target: ArmyId,
        /// Where the target is.
        province: ProvinceId,
    } -> bool, "whether it may";
    /// The route the game itself would choose for an army of the player's.
    game FindRoute "find_route" {
        /// The player's land army.
        army: ArmyId,
        /// Where it should go.
        goal: ProvinceId,
    } -> Vec<ProvinceId>, "the provinces on the route, in order; empty when the army is there already";
    /// Orders an army of the player's along the game's route, as the player
    /// would, also while the game is paused. Done means the route is set, not
    /// that the army has arrived.
    game Move "move" {
        /// The player's land army.
        army: ArmyId,
        /// Where it should go.
        goal: ProvinceId,
    } -> (), "nothing";
    /// Stops an army of the player's where it is, as the player would, also
    /// while the game is paused.
    game Stop "stop" {
        /// The player's land army.
        army: ArmyId,
    } -> (), "nothing";
    /// The player's land army shown in the open army panel.
    game Selected "selected" {} -> ArmyId, "the army";
}

/// Who set the route an army is following.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteSource {
    /// The army has no route.
    None,
    /// The player (remote control included) or the game itself.
    External,
    /// An extension, which keeps moving the army until someone else orders it.
    Extension,
}

/// A land army, or a fleet, of which only the ID and owner are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Army {
    pub id: ArmyId,
    pub owner: CountryTag,
    /// Where it is; none for a fleet.
    pub province: Option<ProvinceId>,
    pub land: bool,
    pub in_battle: bool,
    pub sieging: bool,
    pub retreating: bool,
    pub exiled: bool,
    pub movement_locked: bool,
    /// Attached to another army.
    pub attached: bool,
    /// Other armies are attached to this one.
    pub has_followers: bool,
    /// Running a task of the game's own (such as hunting rebels) or otherwise
    /// not idle.
    pub busy: bool,
    pub regiments: u32,
    /// Provinces left on its route; read for the player's own armies only.
    pub route_len: u32,
    /// The last province of its route.
    pub destination: Option<ProvinceId>,
    pub route_source: RouteSource,
}
