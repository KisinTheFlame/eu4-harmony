//! One army or fleet as the game keeps it: found by its ID with the checks
//! that make it the player's, a land army or one the player sees, and their
//! refusals; its record, place, regiments and the armies aboard a fleet; and
//! the game's own answers of where an army may go and whom it may attack.
//! Visibility follows the game: units in fogged provinces are never read in
//! the player's view.
use super::{armies::Army, country, world, Ctx, GResult, Tag, View, MAX_UNITS};
use crate::{
    containers,
    memory::Memory,
    objects::campaign::{classes, country as country_fields, functions, globals, list, province, route_options, tag, task, unit},
    points::{GameFunction, Point},
};
use eu4_harmony_checks::armies as checks;
use std::collections::BTreeSet;

const CAN_ENTER: GameFunction<9> = GameFunction::at(0x1a2280);
const MILITARY_TAG: GameFunction<2> = GameFunction::at(0x1a8d20);
const CAN_ATTACK: GameFunction<2> = GameFunction::at(0xd54a60);
/// The game's lookup of an object by its ID (the type, u32, then the number).
pub(crate) const FIND_ID: GameFunction<1> = GameFunction::at(0x14cf730);
/// Where the unit constructor registers the unit's ID.
const REGISTER_UNIT: usize = 0xd3b0f8;
/// What the idle task answers: no task kind, and false.
const NO_KIND: usize = 0x8f480;
const IDLE_ANSWERS: [usize; 3] = [NO_KIND, functions::NO, functions::NO];
/// `(fleet, army)`: puts the army aboard the fleet, its fleet set and the
/// army added to the fleet's list (docs/research/units.md).
const LOAD: usize = 0x90ada0;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CAN_ENTER, "48 89 5c 24 08 48 89 6c 24 10 56 57 41 56 48 83", "army access query"),
        Point::keep(MILITARY_TAG, "48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9", "army military tag"),
        Point::keep(CAN_ATTACK, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 80", "attack permission query"),
        Point::keep(FIND_ID, "44 8b 09 41 81 f9 68 12 00 00 76 5c 4c 8b 05 15", "object lookup by ID"),
        Point::keep(REGISTER_UNIT, "48 8d 54 24 48 48 8d 4f 10 e8 8a 40 79 00", "unit registered by its ID").uses(&[unit::ID_HOLDER]),
        Point::keep(NO_KIND, "33 c0 c3", "no task kind"),
        Point::keep(
            LOAD,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b 02 48 8b d9 48 8b ca 48 8b fa ff 50 68 4c 8b 05 b3 50 a3 01 48 89 98 18 02 00 00 49 83 b8 c8 74 00 00 00 75 0e c6 80 12 02 00 00 00 c6 80 0d 02 00 00 00 48 8b b3 20 02 00 00 b9 20 00 00 00 e8 df 84 12 01 48 89 38 48 89 70 08 48 c7 40 10 00 00 00 00 c6 40 18 00 8b 93 28 02 00 00",
            "army put aboard a fleet",
        )
        .uses(&[unit::FLEET, unit::ABOARD + list::TAIL, unit::ABOARD + list::COUNT, list::PREVIOUS, list::NEXT, list::REMOVED]),
        Point::method(classes::ARMY, unit::CAN_ENTER, CAN_ENTER.rva(), "army access method"),
        Point::method(classes::ARMY, unit::MILITARY_TAG, MILITARY_TAG.rva(), "army military tag method"),
        Point::method(classes::IDLE_TASK, task::IDLE_METHODS[0], IDLE_ANSWERS[0], "idle task method"),
        Point::method(classes::IDLE_TASK, task::IDLE_METHODS[1], IDLE_ANSWERS[1], "idle task method"),
        Point::method(classes::IDLE_TASK, task::IDLE_METHODS[2], IDLE_ANSWERS[2], "idle task method"),
    ]
}

/// What a unit is, by its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Army,
    Fleet,
    /// Not a unit the framework knows.
    Other,
}

/// What the object at `unit` is, by its class.
pub fn kind(m: &Memory, unit: usize) -> GResult<Kind> {
    let class = m.ptr(unit)?;
    Ok(match class {
        _ if class == m.at(classes::ARMY) => Kind::Army,
        _ if class == m.at(classes::FLEET) => Kind::Fleet,
        _ => Kind::Other,
    })
}

