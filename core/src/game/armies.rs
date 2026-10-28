//! The player's armies and the armies they can see: reading them, the routes
//! the game would choose, and moving and stopping them.
//!
//! An extension's order installs a route that stays its own until someone
//! else changes it. A remote client's order is the player's: it takes the
//! army back from the extension that was moving it, and extensions hear that
//! the player gave the army an order, exactly once (ADR 0015).
use super::{context, events, fail, game, orders, with_state, world, Caller, GResult, Status, Tag};
use crate::{memory::Memory, objects::campaign::unit};

/// Who set the route an army is following, as the caller sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteSource {
    /// The army has no route.
    #[default]
    None,
    /// The player (remote control included) or the game itself.
    External,
    /// The calling extension, and nobody changed it since.
    Own,
    /// An extension other than the caller.
    OtherExtension,
}

/// A land army; a fleet has only its ID, owner and `land: false`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Army {
    /// Full 64-bit army ID; survives save and load.
    pub id: u64,
    pub owner: Tag,
    pub province: u32,
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
    /// Running a native task (for example hunting rebels) or otherwise not idle.
    pub busy: bool,
    pub regiments: u32,
    /// Provinces left on the current route; 0 when not moving. Set for the
    /// player's own armies only.
    pub route_len: u32,
    /// Last province of the current route.
    pub destination: Option<u32>,
    pub route_source: RouteSource,
}

/// An own army with its route, as `caller` sees it.
fn own(m: &Memory, caller: Caller, address: usize, id: u64) -> GResult<Army> {
    let mut army = world::army_record(m, &super::special_countries(m)?, address)?;
    if army.land {
        let path = orders::read_path(m, address + unit::PATH)?;
        army.route_len = path.ids.len() as u32;
        army.destination = path.ids.last().copied();
        army.route_source = with_state(|s| s.orders.route_source(m, address, id, caller))?;
    }
    Ok(army)
}

/// One of the player's armies or fleets.
pub fn army(caller: Caller, id: u64) -> GResult<Army> {
    let m = game()?;
    let ctx = context(m)?;
    let Some((_, address, _)) = world::own_roster(m, &ctx)?.into_iter().find(|(i, _, _)| *i == id) else {
        return fail(Status::NotFound, format!("no own army {id}"));
    };
    own(m, caller, address, id)
}

/// All the player's armies and fleets.
pub fn own_armies(caller: Caller) -> GResult<Vec<Army>> {
    let m = game()?;
    let ctx = context(m)?;
    world::own_roster(m, &ctx)?.into_iter().map(|(id, address, _)| own(m, caller, address, id)).collect()
}

/// Land armies in a province the player can see; none under fog.
/// [`Status::NotReady`] while visibility is not current.
pub fn in_province(_: Caller, province: u32) -> GResult<Vec<Army>> {
    let m = game()?;
    let ctx = context(m)?;
    if !events::visibility_current(m, &ctx) {
        return fail(Status::NotReady, "visibility is not current yet");
    }
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    Ok(world::province_armies(m, &ctx, address)?.into_iter().map(|(a, _)| a).collect())
}

/// Provinces on the current route of an own land army, in order.
pub fn route(_: Caller, id: u64) -> GResult<Vec<u32>> {
    let m = game()?;
    let ctx = context(m)?;
    let army = world::own_land_army(m, &ctx, id)?;
    Ok(orders::read_path(m, army + unit::PATH)?.ids)
}

/// Whether an own land army may enter a province now (military access and
/// terrain; forts are not considered).
pub fn can_enter(_: Caller, id: u64, province: u32) -> GResult<bool> {
    let m = game()?;
    let ctx = context(m)?;
    let army = world::own_land_army(m, &ctx, id)?;
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    world::can_enter(m, army, address)
}

/// Whether the strait between two neighboring provinces is open to the army.
pub fn strait_open(_: Caller, id: u64, from: u32, to: u32) -> GResult<bool> {
    let m = game()?;
    let ctx = context(m)?;
    let army = world::own_land_army(m, &ctx, id)?;
    with_state(|s| s.geography.strait_open(m, &ctx, army, from, to))
}

/// Whether an own land army may attack a visible army in `province`.
pub fn can_attack(_: Caller, id: u64, target: u64, province: u32) -> GResult<bool> {
    let m = game()?;
    let ctx = context(m)?;
    if !events::visibility_current(m, &ctx) {
        return fail(Status::NotReady, "visibility is not current yet");
    }
    let army = world::own_land_army(m, &ctx, id)?;
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let Some((_, unit)) = world::province_armies(m, &ctx, address)?.into_iter().find(|(a, _)| a.id == target) else {
        return fail(Status::NotFound, format!("no visible army {target} in {province}"));
    };
    Ok(unit != army && world::can_attack(m, army, unit)?)
}

/// The route the game itself would choose for an own land army; empty when
/// `goal` is where the army is. [`Status::NotFound`] when there is none.
pub fn find_route(_: Caller, id: u64, goal: u32) -> GResult<Vec<u32>> {
    let m = game()?;
    let ctx = context(m)?;
    let army = world::own_land_army(m, &ctx, id)?;
    with_state(|s| s.orders.find_route(m, &ctx, &mut s.geography, army, goal))
}

/// Orders an own land army along the game's route to `goal`. Done means the
/// route is installed, not that the army has arrived.
pub fn move_to(caller: Caller, id: u64, goal: u32) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let army = world::own_land_army(m, &ctx, id)?;
    let ordered = with_state(|s| s.orders.move_army(m, &ctx, &mut s.geography, army, id, goal, caller))?;
    ordered.into_iter().for_each(events::raise);
    Ok(())
}

/// Stops an own land army where it is.
pub fn stop(caller: Caller, id: u64) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let army = world::own_land_army(m, &ctx, id)?;
    let ordered = with_state(|s| s.orders.stop_army(m, &ctx, army, id, caller))?;
    ordered.into_iter().for_each(events::raise);
    Ok(())
}

/// The own land army shown in the open single-army panel.
pub fn selected(_: Caller) -> GResult<u64> {
    let m = game()?;
    let ctx = context(m)?;
    crate::ui::selected_army(m, &ctx)
}
