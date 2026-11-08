//! Trade companies: what the province view offers for a province's trade
//! company, adding the province to it and removing it, and the trade
//! company investments of its area, through the game's own commands
//! (docs/research/envoys.md, "Trade companies").
use super::{action, envoys, with_state, Ctx, GResult, Session, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::province as province_fields,
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::trade_companies::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    trade_companies::{Investment, TradeCompanyOptions},
    ProvinceId,
};

/// Whether the country may add the province to its trade company, and why
/// not: `(country, province, reasons) -> bool`, appending to `reasons`.
const MAY_ADD: GameFunction<3> = GameFunction::at(0x3ba790);
/// Whether the country may remove the province from its trade company:
/// `(country, province, text) -> bool`.
const MAY_REMOVE: GameFunction<3> = GameFunction::at(0x3ba830);
/// The province's flags, and the one of a province in a trade company.
const FLAGS: usize = 0x958;
const IN_TRADE_COMPANY: u8 = 0x04;
/// The investments' database, a pointer, and its investments in index
/// order: an array of pointers and their count, i32.
const DATABASE: usize = 0x242be40;
const INVESTMENTS: usize = 0x88;
const INVESTMENT_COUNT: usize = 0x94;
/// An investment's index and key.
const INDEX: usize = 0x10;
const KEY: usize = 0x20;
/// Whether the country may make the investment in the province's area, and
/// why not: `(tag, investment, province, made out, reasons) -> bool`,
/// appending to `reasons`.
const MAY_INVEST: GameFunction<5> = GameFunction::at(0xb40520);
/// What the investment costs the country: `(investment, out i32*, tag,
/// area or null, text or null) -> out`, thousandths of ducats.
const COST: GameFunction<5> = GameFunction::at(0xb3f580);
/// The investment's name for the country, as the panel writes it:
/// `(investment, out, tag)`, constructing `out`.
const NAME: GameFunction<3> = GameFunction::at(0xb3ddc0);
/// The province's area, in its map data.
const AREA: usize = 0x40;
/// The most investments read.
const MOST: usize = 1_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_ADD, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 40 49 8b d8 48 8b fa 48 8b f1", "may add to a trade company"),
        Point::keep(MAY_REMOVE, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 40 49 8b f8", "may remove from a trade company"),
        // The add check's rule that the province is not in one already.
        Point::keep(0x3ba482usize, "f6 87 58 09 00 00 04", "in a trade company").uses(&[FLAGS]),
        Point::keep(NAME, "48 89 5c 24 08 48 89 54 24 10 55 56 57 48 8d ac 24 80 fc ff ff", "investment's name"),
        // The database's maker keeps it in the global.
        Point::keep(0xb4286fusize, "48 8b 05 ca 95 8e 01", "investments database").uses(&[DATABASE]),
        // The possible investments' tooltip walks them.
        Point::keep(0xb40b2busize, "49 8b be 88 00 00 00 49 63 86 94 00 00 00", "investments").uses(&[INVESTMENTS, INVESTMENT_COUNT]),
        // The investment dialog takes its index for the command.
        Point::keep(0x1343cdeusize, "41 8b 7d 10", "investment index").uses(&[INDEX]),
        // Its name is localised from the key.
        Point::keep(0xb3df0ausize, "49 8d 51 20", "investment key").uses(&[KEY]),
        Point::keep(MAY_INVEST, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48 89 7c 24 20 41 56 48 83 ec 50", "may invest"),
        Point::keep(COST, "48 89 5c 24 08 48 89 54 24 10 55 56 57 41 54 41 55 41 56 41 57", "investment cost"),
        // The investment check finds the province's area.
        Point::keep(0xb40587usize, "48 8b 46 28 4c 8b 50 40", "province's area").uses(&[AREA]),
    ]
}

fn province(m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    with_state(|s| s.geography.province_address(m, ctx, id))
}

