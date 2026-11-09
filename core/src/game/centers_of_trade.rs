//! Centers of trade: what the province view's center of trade button offers,
//! and upgrading and downgrading one through the game's own command
//! (docs/research/envoys.md, "Centers of trade").
use super::{action, envoys, with_state, Ctx, GResult, Session, Tag};
use crate::{
    game_string,
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::centers_of_trade::args;
use eu4_harmony_remote::{
    centers_of_trade::CenterOfTradeOptions,
    choices::{Amount, Choice, Resource},
    ProvinceId,
};

/// The province's center of trade, or null; its level is at `+0x14` of the
/// object at its `+8`.
use crate::objects::campaign::province::{CENTER_OF_TRADE as CENTER, CENTER_OF_TRADE_DATA as LEVELS, CENTER_OF_TRADE_LEVEL as LEVEL};
/// The rules of both ways, the upgrade's and the downgrade's: each
/// `(center, tag, reasons or null) -> bool`, writing every line.
const MAY_CHANGE: GameFunction<3> = GameFunction::at(0x476ac0);
const MAY_UPGRADE: GameFunction<3> = GameFunction::at(0x476260);
const MAY_DOWNGRADE: GameFunction<3> = GameFunction::at(0x4760a0);
/// The upgrade's cost: `(center, out i32*, tag) -> out`, thousandths of
/// ducats.
const UPGRADE_COST: GameFunction<3> = GameFunction::at(0x476d60);
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
pub fn options(Session { m, ctx, .. }: Session, args::Options { province: ProvinceId(id) }: args::Options) -> GResult<CenterOfTradeOptions> {
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
    UPGRADE_COST.call(m, [center, &mut cost as *mut i32 as usize, ctx.player.0 as usize])?;
    let upgrade = choice(m, center, ctx.player, MAY_UPGRADE, vec![Amount::new(Resource::Ducats, super::thousandths(cost))])?;
    let prestige = m.i32(m.at(DOWNGRADE_PRESTIGE))?.unsigned_abs() as f64;
    let downgrade = choice(m, center, ctx.player, MAY_DOWNGRADE, vec![Amount::new(Resource::Prestige, prestige)])?;
    Ok((CenterOfTradeOptions { province: ProvinceId(id), level, upgrade, downgrade }, level))
}

/// Whether the player may change the center one way (`check`): the rules of
/// both ways, then that way's, each line the game writes for them.
fn choice(m: &Memory, center: usize, player: Tag, check: GameFunction<3>, cost: Vec<Amount>) -> GResult<Choice> {
    let (both, first) = game_string::written(m, |out| MAY_CHANGE.call(m, [center, player.0 as usize, out]))?;
    let (way, second) = game_string::written(m, |out| check.call(m, [center, player.0 as usize, out]))?;
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
pub fn change(Session { m, ctx, commands, .. }: Session, id: u32, up: bool) -> GResult<()> {
    let (options, level) = read(m, &ctx, province(m, &ctx, id)?, id)?;
    let choice = if up { &options.upgrade } else { &options.downgrade };
    if level == 0 {
        return Err(envoys::checks::no_center_of_trade());
    }
    let doing = if up { "upgrade the center of trade" } else { "downgrade the center of trade" };
    action::carry_out(commands, choice, doing, || Ok(written(ctx.player, id, if up { level + 1 } else { level - 1 })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_names_the_province_the_level_and_the_country() {
        let ven = Tag(u64::from_le_bytes(*b"VEN\0\0\0\0\0"));
        assert_eq!(written(ven, 112, 3), r#"modify_center_of_trade={ members={ dummy=112 dummy=3 dummy="VEN" } }"#);
    }

    use crate::catalogue::fake::tur;
    use crate::game::{
        buffers::{self, write_reason, Image, Object, Provinces},
        commands::record,
        Caller, Fail, Status,
    };
    use std::cell::Cell;

    thread_local! {
        /// Whether the rules of both ways hold, in this test's thread.
        static BOTH: Cell<bool> = const { Cell::new(true) };
    }

    /// The rules of both ways: they hold, or refuse as the game writes it.
    extern "C" fn may_change(_: usize, _: usize, out: usize) -> usize {
        if BOTH.with(Cell::get) {
            return 1;
        }
        unsafe { write_reason(out, b"- at war") };
        0
    }

    /// The upgrade's rules, which hold, and the downgrade's, which refuse.
    extern "C" fn may_downgrade(_: usize, _: usize, out: usize) -> usize {
        unsafe { write_reason(out, b"- too young") };
        0
    }

    extern "C" fn upgrade_cost(_: usize, out: *mut i32) -> usize {
        unsafe { *out = 250_000 };
        0
    }

    /// The Ottomans' province 1 with a center of trade of level 1, and their
    /// province 2 with none; a downgrade costs 20 prestige.
    struct Ottomans {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn ottomans(both: bool) -> Ottomans {
        BOTH.with(|b| b.set(both));
        let image = Image::new();
        image
            .strings()
            .function(MAY_CHANGE.rva(), may_change as *const ())
            .function(MAY_UPGRADE.rva(), buffers::yes as *const ())
            .function(MAY_DOWNGRADE.rva(), may_downgrade as *const ())
            .function(UPGRADE_COST.rva(), upgrade_cost as *const ())
            .i32(DOWNGRADE_PRESTIGE, -20);
        let provinces = Provinces::new(&image, 3);
        let levels = Object::new(LEVEL + 4).i32(LEVEL, 1);
        let center = Object::new(LEVELS + 8).points_to(LEVELS, levels);
        provinces.owner(1, tur()).u64(1, CENTER, center.at() as u64).owner(2, tur());
        let player = Object::country(tur(), 0);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), provinces.world(), tur(), player.at(), countries.at(), Caller::Remote);
        Ottomans { _image: image, _provinces: provinces, _objects: vec![center, player, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_button_offers_an_upgrade_for_ducats_and_a_downgrade_for_prestige() {
        let ottomans = ottomans(true);
        let center = options(ottomans.session, args::Options { province: ProvinceId(1) }).unwrap();
        assert_eq!(center.level, 1);
        assert_eq!((center.upgrade.can, center.upgrade.cost.clone()), (true, vec![Amount::new(Resource::Ducats, 250.0)]));
        assert_eq!((center.downgrade.can, center.downgrade.reasons.clone()), (false, vec!["too young".to_string()]));
        assert_eq!(center.downgrade.cost, [Amount::new(Resource::Prestige, 20.0)]);
        let none = options(ottomans.session, args::Options { province: ProvinceId(2) }).unwrap();
        assert_eq!((none.level, none.upgrade.can, none.downgrade.can), (0, false, false));
    }

    #[test]
    fn a_center_is_upgraded_by_the_dialog_s_command_to_the_next_level() {
        change(ottomans(true).session, 1, true).unwrap();
        assert_eq!(record::posted(), [r#"modify_center_of_trade={ members={ dummy=1 dummy=2 dummy="TUR" } }"#]);
    }

    #[test]
    fn no_center_and_the_game_s_rules_refuse_a_change() {
        assert_eq!(refused(change(ottomans(true).session, 2, true)), (Status::Rejected, "the province has no center of trade".into()));
        assert_eq!(refused(change(ottomans(true).session, 1, false)), (Status::Rejected, "too young".into()));
        assert_eq!(refused(change(ottomans(false).session, 1, true)), (Status::Rejected, "at war".into()));
        assert_eq!(refused(change(ottomans(true).session, 0, true)).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }
}
