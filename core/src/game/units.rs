//! The army and fleet panels' buttons: splitting, merging, disbanding,
//! consolidating, drilling, and a fleet's missions, through the game's own
//! commands as the panels post them, with the game's own answers of whether
//! the player can and why not (docs/research/units.md).
use super::{commands, context, game, world, Caller, Ctx, GResult};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign::{classes, unit},
        list,
    },
    points::Point,
};
use eu4_harmony_checks::military as checks;
use eu4_harmony_remote::{armies::ArmyAction, choices::Choice, ArmyId};

/// `(country, units, text or null, 0)`: whether the units in a game list
/// (head, tail, count; nodes with the unit at `+0`) may merge, with the
/// tooltip's reasons.
const MAY_MERGE: usize = 0x342f60;
/// `(country, unit, text or null)`: whether the unit may be disbanded.
const MAY_DISBAND: usize = 0x3428c0;
/// `(army, text or null, u8*)`: whether the army may drill.
const MAY_DRILL: usize = 0xd40ce0;
/// `(unit, _)`: whether the unit may be split; no reasons.
const MAY_SPLIT: usize = 0xd63fc0;
/// A unit's method `(unit, leave empty ones, text or null)`: whether it may
/// consolidate its regiments.
const MAY_CONSOLIDATE: usize = 0x148;
/// A unit's mission, and the mission's method that answers its kind; a
/// unit's mercenary company.
const MISSION: usize = 0x1f8;
const MISSION_KIND: usize = 0x50;
const COMPANY: usize = 0x48;
/// The mission kinds of drilling and of evading.
const DRILLING: u32 = 15;
const EVADING: u32 = 6;
/// The reason the panel's buttons give a mission's command: the player's
/// action.
const PLAYER_ACTION: u32 = 54;
/// How many regiment kinds a consolidation says of each whether its emptied
/// regiments stay.
const CONSOLIDATE_KINDS: usize = 23;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_MERGE, "48 89 5c 24 18 44 88 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54", "may merge units"),
        Point::keep(MAY_DISBAND, "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8b ec 48 83 ec 70 4d 8b e0", "may disband a unit"),
        Point::keep(MAY_DRILL, "48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec e0 00 00 00", "may drill"),
        Point::keep(MAY_SPLIT, "48 89 5c 24 08 48 89 54 24 10 57 48 83 ec 20 48 8b d9 48 85 c9 0f", "may split a unit"),
        // The consolidate button calls the method, leaving emptied regiments
        // for a mercenary company.
        Point::keep(0x14825c3usize, "80 7b 10 00 48 8b 10 4c 8b 8a 48 01 00 00 75 1c 48 83 78 48 00", "consolidate button").uses(&[MAY_CONSOLIDATE, COMPANY]),
        Point::method(classes::ARMY, MAY_CONSOLIDATE, 0x1a5610, "army may consolidate"),
        // The drill button's state: the mission's kind.
        Point::keep(0x147f7b1usize, "48 8b 88 f8 01 00 00 48 85 c9 74 13 48 8b 01 ff 50 50 83 f8 0f", "drilling").uses(&[
            MISSION,
            MISSION_KIND,
            DRILLING as usize,
        ]),
        Point::keep(0x147fabeusize, "48 8b 4f 38 4c 8b c8 48 2b 4f 30", "split in half").uses(&[unit::REGIMENTS, unit::REGIMENTS + 8]),
    ]
}

/// An ID as the game writes it: its number and its type.
fn id_text(id: u64) -> String {
    format!("{{ id={} type={} }}", (id >> 32) as i32, id as u32 as i32)
}

fn split_text(id: u64, value: usize) -> String {
    format!("split_unit={{ unit={} value={value} }}", id_text(id))
}

fn merge_text(ids: &[u64]) -> String {
    let units: Vec<String> = ids.iter().map(|id| format!("unit={}", id_text(*id))).collect();
    format!("merge_units={{ {} }}", units.join(" "))
}

fn disband_text(id: u64) -> String {
    format!("disband_unit={{ unit={} }}", id_text(id))
}

/// The consolidate command; with `leave_empty`, as the button gives it for a
/// mercenary company, the emptied regiments stay.
fn consolidate_text(id: u64, leave_empty: bool) -> String {
    match leave_empty {
        true => format!("consolidate={{ unit={} empty={{ {} }} }}", id_text(id), vec!["yes"; CONSOLIDATE_KINDS].join(" ")),
        false => format!("consolidate={{ unit={} }}", id_text(id)),
    }
}