/// The object at `unit` is a land army.
pub fn is_army(m: &Memory, unit: usize) -> GResult<bool> {
    Ok(kind(m, unit)? == Kind::Army)
}

/// The object at `unit`, which may be none, is a land army of `player`'s.
pub fn is_own_army(m: &Memory, player: Tag, unit: usize) -> GResult<bool> {
    Ok(unit != 0 && is_army(m, unit)? && m.u64(unit + unit::OWNER)? == player.0)
}

/// The army or fleet the game has under `id`, found by the game's own
/// lookup; none when there is no unit with it.
pub fn by_id(m: &Memory, id: u64) -> GResult<Option<usize>> {
    let key = id;
    let holder = FIND_ID.call(m, [&key as *const u64 as usize])?;
    if holder == 0 {
        return Ok(None);
    }
    let unit = holder - unit::ID_HOLDER;
    if kind(m, unit)? == Kind::Other || m.u64(unit + unit::ID)? != id {
        return Ok(None);
    }
    Ok(Some(unit))
}

/// One of the player's armies or fleets, by its ID.
pub fn own(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    match by_id(m, id)? {
        Some(unit) if m.u64(unit + unit::OWNER)? == ctx.player.0 => Ok(unit),
        _ => Err(checks::not_own(id).into()),
    }
}

/// One of the player's land armies, by its ID.
pub fn own_land(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    let unit = own(m, ctx, id)?;
    match is_army(m, unit)? {
        true => Ok(unit),
        false => Err(checks::not_land().into()),
    }
}

/// A land army the player sees, by its ID, wherever it is: in a province
/// not under fog.
pub fn seen(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    let Some(unit) = by_id(m, id)? else {
        return Err(checks::not_seen(id).into());
    };
    let at = m.ptr(unit + unit::LOCATION)?;
    if at == 0 || !in_province(m, ctx, at, View::Player)?.iter().any(|(a, _)| a.id == id) {
        return Err(checks::not_seen(id).into());
    }
    Ok(unit)
}

/// Every own army and fleet: (id, address, land).
pub fn roster(m: &Memory, ctx: &Ctx) -> GResult<Vec<(u64, usize, bool)>> {
    let head = m.ptr(ctx.country + country_fields::UNITS)?;
    let mut ids = BTreeSet::new();
    let mut out = Vec::new();
    for node in containers::chain(m, head, list::NEXT, MAX_UNITS)? {
        let unit = m.ptr(node + list::ITEM)?;
        let id = m.u64(unit + unit::ID)?;
        if id == 0 || !ids.insert(id) || m.u64(unit + unit::OWNER)? != ctx.player.0 {
            return Err("own unit identity or owner".to_string().into());
        }
        let land = match kind(m, unit)? {
            Kind::Other => return Err("unknown own unit class".to_string().into()),
            kind => kind == Kind::Army,
        };
        out.push((id, unit, land));
    }
    if m.ptr(ctx.country + country_fields::UNITS)? != head {
        return Err("own unit list changed while reading".to_string().into());
    }
    Ok(out)
}

/// Land armies in a province, as (record, address): in the player's view
/// none while fog hides it.
pub fn in_province(m: &Memory, ctx: &Ctx, province: usize, view: View) -> GResult<Vec<(Army, usize)>> {
    let hidden = |m, province| Ok::<_, super::Fail>(view == View::Player && world::hidden(m, province)?);
    if hidden(m, province)? {
        return Ok(Vec::new());
    }
    let specials = super::special_countries(m)?;
    let head = m.ptr(province + province::UNITS)?;
    let mut out = Vec::new();
    for node in containers::chain(m, head, list::NEXT, MAX_UNITS)? {
        let item = m.ptr(node + list::ITEM)?;
        match kind(m, item)? {
            Kind::Fleet => continue,
            Kind::Army if m.ptr(item + unit::LOCATION)? == province => {}
            _ => return Err("unexpected unit in province list".to_string().into()),
        }
        country(m, ctx, Tag(m.u64(item + unit::OWNER)?))?;
        out.push((record(m, &specials, item)?, item));
    }
    if m.ptr(province + province::UNITS)? != head || hidden(m, province)? {
        return Err("province units changed while reading".to_string().into());
    }
    Ok(out)
}