/// What the province view offers the player in province `id`.
pub fn options(Session { m, ctx, .. }: Session, args::Options { province: ProvinceId(id) }: args::Options) -> GResult<TradeCompanyOptions> {
    read(m, &ctx, province(m, &ctx, id)?, id)
}

fn read(m: &Memory, ctx: &Ctx, province: usize, id: u32) -> GResult<TradeCompanyOptions> {
    let in_trade_company = m.u8(province + FLAGS)? & IN_TRADE_COMPANY != 0;
    let add = if in_trade_company {
        Choice::cannot(Vec::new(), Vec::new())
    } else {
        // Allowed, it writes what adding does instead of reasons.
        let (may, text) = game_string::written(m, |out| MAY_ADD.call(m, [ctx.country, province, out]))?;
        if may as u8 != 0 {
            Choice::can(Vec::new())
        } else {
            Choice::cannot(envoys::lines(&text), Vec::new())
        }
    };
    let remove = if in_trade_company && MAY_REMOVE.call(m, [ctx.country, province, 0])? as u8 != 0 {
        Choice::can(Vec::new())
    } else {
        Choice::cannot(Vec::new(), Vec::new())
    };
    // The province view offers the investments in a province of a trade
    // company.
    let investments = if in_trade_company { investments(m, ctx, province)? } else { Vec::new() };
    Ok(TradeCompanyOptions { province: ProvinceId(id), in_trade_company, add, remove, investments })
}

/// The investments in the database's order, each its address and key.
fn database(m: &Memory) -> GResult<Vec<(usize, String)>> {
    let database = m.ptr(m.at(DATABASE))?;
    if database == 0 {
        return Ok(Vec::new());
    }
    let (begin, count) = (m.ptr(database + INVESTMENTS)?, m.i32(database + INVESTMENT_COUNT)?);
    if begin == 0 || !(0..=MOST as i32).contains(&count) {
        return Ok(Vec::new());
    }
    (0..count as usize)
        .map(|i| {
            let at = m.ptr(begin + i * 8)?;
            Ok((at, game_string::key(m, at + KEY)?))
        })
        .collect()
}

fn investments(m: &Memory, ctx: &Ctx, province: usize) -> GResult<Vec<Investment>> {
    let area = m.ptr(m.ptr(province + province_fields::GEOMETRY)? + AREA)?;
    database(m)?
        .into_iter()
        .map(|(at, key)| {
            let mut made = 0u8;
            let (may, text) = game_string::written(m, |out| MAY_INVEST.call(m, [ctx.player.0 as usize, at, province, &mut made as *mut u8 as usize, out]))?;
            let mut cost = 0i32;
            COST.call(m, [at, &mut cost as *mut i32 as usize, ctx.player.0 as usize, area, 0])?;
            let cost = vec![Amount::new(Resource::Ducats, super::thousandths(cost))];
            let choice = if may as u8 != 0 { Choice::can(cost) } else { Choice::cannot(envoys::failing(&text), cost) };
            let (_, name) = game_string::written(m, |out| NAME.call(m, [at, out, ctx.player.0 as usize]))?;
            let name = game_text::plain(&name);
            Ok(Investment { investment: key, name, made: made != 0, choice })
        })
        .collect()
}

/// The command the confirm dialog of the province view's trade company
/// button posts.
fn written(player: Tag, province: u32, add: bool) -> String {
    let name = if add { "add_to_trade_company" } else { "remove_from_trade_company" };
    format!("{name}={{ country={} province={province} }}", envoys::quoted(player))
}

/// Adds the player's province `id` to the trade company of its region
/// (`add`), or removes it from its trade company.
pub fn change(Session { m, ctx, commands, .. }: Session, id: u32, add: bool) -> GResult<()> {
    let options = read(m, &ctx, province(m, &ctx, id)?, id)?;
    let choice = if add { &options.add } else { &options.remove };
    let doing = if add { "add the province to a trade company" } else { "remove the province from its trade company" };
    action::carry_out(commands, choice, doing, || Ok(written(ctx.player, id, add)))
}

