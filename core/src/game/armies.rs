//! The player's armies and the armies they can see: reading them, the routes
//! the game would choose, and moving and stopping them.
//!
//! An extension's order installs a route that stays its own until someone
//! else changes it. A remote client's order is the player's: it takes the
//! army back from the extension that was moving it, and extensions hear that
//! the player gave the army an order, exactly once
//! (ADR remote-control-uses-capabilities).
use super::{events, orders, with_state, world, Caller, GResult, Session, Tag, View};
use crate::{memory::Memory, objects::campaign::unit};
use eu4_harmony_checks::armies as checks;
use eu4_harmony_remote::armies::args;
use eu4_harmony_remote::ArmyId;
use eu4_harmony_remote::ProvinceId;

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

/// A land army, or a fleet, of which the ID, owner, `land: false`, its
/// province, its ships (as `regiments`) and its route are read.
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
    /// An army's fleet while it is aboard one.
    pub aboard: Option<u64>,
    /// A fleet's regiments aboard.
    pub carrying: u32,
}

impl Army {
    /// What of its state stops an order (`eu4_harmony_checks::armies`).
    pub fn condition(&self) -> checks::Condition {
        checks::Condition {
            in_battle: self.in_battle,
            sieging: self.sieging,
            retreating: self.retreating,
            exiled: self.exiled,
            movement_locked: self.movement_locked,
            attached: self.attached,
            has_followers: self.has_followers,
            busy: self.busy,
            regiments: self.regiments,
        }
    }
}

/// An own army with its route, as `caller` sees it.
fn own(m: &Memory, caller: Caller, address: usize, id: u64) -> GResult<Army> {
    let mut army = world::army_record(m, &super::special_countries(m)?, address)?;
    {
        let path = orders::read_path(m, address + unit::PATH)?;
        army.route_len = path.ids.len() as u32;
        army.destination = path.ids.last().copied();
        army.route_source = with_state(|s| s.orders.route_source(m, address, id, caller))?;
    }
    Ok(army)
}

/// One of the player's armies or fleets.
pub fn army(Session { m, ctx, caller, .. }: Session, id: u64) -> GResult<Army> {
    let Some((_, address, _)) = world::own_roster(m, &ctx)?.into_iter().find(|(i, _, _)| *i == id) else {
        return Err(checks::not_own(id).into());
    };
    own(m, caller, address, id)
}

/// All the player's armies and fleets.
pub fn own_armies(Session { m, ctx, caller, .. }: Session) -> GResult<Vec<Army>> {
    world::own_roster(m, &ctx)?.into_iter().map(|(id, address, _)| own(m, caller, address, id)).collect()
}

/// Land armies in a province: in the player's view those the player can see,
/// none under fog, and [`Status::NotReady`] while visibility is not current,
/// and of another country's only whether it fights, besieges or retreats,
/// which the map shows; in the full view every one, as the game keeps it.
pub fn in_province(Session { m, ctx, view, .. }: Session, province: u32) -> GResult<Vec<Army>> {
    if view == View::Player && !events::visibility_current(m, &ctx) {
        return Err(checks::visibility_not_current().into());
    }
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let shown = |a: Army| match view == View::Player && a.owner.index() != ctx.player.index() {
        true => Army { exiled: false, movement_locked: false, attached: false, has_followers: false, busy: false, ..a },
        false => a,
    };
    Ok(world::province_armies(m, &ctx, address, view)?.into_iter().map(|(a, _)| shown(a)).collect())
}

/// Provinces on the current route of an own army or fleet, in order.
pub fn route(Session { m, ctx, .. }: Session, id: u64) -> GResult<Vec<u32>> {
    let army = world::own_unit(m, &ctx, id)?;
    Ok(orders::read_path(m, army + unit::PATH)?.ids)
}

/// Whether an own land army may enter a province now (military access and
/// terrain; forts are not considered).
pub fn can_enter(Session { m, ctx, .. }: Session, id: u64, province: u32) -> GResult<bool> {
    let army = world::own_land_army(m, &ctx, id)?;
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    world::can_enter(m, army, address)
}

/// Whether the strait between two neighboring provinces is open to the army.
pub fn strait_open(Session { m, ctx, .. }: Session, id: u64, from: u32, to: u32) -> GResult<bool> {
    let army = world::own_land_army(m, &ctx, id)?;
    with_state(|s| s.geography.strait_open(m, &ctx, army, from, to))
}

