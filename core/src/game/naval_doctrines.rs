//! Naval doctrines: the doctrines of the game's database as the military
//! view's list offers them, and choosing one through the game's own command
//! (docs/research/power-spending.md, "The country's other spendings").
use super::content::{self, checks};
use super::{action, power, thousandths, Ctx, GResult, Session, Tag};
use crate::{
    containers,
    game_string,
    memory::Memory,
    objects::campaign::country,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::naval_doctrines::{args, NavalDoctrineOption};

/// The naval doctrines' database: its getter, which makes it on first use;
/// a game array of the doctrines.
const DOCTRINES: GameFunction<0> = GameFunction::at(0x918190);
/// `(doctrine, tag, reasons or null) -> bool`: whether the country of the
/// tag may select the doctrine, its `can_select`, writing the failing lines
/// into the `std::string` reasons when not null.
const CAN_SELECT: GameFunction<3> = GameFunction::at(0x9192c0);
/// `(country, out, doctrine) -> out`: what selecting the doctrine costs the
/// country, i32 thousandths of a ducat: its `cost` for each sailor of the
/// country's navy.
const COST: GameFunction<3> = GameFunction::at(0x409c70);
/// `(country, out, null) -> out`: the country's naval force limit, i32
/// thousandths.
const FORCE_LIMIT: GameFunction<3> = GameFunction::at(0x348000);
/// The least naval force limit the view opens the list at, a define, i32
/// (`NAVAL_DOCTRINE_MIN_FORCE_LIMIT`).
const LEAST_FORCE_LIMIT: usize = 0x233db0c;
/// A doctrine's content key, a `std::string`, which is also its name's
/// localisation key.
const KEY: usize = 0x278;
/// A doctrine's cost for each sailor, i32 thousandths of a ducat.
const SAILOR_COST: usize = 0x29c;
/// The country's doctrine, a pointer; null while it has none.
const DOCTRINE: usize = 0x9a60;
const MOST: usize = 1_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(DOCTRINES, "48 83 ec 28 48 8b 05 c5 3b b1 01", "naval doctrines database"),
        // The database's lookup by key compares each doctrine's key.
        Point::keep(0x918246usize, "48 8d 8e 78 02 00 00", "naval doctrine key").uses(&[KEY]),
        Point::keep(CAN_SELECT, "48 8b c4 48 89 58 08 48 89 70 18 55 57 41 56 48 8d 68 88 48 81 ec 60 01 00 00", "naval doctrine can_select"),
        Point::keep(COST, "48 89 5c 24 10 48 89 7c 24 18 4c 89 74 24 20 41 57 48 83 ec 20", "naval doctrine cost"),
        Point::keep(0x409d03usize, "49 63 8f 9c 02 00 00", "naval doctrine sailor cost").uses(&[SAILOR_COST]),
        // The command's check: the doctrine is not the country's own.
        Point::keep(0xc5f9dusize, "49 8b 89 60 9a 00 00", "country's naval doctrine").uses(&[DOCTRINE]),
        Point::keep(FORCE_LIMIT, "48 89 5c 24 08 57 48 83 ec 20 49 8b d8 48 8b fa 48 81 c1 c0 28 00 00", "naval force limit"),
        // The view opens the list at the least force limit, rounded.
        Point::keep(0xfb1c82usize, "e8 79 63 39 ff 48 8b c8 e8 d1 0b 76 00 3b 05 77 be 38 01", "naval doctrine least force limit").uses(&[LEAST_FORCE_LIMIT]),
        // The command's data, written as members: the key, then the tag.
        Point::keep(0xc5f63usize, "48 8b 5f 70", "naval doctrine command's country"),
        Point::keep(0xc15e9usize, "ba a9 2a 00 00", "naval doctrine command's members"),
    ]
}

/// The doctrines of the game's database, in its order.
fn doctrines(m: &Memory) -> GResult<Vec<usize>> {
    Ok(containers::array(m, DOCTRINES.call(m, [])?, MOST)?)
}

/// An i32 a function of the game's writes into the place it is given.
fn written(m: &Memory, function: GameFunction<3>, country_at: usize, it: usize) -> GResult<i32> {
    let mut out = 0i32;
    function.call(m, [country_at, &mut out as *mut i32 as usize, it])?;
    Ok(out)
}