/// Snapshot of an army or fleet; the route is filled in by the caller.
pub fn record(m: &Memory, specials: &[i32; 3], unit: usize) -> GResult<Army> {
    let land = is_army(m, unit)?;
    let id = m.u64(unit + unit::ID)?;
    let owner = Tag(m.u64(unit + unit::OWNER)?);
    // Of a fleet: where it is and its ships; what stops it moving is its
    // move command's to check.
    if !land {
        let carrying = aboard(m, unit)?.into_iter().map(|a| regiments(m, a)).sum::<GResult<u32>>()?;
        return Ok(Army {
            id,
            owner,
            regiments: regiments(m, unit)?,
            province: location(m, unit)?,
            in_battle: m.ptr(unit + unit::BATTLE)? != 0,
            carrying,
            ..Default::default()
        });
    }
    let present = |at: usize| -> GResult<bool> { Ok(m.ptr(unit + at)? != 0) };
    let vector = unit::CONTEXT_VECTOR;
    Ok(Army {
        id,
        owner,
        land,
        in_battle: present(unit::BATTLE)?,
        sieging: present(unit::SIEGE)?,
        retreating: m.u8(unit + unit::RETREATING[0])? != 0 || m.u8(unit + unit::RETREATING[1])? != 0,
        exiled: owner.kind() != tag::REBELS && !specials.contains(&(owner.index() as i32)) && m.u8(unit + unit::EXILED)? != 0,
        movement_locked: m.u8(unit + unit::MOVEMENT_LOCKED)? != 0,
        attached: present(unit::ATTACHED_TO)?,
        has_followers: present(unit::FOLLOWERS)?,
        busy: present(unit::CONTEXTS[0])? || present(unit::CONTEXTS[1])? || m.ptr(unit + vector)? != m.ptr(unit + vector + 8)? || busy_mission(m, unit)?,
        regiments: regiments(m, unit)?,
        province: location(m, unit)?,
        aboard: match m.ptr(unit + unit::FLEET)? {
            0 => None,
            fleet => Some(m.u64(fleet + unit::ID)?),
        },
        ..Default::default()
    })
}

/// Province ID of a unit's location; 0 for none.
pub fn location(m: &Memory, unit: usize) -> GResult<u32> {
    let p = m.ptr(unit + unit::LOCATION)?;
    if p == 0 {
        return Ok(0);
    }
    Ok(m.u32(p + province::ID)?)
}

/// An army's regiments, or a fleet's ships.
pub fn regiments(m: &Memory, unit: usize) -> GResult<u32> {
    Ok(containers::span(m.ptr(unit + unit::REGIMENTS)?, m.ptr(unit + unit::REGIMENTS + 8)?, 8, MAX_UNITS)? as u32)
}

/// The armies aboard a fleet.
pub fn aboard(m: &Memory, fleet: usize) -> GResult<Vec<usize>> {
    let count = m.i32(fleet + unit::ABOARD + list::COUNT)?;
    if !(0..=MAX_UNITS as i32).contains(&count) {
        return Err("invalid count of armies aboard".to_string().into());
    }
    let mut node = m.ptr(fleet + unit::ABOARD + list::HEAD)?;
    let mut out = Vec::new();
    for _ in 0..count {
        if node == 0 {
            return Err("list of armies aboard ends early".to_string().into());
        }
        let army = m.ptr(node + list::ITEM)?;
        if !is_army(m, army)? || m.ptr(army + unit::FLEET)? != fleet {
            return Err("unexpected unit aboard a fleet".to_string().into());
        }
        out.push(army);
        node = m.ptr(node + list::NEXT)?;
    }
    Ok(out)
}