/// The drill command, which starts drilling and stops it.
fn drill_text(id: u64) -> String {
    format!("drill_army_mission={{ unit={} reason={PLAYER_ACTION} }}", id_text(id))
}

fn stop_mission_text(id: u64) -> String {
    format!("stop_unit_mission={{ unit={} reason={PLAYER_ACTION} }}", id_text(id))
}

/// A fleet's mission, its kind in `naval_region_mission` or its command.
const MISSIONS: [&str; 8] = ["protect_trade", "privateer", "hunt_pirates", "hunt_fleets", "blockade", "intercept", "patrol", "evade"];

/// The command of a fleet's mission, from the province it is in, toward
/// `target` where the mission has one.
fn mission_text(mission: &str, id: u64, location: u32, target: Option<u32>) -> Result<String, &'static str> {
    let unit = id_text(id);
    let node = |command: &str| match target {
        Some(index) => Ok(format!("{command}={{ unit={unit} index={index} start={location} replace=yes }}")),
        None => Err("a trade node's number"),
    };
    let region = |kind: u32| match target {
        Some(region) => Ok(format!("naval_region_mission={{ type={kind} unit={unit} region={region} }}")),
        None => Err("a sea region's number"),
    };
    match mission {
        "protect_trade" => node("trade_mission"),
        "privateer" => node("privateer_mission"),
        "hunt_pirates" => node("pirate_hunt"),
        "hunt_fleets" => region(7),
        "blockade" => region(8),
        "intercept" => region(9),
        "patrol" => Ok(format!("patrol={{ unit={unit} }}")),
        "evade" => Ok(format!("evade_mission={{ navy={unit} }}")),
        _ => Err("a mission"),
    }
}

/// A choice from one of the game's answers that writes its reasons.
fn answered(m: &Memory, ask: impl FnOnce(usize) -> crate::Result<usize>) -> GResult<Choice> {
    let (yes, text) = game_string::written(m, ask)?;
    Ok(if yes as u8 != 0 { Choice::can(Vec::new()) } else { Choice::cannot(super::reasons(&text), Vec::new()) })
}

fn is_army(m: &Memory, unit: usize) -> GResult<bool> {
    Ok(m.ptr(unit)? == m.at(classes::ARMY))
}

fn regiments(m: &Memory, unit: usize) -> GResult<usize> {
    super::span(m.ptr(unit + unit::REGIMENTS)?, m.ptr(unit + unit::REGIMENTS + 8)?, 8, super::MAX_UNITS)
}

fn mission_kind(m: &Memory, unit: usize) -> GResult<u32> {
    let mission = m.ptr(unit + MISSION)?;
    if mission == 0 {
        return Ok(0);
    }
    Ok(m.call(m.vslot(mission, MISSION_KIND)?, &[mission])? as u32)
}

/// The player's units of the same kind as `unit` in its province, it first:
/// what the player selects to merge.
fn mergeable(m: &Memory, ctx: &Ctx, unit: usize) -> GResult<Vec<(u64, usize)>> {
    let (land, at) = (is_army(m, unit)?, m.ptr(unit + unit::LOCATION)?);
    let mut out = vec![(m.u64(unit + unit::ID)?, unit)];
    for (id, other, other_land) in world::own_roster(m, ctx)? {
        if other != unit && other_land == land && m.ptr(other + unit::LOCATION)? == at {
            out.push((id, other));
        }
    }
    Ok(out)
}