/// The force limit as the view rounds it, to the nearest whole ship.
fn whole(thousandths: i32) -> i32 {
    if thousandths < 0 {
        (thousandths - 500) / 1000
    } else {
        (thousandths + 500) / 1000
    }
}

/// Whether the player can choose the doctrine `it` now, as the view's
/// list and the doctrine's tooltip say, and its cost.
fn choice(m: &Memory, ctx: &Ctx, it: usize) -> GResult<Choice> {
    let cost_of = written(m, COST, ctx.country, it)?;
    let cost = vec![Amount::new(Resource::Ducats, thousandths(cost_of))];
    let least = m.i32(m.at(LEAST_FORCE_LIMIT))?;
    if whole(written(m, FORCE_LIMIT, ctx.country, 0)?) < least {
        return Ok(Choice::cannot(power::tooltip(m, c"NAVAL_DOCTRINE_NOT_ENOUGH_FORCE_LIMIT", &[(c"VALUE", least.to_string())])?, cost));
    }
    if m.ptr(ctx.country + DOCTRINE)? == it {
        return Ok(Choice::cannot(power::tooltip(m, c"NAVAL_DOCTRINE_CURRENT", &[])?, cost));
    }
    let (can, mut reasons) = content::checked(m, |reasons| Ok(CAN_SELECT.call(m, [it, ctx.player.0 as usize, reasons])?))?;
    if !can {
        let mut lines = power::tooltip(m, c"NAVAL_DOCTRINE_NOT_AVAILABLE", &[])?;
        lines.append(&mut reasons);
        return Ok(Choice::cannot(lines, cost));
    }
    if m.i32(ctx.country + country::TREASURY)? < cost_of {
        return Ok(Choice::cannot(power::tooltip(m, c"NAVAL_DOCTRINE_CANNOT_AFFORD", &[])?, cost));
    }
    Ok(Choice::can(cost))
}

/// What the list offers for one doctrine.
fn offered(m: &Memory, ctx: &Ctx, it: usize) -> GResult<NavalDoctrineOption> {
    let key = game_string::key(m, it + KEY)?;
    let current = m.ptr(ctx.country + DOCTRINE)? == it;
    Ok(NavalDoctrineOption { name: content::name(m, &key)?, doctrine: key, current, choice: choice(m, ctx, it)? })
}

/// The doctrines, as the military view's list offers them.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Vec<NavalDoctrineOption>> {
    doctrines(m)?.into_iter().map(|it| offered(m, &ctx, it)).collect()
}

/// The command choosing a doctrine: the country and the doctrine's key, as
/// the members of a tuple, written last first.
fn command(player: Tag, key: &str) -> String {
    format!("naval_doctrine={{ members={{ dummy=\"{}\" dummy=\"{key}\" }} }}", content::letters(player))
}

/// Chooses a doctrine, as its button in the view's list does.
pub fn choose(Session { m, ctx, commands, .. }: Session, args::Choose { doctrine }: args::Choose) -> GResult<()> {
    for it in doctrines(m)? {
        if game_string::key(m, it + KEY)? == doctrine {
            let choice = choice(m, &ctx, it)?;
            return action::carry_out(commands, &choice, &format!("choose the naval doctrine {doctrine}"), || Ok(command(ctx.player, &doctrine)));
        }
    }
    Err(checks::not_found("naval doctrine", &doctrine))
}

crate::catalogue::bind! {
    eu4_harmony_remote::naval_doctrines: NavalDoctrineRequest => NavalDoctrineAnswer;
    Options => options;
    Choose => choose;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;

    #[test]
    fn the_command_names_the_country_then_the_doctrine() {
        assert_eq!(command(tur(), "merchant_navy"), r#"naval_doctrine={ members={ dummy="TUR" dummy="merchant_navy" } }"#);
    }

    #[test]
    fn the_force_limit_is_rounded_to_the_nearest_ship() {
        assert_eq!((whole(19_499), whole(19_500), whole(20_000), whole(-1_500)), (19, 20, 20, -2));
    }
}