/// Whether an own land army may attack a visible army in `province`.
pub fn can_attack(Session { m, ctx, .. }: Session, id: u64, target: u64, province: u32) -> GResult<bool> {
    if !events::visibility_current(m, &ctx) {
        return Err(checks::visibility_not_current().into());
    }
    let army = world::own_land_army(m, &ctx, id)?;
    let address = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let Some((_, unit)) = world::province_armies(m, &ctx, address, View::Player)?.into_iter().find(|(a, _)| a.id == target) else {
        return Err(checks::not_visible(target, province).into());
    };
    Ok(unit != army && world::can_attack(m, army, unit)?)
}

/// Whether an own land army may attack an army the player sees, wherever
/// that is.
pub fn can_attack_seen(Session { m, ctx, .. }: Session, args::CanAttack { army: ArmyId(army), target: ArmyId(target) }: args::CanAttack) -> GResult<bool> {
    if !events::visibility_current(m, &ctx) {
        return Err(checks::visibility_not_current().into());
    }
    let army = world::own_land_army(m, &ctx, army)?;
    // Found by the game's lookup, and seen as army.at sees it: a land army in
    // a province not under fog.
    let Some(other) = world::unit_by_id(m, target)? else {
        return Err(checks::not_seen(target).into());
    };
    let at = m.ptr(other + unit::LOCATION)?;
    if at == 0 || !world::province_armies(m, &ctx, at, View::Player)?.iter().any(|(a, _)| a.id == target) {
        return Err(checks::not_seen(target).into());
    }
    Ok(other != army && world::can_attack(m, army, other)?)
}

/// Whether an own land army may go from a province to a neighboring one
/// now: the neighbor is open to it (military access and terrain; forts are
/// not considered), and so is the strait between them, if any.
pub fn can_step(session: Session, args::CanStep { army: ArmyId(id), from: ProvinceId(from), to: ProvinceId(to) }: args::CanStep) -> GResult<bool> {
    Ok(can_enter(session, id, to)? && strait_open(session, id, from, to)?)
}

/// The route the game itself would choose for an own land army; empty when
/// `goal` is where the army is. [`Status::NotFound`] when there is none.
/// Asked by an extension, [`Status::NotReady`] while the game is paused, as
/// the orders below; remote control asks as the player, paused or not.
pub fn find_route(Session { m, ctx, caller, .. }: Session, args::RouteTo { army: ArmyId(id), province: ProvinceId(goal) }: args::RouteTo) -> GResult<Vec<u32>> {
    let army = world::own_unit(m, &ctx, id)?;
    with_state(|s| s.orders.find_route(m, &ctx, &mut s.geography, army, goal, caller))
}

/// Orders an own army or fleet along the game's route to `goal`. Done means
/// the route is installed, not that the army has arrived. An extension's
/// order is [`Status::NotReady`] while the game is paused; remote control's
/// is the player's, given while the game is paused as the player gives one.
/// With `transports`, an own land army is carried there by the player's
/// fleets, as the player's automatic transport carries it.
pub fn move_to(session: Session, id: u64, goal: u32, transports: bool) -> GResult<()> {
    let Session { m, ctx, .. } = session;
    if transports {
        let army = world::own_land_army(m, &ctx, id)?;
        return orders::transport(&session, army, id, goal);
    }
    let army = world::own_unit(m, &ctx, id)?;
    let ordered = with_state(|s| s.orders.move_army(&session, &mut s.geography, army, id, goal))?;
    ordered.into_iter().for_each(events::raise);
    Ok(())
}

/// Stops an own land army where it is.
pub fn stop(session: Session, args::Stop { army: ArmyId(id) }: args::Stop) -> GResult<()> {
    let Session { m, ctx, .. } = session;
    let army = world::own_land_army(m, &ctx, id)?;
    let ordered = with_state(|s| s.orders.stop_army(&session, army, id))?;
    ordered.into_iter().for_each(events::raise);
    Ok(())
}