/// Whether the units may merge, asked with a game list of them.
fn may_merge(m: &Memory, ctx: &Ctx, units: &[usize]) -> GResult<Choice> {
    let mut nodes = vec![[0u64; list::NODE_SIZE / 8]; units.len()];
    let addresses: Vec<usize> = nodes.iter_mut().map(|n| n.as_mut_ptr() as usize).collect();
    for (i, (&node, &u)) in addresses.iter().zip(units).enumerate() {
        m.write(node + list::ITEM, &(u as u64).to_le_bytes())?;
        m.write(node + list::PREVIOUS, &(if i > 0 { addresses[i - 1] } else { 0 } as u64).to_le_bytes())?;
        m.write(node + list::NEXT, &(addresses.get(i + 1).copied().unwrap_or(0) as u64).to_le_bytes())?;
    }
    let mut header = [0u64; list::SIZE / 8];
    let h = header.as_mut_ptr() as usize;
    m.write(h + list::HEAD, &(addresses.first().copied().unwrap_or(0) as u64).to_le_bytes())?;
    m.write(h + list::TAIL, &(addresses.last().copied().unwrap_or(0) as u64).to_le_bytes())?;
    m.write(h + list::COUNT, &(units.len() as i32).to_le_bytes())?;
    answered(m, |t| m.call(m.at(MAY_MERGE), &[ctx.country, h, t, 0]))
}

fn may_split(m: &Memory, unit: usize) -> GResult<Choice> {
    let can = m.call(m.at(MAY_SPLIT), &[unit, 0])? as u8 != 0 && regiments(m, unit)? > 1;
    Ok(if can { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) })
}

fn may_disband(m: &Memory, ctx: &Ctx, unit: usize) -> GResult<Choice> {
    answered(m, |t| m.call(m.at(MAY_DISBAND), &[ctx.country, unit, t]))
}

fn may_consolidate(m: &Memory, unit: usize) -> GResult<Choice> {
    let leave = m.ptr(unit + COMPANY)? != 0;
    answered(m, |t| m.call(m.vslot(unit, MAY_CONSOLIDATE)?, &[unit, leave as usize, t]))
}

fn may_drill(m: &Memory, unit: usize) -> GResult<Choice> {
    let mut out = 0u8;
    answered(m, |t| m.call(m.at(MAY_DRILL), &[unit, t, &mut out as *mut u8 as usize]))
}

/// Whether a command the panel posts would pass the game's check now, as a
/// choice without reasons.
fn passes(text: &str) -> GResult<Choice> {
    Ok(if commands::passes(text)? { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) })
}

fn entry(action: &str, choice: Choice) -> ArmyAction {
    ArmyAction { action: action.into(), choice }
}

/// What the panel's buttons offer for one of the player's armies or fleets.
pub fn options(_: Caller, id: u64) -> GResult<Vec<ArmyAction>> {
    let m = game()?;
    let ctx = context(m)?;
    let unit = world::own_unit(m, &ctx, id)?;
    let others: Vec<usize> = mergeable(m, &ctx, unit)?.into_iter().map(|(_, u)| u).collect();
    let mut out = vec![entry("split", may_split(m, unit)?), entry("merge", may_merge(m, &ctx, &others)?), entry("disband", may_disband(m, &ctx, unit)?)];
    if is_army(m, unit)? {
        out.push(entry("consolidate", may_consolidate(m, unit)?));
        let drilling = mission_kind(m, unit)? == DRILLING;
        out.push(entry(if drilling { "stop_drill" } else { "drill" }, if drilling { Choice::can(Vec::new()) } else { may_drill(m, unit)? }));
    } else {
        let location = world::location(m, unit)?;
        for mission in ["patrol", "evade"] {
            let text = mission_text(mission, id, location, None).map_err(|e| e.to_string())?;
            out.push(entry(mission, passes(&text)?));
        }
    }
    Ok(out)
}

/// Refuses with the reasons when `choice` says the player cannot `what`.
fn allowed(choice: Choice, what: &str) -> GResult<()> {
    match choice.can {
        true => Ok(()),
        false => Err(checks::not_allowed(what, &choice.reasons).into()),
    }
}

pub fn split(_: Caller, id: u64) -> GResult<u64> {
    let m = game()?;
    let ctx = context(m)?;
    let unit = world::own_unit(m, &ctx, id)?;
    allowed(may_split(m, unit)?, "split this unit")?;
    let before: Vec<u64> = world::own_roster(m, &ctx)?.into_iter().map(|(i, _, _)| i).collect();
    commands::post(&split_text(id, regiments(m, unit)?.div_ceil(2)))?;
    match world::own_roster(m, &ctx)?.into_iter().find(|(i, _, _)| !before.contains(i)) {
        Some((new, _, _)) => Ok(new),
        None => Err("the game split off no unit".to_string().into()),
    }
}

