//! Centers of trade: what the province view's center of trade button offers,
//! and upgrading and downgrading one through the game's own command
//! (docs/research/envoys.md, "Centers of trade").
use super::{envoys, with_state, Ctx, GResult, Session, Tag};
use crate::{game_string, memory::Memory, points::Point};
use eu4_harmony_remote::{
    centers_of_trade::CenterOfTradeOptions,
    choices::{Amount, Choice, Resource},
    ProvinceId,
};

/// The province's center of trade, or null; its level is at `+0x14` of the
/// object at its `+8`.
const CENTER: usize = 0x960;
const LEVELS: usize = 0x8;
const LEVEL: usize = 0x14;
/// The rules of both ways, the upgrade's and the downgrade's: each
/// `(center, tag, reasons or null) -> bool`, writing every line.
const MAY_CHANGE: usize = 0x476ac0;
const MAY_UPGRADE: usize = 0x476260;
const MAY_DOWNGRADE: usize = 0x4760a0;
/// The upgrade's cost: `(center, out i32*, tag) -> out`, thousandths of
/// ducats.
const UPGRADE_COST: usize = 0x476d60;
/// The prestige a downgrade costs, whole points, which the downgrade's
/// execute takes.
const DOWNGRADE_PRESTIGE: usize = 0x233e138;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x1392d4fusize, "4d 8b b7 60 09 00 00", "center of trade").uses(&[CENTER]),
        // The upgrade dialog posts the level after the center's.
        Point::keep(0x1392dc4usize, "44 8b 41 14 41 ff c0", "center of trade level").uses(&[LEVEL]),
        Point::keep(MAY_CHANGE, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "may change a center of trade"),
        Point::keep(MAY_UPGRADE, "48 89 5c 24 10 48 89 4c 24 08 55 56 57 41 54 41", "may upgrade a center of trade"),
        Point::keep(MAY_DOWNGRADE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 40 49", "may downgrade a center of trade"),
        Point::keep(UPGRADE_COST, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48", "center of trade upgrade cost"),
        Point::keep(0x4750e2usize, "69 05 4c 90 ec 01 e8 03 00 00", "center of trade downgrade prestige").uses(&[DOWNGRADE_PRESTIGE]),
    ]
}

fn province(m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    with_state(|s| s.geography.province_address(m, ctx, id))
}

/// The center of trade in province `id`, and what the province view offers
/// the player for it.
pub fn options(Session { m, ctx, .. }: Session, id: u32) -> GResult<CenterOfTradeOptions> {
    read(m, &ctx, province(m, &ctx, id)?, id).map(|(options, _)| options)
}

/// The options, and the level now.
fn read(m: &Memory, ctx: &Ctx, province: usize, id: u32) -> GResult<(CenterOfTradeOptions, u32)> {
    let center = m.ptr(province + CENTER)?;
    if center == 0 {
        let none = Choice::cannot(Vec::new(), Vec::new());
        return Ok((CenterOfTradeOptions { province: ProvinceId(id), level: 0, upgrade: none.clone(), downgrade: none }, 0));
    }
    let level = m.i32(m.ptr(center + LEVELS)? + LEVEL)?.max(0) as u32;
    let mut cost = 0i32;
    m.call(m.at(UPGRADE_COST), &[center, &mut cost as *mut i32 as usize, ctx.player.0 as usize])?;
    let upgrade = choice(m, center, ctx.player, MAY_UPGRADE, vec![Amount::new(Resource::Ducats, super::thousandths(cost))])?;
    let prestige = m.i32(m.at(DOWNGRADE_PRESTIGE))?.unsigned_abs() as f64;
    let downgrade = choice(m, center, ctx.player, MAY_DOWNGRADE, vec![Amount::new(Resource::Prestige, prestige)])?;
    Ok((CenterOfTradeOptions { province: ProvinceId(id), level, upgrade, downgrade }, level))
}

/// Whether the player may change the center one way (`check`): the rules of
/// both ways, then that way's, each line the game writes for them.
fn choice(m: &Memory, center: usize, player: Tag, check: usize, cost: Vec<Amount>) -> GResult<Choice> {
    let (both, first) = game_string::written(m, |out| m.call(m.at(MAY_CHANGE), &[center, player.0 as usize, out]))?;
    let (way, second) = game_string::written(m, |out| m.call(m.at(check), &[center, player.0 as usize, out]))?;
    if both as u8 != 0 && way as u8 != 0 {
        return Ok(Choice::can(cost));
    }
    // The functions write the rules that hold too; the reasons are those of
    // the one that refuses.
    let mut reasons = Vec::new();
    if both as u8 == 0 {
        reasons.extend(envoys::lines(&first));
    }
    if way as u8 == 0 {
        reasons.extend(envoys::lines(&second));
    }
    Ok(Choice::cannot(reasons, cost))
}

/// The command the confirm dialogs post: the province, the level it is to
/// have, and the country, each under the name `dummy`.
fn written(player: Tag, province: u32, level: u32) -> String {
    format!("modify_center_of_trade={{ members={{ dummy={province} dummy={level} dummy={} }} }}", envoys::quoted(player))
}

/// Upgrades (`up`) or downgrades the center of trade in the player's
/// province `id` a level.
pub fn change(Session { m, ctx, .. }: Session, id: u32, up: bool) -> GResult<()> {
    let (options, level) = read(m, &ctx, province(m, &ctx, id)?, id)?;
    let choice = if up { &options.upgrade } else { &options.downgrade };
    if level == 0 {
        return Err(eu4_harmony_checks::envoys::no_center_of_trade().into());
    }
    if !choice.can {
        let what = if up { "upgrade the center of trade" } else { "downgrade the center of trade" };
        return Err(eu4_harmony_checks::envoys::not_allowed(what, &choice.reasons).into());
    }
    envoys::post(&written(ctx.player, id, if up { level + 1 } else { level - 1 }), "the center of trade command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_names_the_province_the_level_and_the_country() {
        let ven = Tag(u64::from_le_bytes(*b"VEN\0\0\0\0\0"));
        assert_eq!(written(ven, 112, 3), r#"modify_center_of_trade={ members={ dummy=112 dummy=3 dummy="VEN" } }"#);
    }
}
