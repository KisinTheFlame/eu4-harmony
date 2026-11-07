//! The army and fleet panels' buttons: splitting, merging, disbanding,
//! consolidating, drilling, and a fleet's missions, through the game's own
//! commands as the panels post them, with the game's own answers of whether
//! the player can and why not (docs/research/units.md).
use super::{action, commands, world, Ctx, GResult, Session};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign::{classes, unit},
        list,
    },
    points::Point,
};
use eu4_harmony_remote::armies::args;
use eu4_harmony_remote::ArmyList;
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
    let passes = commands::passes(text).map_err(|refused| action::refused(refused, "give this fleet the mission"))?;
    Ok(if passes { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) })
}

fn entry(action: &str, choice: Choice) -> ArmyAction {
    ArmyAction { action: action.into(), choice }
}

/// What the panel's buttons offer for one of the player's armies or fleets.
pub fn options(Session { m, ctx, .. }: Session, args::Options { army: ArmyId(id) }: args::Options) -> GResult<Vec<ArmyAction>> {
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

pub fn split(Session { m, ctx, commands, .. }: Session, args::Split { army: ArmyId(id) }: args::Split) -> GResult<u64> {
    let unit = world::own_unit(m, &ctx, id)?;
    action::allowed(&may_split(m, unit)?, "split this unit")?;
    let before: Vec<u64> = world::own_roster(m, &ctx)?.into_iter().map(|(i, _, _)| i).collect();
    action::post(commands, &split_text(id, regiments(m, unit)?.div_ceil(2)), "split this unit")?;
    match world::own_roster(m, &ctx)?.into_iter().find(|(i, _, _)| !before.contains(i)) {
        Some((new, _, _)) => Ok(new),
        None => Err("the game split off no unit".to_string().into()),
    }
}

pub fn merge(Session { m, ctx, commands, .. }: Session, args::Merge { armies: ArmyList(ids) }: args::Merge) -> GResult<()> {
    let units = ids.iter().map(|id| world::own_unit(m, &ctx, id.0)).collect::<GResult<Vec<_>>>()?;
    action::allowed(&may_merge(m, &ctx, &units)?, "merge these units")?;
    action::post(commands, &merge_text(&ids.iter().map(|id| id.0).collect::<Vec<_>>()), "merge these units")?;
    Ok(())
}

pub fn disband(Session { m, ctx, commands, .. }: Session, args::Disband { army: ArmyId(id) }: args::Disband) -> GResult<()> {
    let unit = world::own_unit(m, &ctx, id)?;
    action::allowed(&may_disband(m, &ctx, unit)?, "disband this unit")?;
    action::post(commands, &disband_text(id), "disband this unit")?;
    Ok(())
}

/// One of the player's land armies, or the refusal.
fn own_army(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    world::own_land_army(m, ctx, id)
}

pub fn consolidate(Session { m, ctx, commands, .. }: Session, args::Consolidate { army: ArmyId(id) }: args::Consolidate) -> GResult<()> {
    let unit = own_army(m, &ctx, id)?;
    action::allowed(&may_consolidate(m, unit)?, "consolidate this army")?;
    action::post(commands, &consolidate_text(id, m.ptr(unit + COMPANY)? != 0), "consolidate this army")?;
    Ok(())
}

pub fn drill(Session { m, ctx, commands, .. }: Session, args::Drill { army: ArmyId(id) }: args::Drill) -> GResult<()> {
    let unit = own_army(m, &ctx, id)?;
    if mission_kind(m, unit)? == DRILLING {
        return Err(checks::state("the army drills already"));
    }
    action::allowed(&may_drill(m, unit)?, "drill this army")?;
    action::post(commands, &drill_text(id), "drill this army")
}

pub fn stop_drill(Session { m, ctx, commands, .. }: Session, args::StopDrill { army: ArmyId(id) }: args::StopDrill) -> GResult<()> {
    let unit = own_army(m, &ctx, id)?;
    if mission_kind(m, unit)? != DRILLING {
        return Err(checks::state("the army does not drill"));
    }
    action::post(commands, &drill_text(id), "stop drilling this army")
}

/// One of the player's fleets, or the refusal.
fn own_fleet(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    let unit = world::own_unit(m, ctx, id)?;
    match is_army(m, unit)? {
        true => Err(checks::not_fleet(id)),
        false => Ok(unit),
    }
}

pub fn mission(Session { m, ctx, commands, .. }: Session, args::Mission { army: ArmyId(id), mission, target }: args::Mission) -> GResult<()> {
    let fleet = own_fleet(m, &ctx, id)?;
    if !MISSIONS.contains(&mission.as_str()) {
        return Err(checks::unknown("mission", &mission, &MISSIONS));
    }
    let text = mission_text(&mission, id, world::location(m, fleet)?, target).map_err(|needs| checks::needs_target(&mission, needs))?;
    // The evade button stops evading instead, when the fleet evades.
    if mission == "evade" && mission_kind(m, fleet)? == EVADING {
        return Err(checks::state("the fleet evades already"));
    }
    action::post(commands, &text, &format!("give this fleet the mission {mission}"))?;
    Ok(())
}

pub fn stop_mission(Session { m, ctx, commands, .. }: Session, args::StopMission { army: ArmyId(id) }: args::StopMission) -> GResult<()> {
    own_fleet(m, &ctx, id)?;
    action::post(commands, &stop_mission_text(id), "end this fleet's mission")?;
    Ok(())
}

/// The refusals the framework words itself in the military panels'
/// actions, of armies and fleets, leaders, mercenaries, sieges and forts
/// (docs/player-actions.md, A4); those the game gives reasons for are
/// [`super::action`]'s.
pub mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of an action the unit's state makes pointless, in the
    /// words `why`, as the panel shows the other button instead.
    pub fn state(why: &str) -> Fail {
        Fail::new(Status::Rejected, why)
    }

    /// The refusal of a word that names none of `known`, for the argument
    /// `argument`.
    pub fn unknown(argument: &str, word: &str, known: &[&str]) -> Fail {
        Fail::new(Status::InvalidArgument, format!("{argument} {word:?} is none of {}", known.join(", ")))
    }

    /// The refusal of a leader the player does not have.
    pub fn no_leader(id: u32) -> Fail {
        Fail::new(Status::NotFound, format!("the player has no leader {id}"))
    }

    /// The refusal of a leader given to a unit of the other kind: a general or a
    /// conquistador leads an army, an admiral or an explorer a fleet.
    pub fn wrong_unit(kind: &str, land: bool) -> Fail {
        let (leads, not) = if land { ("a fleet", "an army") } else { ("an army", "a fleet") };
        Fail::new(Status::Rejected, format!("a leader of the kind {kind} leads {leads}, not {not}"))
    }

    /// The refusal of dismissing a leader that still leads an army or a fleet,
    /// which the game's command refuses: the leader is taken from it first.
    pub fn leads_a_unit(id: u32) -> Fail {
        Fail::new(Status::Rejected, format!("leader {id} leads an army or a fleet; take it from there first (leader.remove)"))
    }

    /// The refusal of a fleet's action for an army.
    pub fn not_fleet(id: u64) -> Fail {
        Fail::new(Status::InvalidArgument, format!("{id} is an army, not a fleet"))
    }

    /// The refusal of a mission given without the target it needs (`needs`,
    /// such as "a trade node's number").
    pub fn needs_target(mission: &str, needs: &str) -> Fail {
        Fail::new(Status::InvalidArgument, format!("the mission {mission} needs {needs} as its target"))
    }

    /// The refusal of a mercenary company the mercenary view does not list.
    pub fn no_company(key: &str) -> Fail {
        Fail::new(Status::NotFound, format!("the mercenary view lists no company {key:?}"))
    }

    /// The refusal of a province with no siege for the siege view's buttons.
    pub fn no_siege(province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("there is no siege in province {province}"))
    }

    /// The refusal of a fort action on a province with no fort of the player's.
    pub fn no_fort(province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("province {province} has no fort of the player's"))
    }

    /// The refusal of mothballing a fort already mothballed, or reopening one
    /// that is not.
    pub fn fort_already(province: u32, mothballed: bool) -> Fail {
        Fail::new(Status::Rejected, format!("the fort in province {province} is {} already", if mothballed { "mothballed" } else { "open" }))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_word_names_one_of_those_known_and_a_leader_its_kind_of_unit() {
            assert_eq!(unknown("kind", "captain", &["general", "admiral"]).status, Status::InvalidArgument);
            assert_eq!(wrong_unit("general", false).message, "a leader of the kind general leads an army, not a fleet");
        }
    }
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