pub fn merge(_: Caller, ids: Vec<ArmyId>) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let units = ids.iter().map(|id| world::own_unit(m, &ctx, id.0)).collect::<GResult<Vec<_>>>()?;
    allowed(may_merge(m, &ctx, &units)?, "merge these units")?;
    commands::post(&merge_text(&ids.iter().map(|id| id.0).collect::<Vec<_>>()))?;
    Ok(())
}

pub fn disband(_: Caller, id: u64) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let unit = world::own_unit(m, &ctx, id)?;
    allowed(may_disband(m, &ctx, unit)?, "disband this unit")?;
    commands::post(&disband_text(id))?;
    Ok(())
}

/// One of the player's land armies, or the refusal.
fn own_army(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    world::own_land_army(m, ctx, id)
}

pub fn consolidate(_: Caller, id: u64) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let unit = own_army(m, &ctx, id)?;
    allowed(may_consolidate(m, unit)?, "consolidate this army")?;
    commands::post(&consolidate_text(id, m.ptr(unit + COMPANY)? != 0))?;
    Ok(())
}

pub fn drill(_: Caller, id: u64) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let unit = own_army(m, &ctx, id)?;
    if mission_kind(m, unit)? == DRILLING {
        return Err(checks::state("the army drills already").into());
    }
    allowed(may_drill(m, unit)?, "drill this army")?;
    commands::post(&drill_text(id))?;
    Ok(())
}

pub fn stop_drill(_: Caller, id: u64) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let unit = own_army(m, &ctx, id)?;
    if mission_kind(m, unit)? != DRILLING {
        return Err(checks::state("the army does not drill").into());
    }
    commands::post(&drill_text(id))?;
    Ok(())
}

/// One of the player's fleets, or the refusal.
fn own_fleet(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    let unit = world::own_unit(m, ctx, id)?;
    match is_army(m, unit)? {
        true => Err(checks::not_fleet(id).into()),
        false => Ok(unit),
    }
}

pub fn mission(_: Caller, id: u64, mission: String, target: Option<u32>) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let fleet = own_fleet(m, &ctx, id)?;
    if !MISSIONS.contains(&mission.as_str()) {
        return Err(checks::unknown("mission", &mission, &MISSIONS).into());
    }
    let text = mission_text(&mission, id, world::location(m, fleet)?, target).map_err(|needs| checks::needs_target(&mission, needs))?;
    // The evade button stops evading instead, when the fleet evades.
    if mission == "evade" && mission_kind(m, fleet)? == EVADING {
        return Err(checks::state("the fleet evades already").into());
    }
    commands::post(&text)?;
    Ok(())
}

pub fn stop_mission(_: Caller, id: u64) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    own_fleet(m, &ctx, id)?;
    commands::post(&stop_mission_text(id))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: u64 = 12 << 32 | 54;

    #[test]
    fn the_commands_as_the_panels_post_them() {
        assert_eq!(split_text(UNIT, 5), "split_unit={ unit={ id=12 type=54 } value=5 }");
        assert_eq!(merge_text(&[UNIT, 13 << 32 | 54]), "merge_units={ unit={ id=12 type=54 } unit={ id=13 type=54 } }");
        assert_eq!(disband_text(UNIT), "disband_unit={ unit={ id=12 type=54 } }");
        assert_eq!(consolidate_text(UNIT, false), "consolidate={ unit={ id=12 type=54 } }");
        assert!(consolidate_text(UNIT, true).ends_with("yes yes } }"));
        assert_eq!(drill_text(UNIT), "drill_army_mission={ unit={ id=12 type=54 } reason=54 }");
        assert_eq!(stop_mission_text(UNIT), "stop_unit_mission={ unit={ id=12 type=54 } reason=54 }");
    }

    #[test]
    fn missions_by_their_words() {
        assert_eq!(mission_text("blockade", UNIT, 1301, Some(47)).unwrap(), "naval_region_mission={ type=8 unit={ id=12 type=54 } region=47 }");
        assert_eq!(mission_text("privateer", UNIT, 1301, Some(12)).unwrap(), "privateer_mission={ unit={ id=12 type=54 } index=12 start=1301 replace=yes }");
        assert_eq!(mission_text("evade", UNIT, 1301, None).unwrap(), "evade_mission={ navy={ id=12 type=54 } }");
        assert!(mission_text("blockade", UNIT, 1301, None).is_err());
    }
}