/// Native task attached to the army: false for none or the idle default
/// task.
fn busy_mission(m: &Memory, army: usize) -> GResult<bool> {
    let mission = m.ptr(army + unit::TASK)?;
    if mission == 0 {
        return Ok(false);
    }
    if mission != m.ptr(m.at(globals::IDLE_TASK))? || m.ptr(mission)? != m.at(classes::IDLE_TASK) {
        return Ok(true);
    }
    let vt = m.ptr(mission)?;
    for (slot, rva) in task::IDLE_METHODS.into_iter().zip(IDLE_ANSWERS) {
        if m.ptr(vt + slot)? != m.at(rva) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A land army whose methods the framework calls are the game's own.
fn check_army_vtable(m: &Memory, army: usize) -> GResult<()> {
    let vt = m.ptr(army)?;
    if vt != m.at(classes::ARMY) || m.ptr(vt + unit::CAN_ENTER)? != m.at(CAN_ENTER) || m.ptr(vt + unit::MILITARY_TAG)? != m.at(MILITARY_TAG) {
        return Err(checks::not_land().into());
    }
    Ok(())
}

/// The country an army fights for, which a strait's blockade is asked of.
pub(super) fn military_tag(m: &Memory, ctx: &Ctx, army: usize) -> GResult<Tag> {
    check_army_vtable(m, army)?;
    let mut out = 0u64;
    let ptr = &mut out as *mut u64 as usize;
    if MILITARY_TAG.call(m, [army, ptr])? != ptr {
        return Err("military tag getter returned an unexpected pointer".to_string().into());
    }
    let tag = Tag(out);
    if !tag.normal() {
        return Err("army has no military country".to_string().into());
    }
    country(m, ctx, tag)?;
    Ok(tag)
}

/// Whether the land army may enter the province now, as the game answers
/// it (military access and terrain; forts are not considered).
pub fn can_enter(m: &Memory, army: usize, province: usize) -> GResult<bool> {
    check_army_vtable(m, army)?;
    Ok(CAN_ENTER.call(m, [army, province, 0, 0, 0, route_options::NO_TAG as usize, 0, 0, 0])? as u8 != 0)
}

/// Whether the land army may attack the army at `target`, as the game
/// answers it.
pub fn can_attack(m: &Memory, army: usize, target: usize) -> GResult<bool> {
    check_army_vtable(m, army)?;
    Ok(CAN_ATTACK.call(m, [army, target])? as u8 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::{byz, tur};
    use crate::game::{
        buffers::{self, Image, Object, Provinces},
        Caller, Fail, Session, Status,
    };
    use crate::objects::campaign::country;

    /// The Ottomans' army 11 of two regiments and fleet 12 of one ship in
    /// province 1, the army aboard the fleet; Byzantium's army 21 in
    /// province 2, under fog when `fog`; and an army 31 of Byzantium's that
    /// the game's lookup finds but no province holds.
    struct Units {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn units(fog: bool) -> Units {
        let image = Image::new();
        let m = image.memory();
        for rva in globals::SPECIAL_COUNTRIES {
            image.i32(rva, 0);
        }
        let provinces = Provinces::new(&image, 3);
        provinces.owner(1, tur()).owner(2, byz());
        if fog {
            provinces.u8(2, province::FOGGED, province::FOGGED_BIT).u8(2, province::HIDDEN, province::HIDDEN_BIT);
        }
        let regiment = Object::new(8);
        let unit = |id: u64, owner: Tag, land: bool, province: u32, regiments: usize| {
            let at = if province == 0 { 0 } else { provinces.province(province) as u64 };
            buffers::unit(m, id, owner, land, 0x240).u64(unit::LOCATION, at).vector(unit::REGIMENTS, &vec![regiment.at() as u64; regiments])
        };
        let (fleet, stray) = (unit(12, tur(), false, 1, 1), unit(31, byz(), true, 0, 1));
        let army = unit(11, tur(), true, 1, 2).u64(unit::FLEET, fleet.at() as u64);
        let theirs = unit(21, byz(), true, 2, 1);
        let node = |unit: &Object, next: Option<&Object>| {
            Object::new(list::NODE_SIZE).u64(list::ITEM, unit.at() as u64).u64(list::NEXT, next.map_or(0, |n| n.at() as u64))
        };
        let on_board = node(&army, None);
        let fleet = fleet.u64(unit::ABOARD + list::HEAD, on_board.at() as u64).i32(unit::ABOARD + list::COUNT, 1);
        // Province 1's units, and the player's, are the army and then the
        // fleet; province 2's is Byzantium's army.
        let fleet_in_one = node(&fleet, None);
        let army_in_one = node(&army, Some(&fleet_in_one));
        provinces.u64(1, province::UNITS, army_in_one.at() as u64);
        let in_two = node(&theirs, None);
        provinces.u64(2, province::UNITS, in_two.at() as u64);
        let own_fleet = node(&fleet, None);
        let own_army = node(&army, Some(&own_fleet));
        buffers::units(&image, &[&army, &fleet, &theirs, &stray]);
        let player = Object::country(tur(), country::UNITS + 8).u64(country::UNITS, own_army.at() as u64);
        let other = Object::country(byz(), country::UNITS + 8);
        let countries = Object::countries(&[&player, &other]);
        let session = buffers::session(m, provinces.world(), tur(), player.at(), countries.at(), Caller::Remote);
        let objects = vec![regiment, army, fleet, theirs, stray, on_board, fleet_in_one, army_in_one, in_two, own_fleet, own_army, player, other, countries];
        Units { _image: image, _provinces: provinces, _objects: objects, session }
    }

    fn refusal<T: std::fmt::Debug>(result: GResult<T>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was found") };
        (status, message)
    }

    #[test]
    fn the_player_s_own_units_are_found_by_id_and_others_refused() {
        let units = units(false);
        let Session { m, ctx, .. } = units.session;
        let (army, fleet) = (own(m, &ctx, 11).unwrap(), own(m, &ctx, 12).unwrap());
        assert_eq!((kind(m, army).unwrap(), kind(m, fleet).unwrap()), (Kind::Army, Kind::Fleet));
        assert_eq!(own_land(m, &ctx, 11).unwrap(), army);
        assert_eq!(refusal(own_land(m, &ctx, 12)), (Status::InvalidArgument, "not a land army".into()));
        assert_eq!(refusal(own(m, &ctx, 21)), (Status::NotFound, "no own army 21".into()));
        assert_eq!(refusal(own_land(m, &ctx, 99)), (Status::NotFound, "no own army 99".into()));
        assert!(is_own_army(m, ctx.player, army).unwrap());
        assert!(!is_own_army(m, ctx.player, fleet).unwrap() && !is_own_army(m, ctx.player, 0).unwrap());
        let roster: Vec<_> = roster(m, &ctx).unwrap().into_iter().map(|(id, at, land)| (id, at == army || at == fleet, land)).collect();
        assert_eq!(roster, [(11, true, true), (12, true, false)]);
    }

    #[test]
    fn an_army_is_seen_only_in_a_province_not_under_fog() {
        let clear = units(false);
        let Session { m, ctx, .. } = clear.session;
        assert_eq!(seen(m, &ctx, 21).unwrap(), by_id(m, 21).unwrap().unwrap());
        assert_eq!(refusal(seen(m, &ctx, 31)), (Status::NotFound, "the player sees no army 31".into()));
        assert_eq!(refusal(seen(m, &ctx, 12)), (Status::NotFound, "the player sees no army 12".into()));
        drop(clear);
        let fogged = units(true);
        let Session { m, ctx, .. } = fogged.session;
        assert_eq!(refusal(seen(m, &ctx, 21)), (Status::NotFound, "the player sees no army 21".into()));
    }

    #[test]
    fn a_province_s_land_armies_are_read_in_the_player_s_view_unless_fog_hides_them() {
        let units = units(true);
        let Session { m, ctx, .. } = units.session;
        let ids = |province: u32, view: View| -> Vec<u64> {
            in_province(m, &ctx, units._provinces.province(province), view).unwrap().into_iter().map(|(a, _)| a.id).collect()
        };
        // The fleet is not a land army.
        assert_eq!(ids(1, View::Player), [11]);
        assert_eq!(ids(2, View::Player), [] as [u64; 0]);
        assert_eq!(ids(2, View::Full), [21]);
    }

    #[test]
    fn a_fleet_s_record_counts_the_regiments_aboard_and_an_army_s_names_its_fleet() {
        let units = units(false);
        let Session { m, ctx, .. } = units.session;
        let specials = crate::game::special_countries(m).unwrap();
        let (army, fleet) = (own(m, &ctx, 11).unwrap(), own(m, &ctx, 12).unwrap());
        assert_eq!(aboard(m, fleet).unwrap(), [army]);
        let fleet = record(m, &specials, fleet).unwrap();
        assert_eq!((fleet.land, fleet.province, fleet.regiments, fleet.carrying), (false, 1, 1, 2));
        let army = record(m, &specials, army).unwrap();
        assert_eq!((army.land, army.province, army.regiments, army.aboard, army.busy), (true, 1, 2, Some(12), false));
    }
}
