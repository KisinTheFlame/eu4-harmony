//! The player's armies and the armies they can see: reading them, the routes
//! the game would choose, and moving and stopping them. A move or stop is the
//! player's order: it takes the army back from an extension moving it.
use crate::{choices::Choice, ArmyId, ArmyList, CountryTag, ProvinceId};
use serde::{Deserialize, Serialize};

group! {
    /// The player's armies and the armies they can see.
    "army": ArmyRequest, ArmyAnswer;

    /// One of the player's armies or fleets.
    game Get "get" {
        /// The army.
        army: ArmyId,
    } -> Army, "the army: id, owner, kind (army or navy; of a navy id, owner, kind, province, regiments (its ships) and route are read), province, regiments, blocked (what stops it taking orders now: in_battle, sieging, retreating, exiled, movement_locked, attached, has_followers, busy), route (the provinces still to go, in order), route_by (who set the route: this_mod, other_mod or player_or_game; null without one), aboard (the navy an army is aboard; null on land) and carrying (a navy's regiments aboard)";
    /// The player's armies and fleets.
    game List "list" {} -> Vec<Army>, "the armies and fleets, each as army.get answers it";
    /// The land armies in a province: every one for an extension and in the
    /// full view; in the player's view only those the player sees, none
    /// under fog of war, and not ready while visibility has not caught up
    /// with the date.
    game At "at" {
        /// The province.
        province: ProvinceId,
    } -> Vec<Army>, "the armies there, each as army.get answers it, the route read for the player's own only";
    /// The route the game itself would choose for an army or a fleet of the
    /// player's; not_found, with the reason, when there is none.
    game RouteTo "route_to" {
        /// The player's army or fleet.
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
    /// Whether an army of the player's may attack an army the player sees,
    /// wherever that is.
    game CanAttack "can_attack" {
        /// The player's land army.
        army: ArmyId,
        /// The army to attack.
        target: ArmyId,
    } -> bool, "whether it may";
    /// Orders an army or a fleet of the player's along the game's route, as
    /// the player would, also while the game is paused. Done means the route
    /// is set, not that the army has arrived. An army ordered onto a sea zone
    /// where a fleet of the player's is boards it there; armies aboard a
    /// fleet move with it, and an army aboard ordered to a coastal province
    /// lands there. With transports, an army is carried across the sea by
    /// the player's fleets as the player's automatic transport carries it.
    game Move "move" {
        /// The player's army or fleet.
        army: ArmyId,
        /// Where it should go.
        province: ProvinceId,
        /// For a land army: have the player's fleets carry it there, as the
        /// right click's "use transports" does, rather than by land; absent
        /// or false for the route by land.
        transports: Option<bool>,
    } -> (), "nothing";
    /// Stops an army of the player's where it is, as the player would, also
    /// while the game is paused.
    game Stop "stop" {
        /// The player's land army.
        army: ArmyId,
    } -> (), "nothing";
    /// The player's land army shown in the open army panel.
    game Selected "selected" {} -> ArmyId, "the army's id";
    /// What the army or fleet panel's buttons offer for one of the player's
    /// armies or fleets now: split, merge (with the player's others of its
    /// kind in its province), disband, consolidate and drill or stop_drill
    /// for an army, patrol and evade for a fleet; each with whether the
    /// player can, why not as the button's tooltip says, and what it costs.
    game Options "options" {
        /// The player's army or fleet.
        army: ArmyId,
    } -> Vec<ArmyAction>, "the buttons: action (the capability that presses it), can, reasons, cost";
    /// Splits an army or a fleet in half, as the panel's button does: the
    /// new one takes the smaller half.
    game Split "split" {
        /// The player's army or fleet.
        army: ArmyId,
    } -> ArmyId, "the new army or fleet";
    /// Merges armies, or fleets, of the player's in one province into the
    /// first, as the panel's button does with them selected.
    game Merge "merge" {
        /// The player's armies, or fleets, the first the one the others
        /// merge into.
        armies: ArmyList,
    } -> (), "nothing";
    /// Disbands an army or a fleet of the player's, as the panel's button
    /// does once the player confirms.
    game Disband "disband" {
        /// The player's army or fleet.
        army: ArmyId,
    } -> (), "nothing";
    /// Consolidates an army's regiments, as the panel's button does: the
    /// men of weak regiments fill up the others of their kind, and the
    /// emptied ones are removed.
    game Consolidate "consolidate" {
        /// The player's land army.
        army: ArmyId,
    } -> (), "nothing";
    /// Has an army drill, as the panel's button does: it stays where it is.
    game Drill "drill" {
        /// The player's land army.
        army: ArmyId,
    } -> (), "nothing";
    /// Stops an army drilling, as the panel's button does.
    game StopDrill "stop_drill" {
        /// The player's land army.
        army: ArmyId,
    } -> (), "nothing";
    /// Gives a fleet of the player's a mission, as the fleet panel's mission
    /// buttons and then the map do: protect_trade, privateer or
    /// hunt_pirates in a trade node; hunt_fleets, blockade or intercept in
    /// a sea region; patrol or evade towards where the fleet was last moved
    /// (army.move it first). Refused when the game's command refuses it,
    /// with no reason.
    game Mission "mission" {
        /// The player's fleet.
        army: ArmyId,
        /// The mission: protect_trade, privateer, hunt_pirates,
        /// hunt_fleets, blockade, intercept, patrol or evade.
        mission: String,
        /// For protect_trade, privateer and hunt_pirates the trade node's
        /// number, for hunt_fleets, blockade and intercept the sea region's,
        /// as the game numbers them (the index in its list of them).
        target: Option<u32>,
    } -> (), "nothing";
    /// Stops a fleet's mission, as the fleet panel's button does.
    game StopMission "stop_mission" {
        /// The player's fleet.
        army: ArmyId,
    } -> (), "nothing";
}

/// One of the army or fleet panel's buttons.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct ArmyAction {
    /// The capability that presses it: split, merge, disband, consolidate,
    /// drill, stop_drill, or for a fleet the mission patrol or evade.
    pub action: String,
    #[serde(flatten)]
    pub choice: Choice,
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

/// An army, or a navy, of which the ID, owner, kind, province, ships (as
/// regiments) and route are read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Army {
    pub id: ArmyId,
    pub owner: CountryTag,
    pub kind: ArmyKind,
    /// Where it is: a sea zone or a port for a navy.
    pub province: Option<ProvinceId>,
    /// Its regiments; a navy's ships.
    pub regiments: u32,
    /// What stops it taking orders now; empty when it can take them.
    pub blocked: Vec<Blocked>,
    /// The provinces still to go, in order; read for the player's own
    /// armies only.
    pub route: Vec<ProvinceId>,
    /// Who set its route; none without one.
    pub route_by: Option<RouteBy>,
    /// The navy an army is aboard; none on land, and for a navy.
    pub aboard: Option<ArmyId>,
    /// A navy's regiments aboard; 0 for an army.
    pub carrying: u32,
}