/// The investment command the investment dialog posts: the country, the
/// investment's index, the province, each under the name `dummy`.
fn invest_written(player: Tag, index: i32, province: u32) -> String {
    format!("make_trade_company_investment={{ members={{ dummy={} dummy={index} dummy={province} }} }}", envoys::quoted(player))
}

/// Makes the investment `key` in the area of the player's province `id`.
pub fn invest(Session { m, ctx, commands, .. }: Session, id: u32, key: &str) -> GResult<()> {
    let province = province(m, &ctx, id)?;
    let Some((at, _)) = database(m)?.into_iter().find(|(_, k)| k == key) else {
        return Err(envoys::checks::unknown("trade company investment", key));
    };
    let offered = read(m, &ctx, province, id)?.investments.into_iter().find(|i| i.investment == key);
    let Some(offered) = offered else {
        return Err(action::not_allowed("invest there", &[]));
    };
    if !offered.choice.can {
        return Err(action::not_allowed("make that investment there", &offered.choice.reasons));
    }
    action::post(commands, &invest_written(ctx.player, m.i32(at + INDEX)?, id), "make that investment there")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_name_the_country_and_the_province() {
        let por = Tag(u64::from_le_bytes(*b"POR\0\0\0\0\0"));
        assert_eq!(written(por, 1234, true), r#"add_to_trade_company={ country="POR" province=1234 }"#);
        assert_eq!(written(por, 1234, false), r#"remove_from_trade_company={ country="POR" province=1234 }"#);
        assert_eq!(invest_written(por, 3, 1234), r#"make_trade_company_investment={ members={ dummy="POR" dummy=3 dummy=1234 } }"#);
    }

    mod through_the_layer {
        use super::super::*;
        use crate::catalogue::fake::tur;
        use crate::game::{
            buffers::{self, Image, Object, Provinces},
            commands::record,
            Caller, Fail, Status,
        };
        use std::cell::Cell;

        thread_local! {
            /// Whether the game's checks let the player add, remove and
            /// invest.
            static MAY: Cell<bool> = const { Cell::new(true) };
        }

        fn may(text: usize, reason: &[u8]) -> usize {
            let may = MAY.get();
            if !may && text != 0 {
                unsafe { buffers::write_string(text, reason) };
            }
            may as usize
        }

        extern "C" fn may_add(_: usize, _: usize, text: usize) -> usize {
            may(text, b"- Not a coast")
        }

        extern "C" fn may_remove(_: usize, _: usize, _: usize) -> usize {
            may(0, b"")
        }

        /// The game's check of an investment, its failing condition marked
        /// as the game marks it.
        extern "C" fn may_invest(_: usize, _: usize, _: usize, _: usize, text: usize) -> usize {
            may(text, b"\xa3no Too poor")
        }

        extern "C" fn cost(_: usize, out: *mut i32, _: usize, _: usize, _: usize) -> usize {
            unsafe { *out = 300_000 };
            out as usize
        }

        extern "C" fn name(_: usize, out: usize, _: usize) -> usize {
            unsafe { buffers::write_string(out, "Local Quarter") };
            out
        }

        /// Turkey's province 1, in a trade company when `in_company`, among
        /// provinces 0 and 1, with the investment `local_quarter`, of index
        /// 3. Runs `action` over Turkey's session.
        fn turkey(in_company: bool, action: impl FnOnce(Session) -> GResult<()>) -> GResult<()> {
            let image = Image::new();
            buffers::strings_kept(&image).function(MAY_ADD.rva(), may_add as *const ()).function(MAY_REMOVE.rva(), may_remove as *const ());
            image.function(MAY_INVEST.rva(), may_invest as *const ()).function(COST.rva(), cost as *const ()).function(NAME.rva(), name as *const ());
            let provinces = Provinces::new(&image, 2);
            provinces.set(1, FLAGS, &[if in_company { IN_TRADE_COMPANY } else { 0 }]);
            let quarter = Object::new(KEY + 0x20).i32(INDEX, 3).string(KEY, "local_quarter");
            let database = Object::new(INVESTMENT_COUNT + 4).vector(INVESTMENTS, &[quarter.at() as u64]).i32(INVESTMENT_COUNT, 1);
            image.u64(DATABASE, database.at() as u64);
            let country = Object::country(tur(), 0);
            let countries = Object::countries(&[&country]);
            let result = action(buffers::session(image.memory(), provinces.world.at(), tur(), country.at(), countries.at(), Caller::Remote));
            drop((quarter, database, provinces, country, countries, image));
            result
        }

        fn refused(result: GResult<()>) -> (Status, String) {
            let Err(Fail { status, message }) = result else { panic!("the action was carried out") };
            (status, message)
        }

        #[test]
        fn a_province_is_added_and_removed_as_the_game_lets_the_player() {
            turkey(false, |s| change(s, 1, true)).unwrap();
            turkey(true, |s| change(s, 1, false)).unwrap();
            assert_eq!(record::posted(), [written(tur(), 1, true), written(tur(), 1, false)]);
            let no_reason = |doing: &str| format!("the game does not let the player {doing} now, and gives no reason");
            assert_eq!(refused(turkey(true, |s| change(s, 1, true))), (Status::Rejected, no_reason("add the province to a trade company")));
            assert_eq!(refused(turkey(false, |s| change(s, 1, false))), (Status::Rejected, no_reason("remove the province from its trade company")));
            MAY.set(false);
            assert_eq!(refused(turkey(false, |s| change(s, 1, true))), (Status::Rejected, "Not a coast".into()));
            assert_eq!(refused(turkey(true, |s| change(s, 1, false))), (Status::Rejected, no_reason("remove the province from its trade company")));
            assert_eq!(refused(turkey(false, |s| change(s, 0, true))).0, Status::NotFound);
            assert!(record::posted().is_empty());
        }

        #[test]
        fn an_investment_is_one_of_the_game_s_made_in_a_province_of_a_trade_company() {
            turkey(true, |s| invest(s, 1, "local_quarter")).unwrap();
            assert_eq!(record::posted(), [invest_written(tur(), 3, 1)]);
            assert_eq!(refused(turkey(true, |s| invest(s, 1, "harbor"))), (Status::NotFound, "no trade company investment is named harbor".into()));
            assert_eq!(
                refused(turkey(false, |s| invest(s, 1, "local_quarter"))),
                (Status::Rejected, "the game does not let the player invest there now, and gives no reason".into())
            );
            MAY.set(false);
            assert_eq!(refused(turkey(true, |s| invest(s, 1, "local_quarter"))), (Status::Rejected, "Too poor".into()));
            assert!(record::posted().is_empty());
        }

        #[test]
        fn the_options_show_the_investments_only_in_a_trade_company() {
            let read = |in_company| {
                let mut read = None;
                turkey(in_company, |s| {
                    read = Some(options(s, args::Options { province: ProvinceId(1) })?);
                    Ok(())
                })
                .unwrap();
                read.unwrap()
            };
            let outside = read(false);
            assert!(!outside.in_trade_company && outside.add.can && !outside.remove.can && outside.investments.is_empty());
            let inside = read(true);
            assert!(inside.in_trade_company && !inside.add.can && inside.remove.can);
            let [quarter] = inside.investments.as_slice() else { panic!("{:?}", inside.investments) };
            assert_eq!((quarter.investment.as_str(), quarter.name.as_str(), quarter.choice.can), ("local_quarter", "Local Quarter", true));
            assert_eq!(quarter.choice.cost, vec![Amount::new(Resource::Ducats, 300.0)]);
        }
    }
}
