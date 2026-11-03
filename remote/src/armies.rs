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
    } -> Army, "the army: id, owner, kind (army or navy; of a navy only id, owner and kind are read), province, regiments, blocked (what stops it taking orders now: in_battle, sieging, retreating, exiled, movement_locked, attached, has_followers, busy), route (the provinces still to go, in order) and route_by (who set the route: this_mod, other_mod or player_or_game; null without one)";
    /// The player's armies and fleets.
    game List "list" {} -> Vec<Army>, "the armies and fleets, each as army.get answers it";
    /// The land armies the player sees in a province; none under fog of
    /// war. Not ready while visibility has not caught up with the date.
    game At "at" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Army>, "the armies there, each as army.get answers it, the route read for the player's own only";
    /// The route the game itself would choose for an army of the player's;
    /// not_found, with the reason, when there is none.
    game RouteTo "route_to" {
        /// The player's land army.
        army: ArmyId,
        /// Where it should go.
        province: ProvinceId,
    } -> Vec<ProvinceId>, "the provinces on the route, in order; empty when the army is there already";
    /// Whether an army of the player's may go from a province to a
    /// neighboring one now: military access and terrain, and the strait
    /// between them, if any, open to it; forts are not considered.
    game CanStep "can_step" {
        /// The player's land army.
        army: ArmyId,
        /// The province it would go from.
        from: ProvinceId,
        /// The neighboring province it would go to.
        to: ProvinceId,
    } -> bool, "whether it may";
    /// Where an army of the player's may get to from a province in at most
    /// a number of steps, each one it may take now as army.can_step says:
    /// military access and terrain, and straits open to it; forts are not
    /// considered. One call in place of a can_step for each pair of
    /// neighbors.
    game Range "range" {
        /// The player's land army.
        army: ArmyId,
        /// The province the steps are counted from.
        province: ProvinceId,
        /// The most steps.
        steps: u32,
    } -> Vec<Reach>, "the provinces it may get to, the one counted from first, then by steps and number: province, steps, and next, the provinces of the range it may step to from there";
    /// Whether an army of the player's may attack an army the player sees,
    /// wherever that is.
    game CanAttack "can_attack" {
        /// The player's land army.
        army: ArmyId,
        /// The army to attack.
        target: ArmyId,
    } -> bool, "whether it may";
    /// Orders an army of the player's along the game's route, as the player
    /// would, also while the game is paused. Done means the route is set, not
    /// that the army has arrived.
    game Move "move" {
        /// The player's land army.
        army: ArmyId,
        /// Where it should go.
        province: ProvinceId,
    } -> (), "nothing";
    /// Stops an army of the player's where it is, as the player would, also
    /// while the game is paused.
    game Stop "stop" {
        /// The player's land army.
        army: ArmyId,
    } -> (), "nothing";
    /// The player's land army shown in the open army panel.
    game Selected "selected" {} -> ArmyId, "the army's id";
}

/// Whether an army fights on land or at sea.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum ArmyKind {
    Army,
    Navy,
}

/// What stops an army taking orders now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Blocked {
    InBattle,
    Sieging,
    Retreating,
    Exiled,
    MovementLocked,
    /// It is attached to another army.
    Attached,
    /// Other armies are attached to it.
    HasFollowers,
    /// It runs a task of the game's own (such as hunting rebels) or is
    /// otherwise not idle.
    Busy,
}

/// Who set the route an army follows, as the caller sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum RouteBy {
    /// The calling extension, and nobody changed it since.
    ThisMod,
    /// Another extension, which keeps moving the army until someone else
    /// orders it.
    OtherMod,
    /// The player (remote control included) or the game itself.
    PlayerOrGame,
}

/// An army, or a navy, of which only the ID, owner and kind are read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Army {
    pub id: ArmyId,
    pub owner: CountryTag,
    pub kind: ArmyKind,
    /// Where it is; none for a navy.
    pub province: Option<ProvinceId>,
    pub regiments: u32,
    /// What stops it taking orders now; empty when it can take them.
    pub blocked: Vec<Blocked>,
    /// The provinces still to go, in order; read for the player's own
    /// armies only.
    pub route: Vec<ProvinceId>,
    /// Who set its route; none without one.
    pub route_by: Option<RouteBy>,
}

/// A province an army may get to within army.range's steps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Reach {
    pub province: ProvinceId,
    /// The fewest steps to it.
    pub steps: u32,
    /// The provinces of the range the army may step to from it.
    pub next: Vec<ProvinceId>,
}