/// The own land army shown in the open single-army panel.
pub fn selected(Session { m, ctx, .. }: Session) -> GResult<u64> {
    crate::ui::selected_army(m, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::{byz, tur};
    use crate::game::{
        buffers::{self, Image, Object, Provinces},
        commands::record,
        Fail, Status,
    };
    use crate::objects::{
        campaign::{classes, country, globals},
        list,
    };
    use std::cell::RefCell;

    thread_local! {
        /// The units the game's lookup finds in this test's thread, by ID.
        static UNITS: RefCell<Vec<(u64, usize)>> = const { RefCell::new(Vec::new()) };
    }

    /// The game's lookup of an object by its ID: where a unit keeps it.
    extern "C" fn find_id(key: *const u64) -> usize {
        let id = unsafe { *key };
        UNITS.with(|u| u.borrow().iter().find(|(i, _)| *i == id).map_or(0, |(_, at)| at + unit::ID_HOLDER))
    }

    /// The units: the Ottomans' army 11 in province 1, of `regiments`
    /// regiments, fighting or not; their fleet 12 there; Byzantium's army 21
    /// in province 2.
    struct Units {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn units(regiments: usize, fighting: bool) -> Units {
        let image = Image::new();
        let m = image.memory();
        image.function(world::FIND_ID.rva(), find_id as *const ());
        for rva in globals::SPECIAL_COUNTRIES {
            image.i32(rva, 0);
        }
        let provinces = Provinces::new(&image, 3);
        provinces.owner(1, tur()).owner(2, byz());
        let regiment = Object::new(8);
        let unit = |class: usize, id: u64, owner: Tag, province: u32, regiments: usize| {
            Object::new(0x240)
                .u64(0, m.at(class) as u64)
                .u64(unit::ID, id)
                .u64(unit::OWNER, owner.0)
                .u64(unit::LOCATION, provinces.province(province) as u64)
                .vector(unit::REGIMENTS, &vec![regiment.at() as u64; regiments])
        };
        let army = unit(classes::ARMY, 11, tur(), 1, regiments).u64(unit::BATTLE, fighting.into());
        let (fleet, theirs) = (unit(classes::FLEET, 12, tur(), 1, 1), unit(classes::ARMY, 21, byz(), 2, 1));
        UNITS.with(|u| *u.borrow_mut() = vec![(11, army.at()), (12, fleet.at()), (21, theirs.at())]);
        let second = Object::new(list::NODE_SIZE).u64(list::ITEM, fleet.at() as u64);
        let first = Object::new(list::NODE_SIZE).u64(list::ITEM, army.at() as u64).u64(list::NEXT, second.at() as u64);
        let player = Object::country(tur(), country::UNITS + 8).u64(country::UNITS, first.at() as u64);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(m, provinces.world(), tur(), player.at(), countries.at(), Caller::Remote);
        Units { _image: image, _provinces: provinces, _objects: vec![regiment, army, fleet, theirs, second, first, player, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was ordered") };
        (status, message)
    }

    #[test]
    fn the_player_s_armies_and_fleets_are_read_with_their_routes() {
        let units = units(2, false);
        let all = own_armies(units.session).unwrap();
        let summary: Vec<_> = all.iter().map(|a| (a.id, a.land, a.province, a.regiments, a.route_len, a.route_source)).collect();
        assert_eq!(summary, [(11, true, 1, 2, 0, RouteSource::None), (12, false, 1, 1, 0, RouteSource::None)]);
        assert_eq!(army(units.session, 11).unwrap(), all[0]);
        assert_eq!(army(units.session, 21).unwrap_err().message, "no own army 21");
    }

    #[test]
    fn an_army_without_a_route_is_stopped_where_it_is_with_no_command() {
        let units = units(2, false);
        stop(units.session, args::Stop { army: ArmyId(11) }).unwrap();
        // Moving it to where it is stops it, as the right click there does.
        move_to(units.session, 11, 1, false).unwrap();
        assert!(record::posted().is_empty());
    }

    #[test]
    fn an_order_for_an_army_not_the_player_s_or_one_that_cannot_move_is_refused() {
        let some = units(2, false);
        assert_eq!(refused(stop(some.session, args::Stop { army: ArmyId(12) })), (Status::InvalidArgument, "not a land army".into()));
        assert_eq!(refused(stop(some.session, args::Stop { army: ArmyId(21) })), (Status::NotFound, "no own army 21".into()));
        assert_eq!(refused(move_to(some.session, 99, 2, false)), (Status::NotFound, "no own army 99".into()));
        drop(some);
        assert_eq!(refused(stop(units(2, true).session, args::Stop { army: ArmyId(11) })), (Status::Rejected, "army is in battle".into()));
        assert_eq!(refused(move_to(units(0, false).session, 11, 1, false)), (Status::Rejected, "army has no regiments".into()));
        assert!(record::posted().is_empty());
    }
}
