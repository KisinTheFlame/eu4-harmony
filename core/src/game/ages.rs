//! The ages: the age the world is in, the player's splendor, and the age's
//! abilities, taking one through the game's own command
//! (docs/research/institutions-ages-monuments.md, "Ages").
use super::commands::{write, Block};
use super::content::{self, checks};
use super::{action, thousandths, Ctx, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::objects::gui::functions::ADD_MONTHS;
use crate::{
    memory::Memory,
    objects::campaign::{age_ability, country, world},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::ages::{args, Age, AgeAbilityOption, GoldenEra};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};

/// The ages' database: its getter, which makes it on first use.
const AGES: GameFunction<0> = GameFunction::at(0x8db70);
/// `(country, ability, reasons) -> bool`: whether the country may take the
/// ability, writing why not into the `std::string` reasons, as the
/// command's check asks it; no for one it has taken, without a reason.
const CAN_TAKE: GameFunction<3> = GameFunction::at(0x3f8d30);
/// What an ability costs in splendor, a define, i32 thousandths.
const ABILITY_COST: usize = 0x233e4e4;
const MOST: usize = 1_000;
/// `(country, text) -> bool`: whether the country may enact its golden era
/// now, writing the ages view's button's tooltip into the `std::string`
/// text: why not, or what it brings.
const MAY_ENACT_GOLDEN_ERA: GameFunction<2> = GameFunction::at(0x3f9f00);
/// How many years a golden era lasts, a define, i32.
const GOLDEN_ERA_YEARS: usize = 0x233e2d0;
/// The hours of a year, as the game reckons a golden era's end.
const YEAR: i32 = 8760;
/// A country's golden era started after this date once it has had one.
const NO_GOLDEN_ERA: i32 = 0x29c77f8;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(AGES, "48 89 5c 24 10 57 48 83 ec 20 48 8b 05 5f de 39 02", "ages database"),
        Point::keep(CAN_TAKE, "48 89 5c 24 08 48 89 74 24 10 55 57", "age ability check"),
        // The check: the abilities taken, then the splendor against the cost.
        Point::keep(0x3f8da7usize, "49 8b 4e 38 49 8b 46 30", "age abilities taken").uses(&[country::AGE_ABILITIES]),
        Point::keep(0x3f8fe5usize, "8b 05 f9 54 f4 01 41 39 46 48", "age ability cost").uses(&[ABILITY_COST, country::SPLENDOR]),
        // The world's save writer writes the age's key.
        Point::keep(0x750a43usize, "49 8b 9e 20 24 00 00 48 83 c3 10", "the age").uses(&[world::AGE, age_ability::AGE_KEY]),
        // The age's reader makes each ability: its index and its age.
        Point::keep(0x8c65ausize, "89 4f 10 4c 89 6f 18", "age ability index and age").uses(&[age_ability::INDEX, age_ability::AGE]),
        // The country's save writer writes each ability taken by its key.
        Point::keep(0x2dca08usize, "48 8d 8f 98 00 00 00", "age ability key").uses(&[age_ability::KEY]),
        // The command's reader takes the ability by its index.
        Point::keep(0x2bdb2cusize, "48 8b 43 60 48 8b 5c 24 30 48 8b 0c c8", "age abilities by index").uses(&[age_ability::DATABASE_LIST]),
        Point::keep(MAY_ENACT_GOLDEN_ERA, "48 89 5c 24 10 55 56 57 41 56 41 57 48 8d ac 24 70 ff ff ff 48 81 ec 90 01 00 00", "may enact golden era"),
        // The tooltip: when the golden era started, the years it lasts and
        // the months it is lengthened by.
        Point::keep(0x3f9f2busize, "8b 89 88 27 00 00 81 f9 f8 77 9c 02", "golden era start").uses(&[country::GOLDEN_ERA_START]),
        Point::keep(0x3f9f3dusize, "69 05 89 43 f4 01 38 22 00 00", "golden era years").uses(&[GOLDEN_ERA_YEARS]),
        Point::keep(0x3f9f4fusize, "8b 96 8c 27 00 00", "golden era months").uses(&[country::GOLDEN_ERA_MONTHS]),
    ]
}

/// The age the world is in.
fn age(m: &Memory, world_at: usize) -> GResult<usize> {
    match m.ptr(world_at + world::AGE)? {
        0 => Err(checks::not_now("the world is in no age yet")),
        age => Ok(age),
    }
}

/// The player's golden era: the day it ends while the player is in one, and
/// whether the player can enact it now, as the button's tooltip says.
fn golden_era(m: &Memory, ctx: &Ctx) -> GResult<GoldenEra> {
    let start = m.i32(ctx.country + country::GOLDEN_ERA_START)?;
    let mut until = None;
    if start > NO_GOLDEN_ERA {
        let mut end = start.saturating_add(m.i32(m.at(GOLDEN_ERA_YEARS))?.saturating_mul(YEAR));
        let months = m.i32(ctx.country + country::GOLDEN_ERA_MONTHS)?;
        ADD_MONTHS.call(m, [&mut end as *mut i32 as usize, months as i64 as usize])?;
        if end > ctx.date {
            until = Some(eu4_harmony_remote::Date(end));
        }
    }
    let (may, why) = game_string::written(m, |text| MAY_ENACT_GOLDEN_ERA.call(m, [ctx.country, text]))?;
    let choice = if may & 0xff != 0 { Choice::can(Vec::new()) } else { Choice::cannot(super::power::plain_lines(&why), Vec::new()) };
    Ok(GoldenEra { until, choice })
}

/// The age the world is in, the player's splendor and its golden era.
pub fn get(Session { m, ctx, .. }: Session) -> GResult<Age> {
    let key = game_string::key(m, age(m, ctx.world)? + age_ability::AGE_KEY)?;
    let golden_era = golden_era(m, &ctx)?;
    Ok(Age { name: content::name(m, &key)?, age: key, splendor: thousandths(m.i32(ctx.country + country::SPLENDOR)?), golden_era })
}

/// The command enacting the golden era: the country alone.
fn golden_era_command(player: Tag) -> String {
    write("enact_golden_age", Block::new().tag("country", player.letters()))
}

/// Enacts the player's golden era, as the view's button does.
pub fn enact_golden_era(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    let choice = golden_era(m, &ctx)?.choice;
    action::carry_out(commands, &choice, "enact the golden era", || Ok(golden_era_command(ctx.player)))
}

/// The abilities of the age the world is in, in the database's order.
fn abilities(m: &Memory, world_at: usize) -> GResult<Vec<usize>> {
    let now = age(m, world_at)?;
    let all = containers::pointers(m, AGES.call(m, [])? + age_ability::DATABASE_LIST, MOST)?;
    let mut out = Vec::new();
    for at in all {
        if m.ptr(at + age_ability::AGE)? == now {
            out.push(at);
        }
    }
    Ok(out)
}

fn taken(m: &Memory, country_at: usize) -> GResult<Vec<usize>> {
    Ok(containers::pointers(m, country_at + country::AGE_ABILITIES, MOST)?)
}

/// What the view offers for one ability: nothing once taken.
fn offered(m: &Memory, country_at: usize, taken: &[usize], at: usize) -> GResult<AgeAbilityOption> {
    let key = game_string::key(m, at + age_ability::KEY)?;
    let had = taken.contains(&at);
    let cost = vec![Amount::new(Resource::Splendor, thousandths(m.i32(m.at(ABILITY_COST))?))];
    let choice = if had {
        Choice::cannot(Vec::new(), Vec::new())
    } else {
        let (can, reasons) = content::checked(m, |reasons| Ok(CAN_TAKE.call(m, [country_at, at, reasons])?))?;
        if can {
            Choice::can(cost)
        } else {
            Choice::cannot(reasons, cost)
        }
    };
    Ok(AgeAbilityOption { name: content::name(m, &key)?, ability: key, taken: had, choice })
}

/// The age's abilities, as the ages view offers them.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<AgeAbilityOption>> {
    let had = taken(m, ctx.country)?;
    abilities(m, ctx.world)?.into_iter().map(|at| offered(m, ctx.country, &had, at)).collect()
}

/// The command taking an ability, by its index, as the view's button sends
/// it.
fn command(player: Tag, index: i32) -> String {
    write("age_ability_command", Block::new().tag("country", player.letters()).word("which", index))
}

/// Takes an ability of the age, as the view's button does.
pub fn take_ability(Session { m, ctx, commands, .. }: Session, args::TakeAbility { ability: wanted }: args::TakeAbility) -> GResult<()> {
    let had = taken(m, ctx.country)?;
    let all = containers::pointers(m, AGES.call(m, [])? + age_ability::DATABASE_LIST, MOST)?;
    let Some(&at) = all.iter().find(|&&at| game_string::key(m, at + age_ability::KEY).is_ok_and(|k| k == wanted)) else {
        return Err(checks::not_found("age ability", &wanted));
    };
    if had.contains(&at) {
        return Err(checks::not_now(format!("the player has taken {wanted} already")));
    }
    if !abilities(m, ctx.world)?.contains(&at) {
        return Err(checks::not_now(format!("{wanted} is not an ability of the age the world is in")));
    }
    let choice = offered(m, ctx.country, &had, at)?.choice;
    let index = m.i32(at + age_ability::INDEX)?;
    action::carry_out(commands, &choice, &format!("take {wanted}"), || Ok(command(ctx.player, index)))
}

crate::catalogue::bind! {
    eu4_harmony_remote::ages: AgeRequest => AgeAnswer;
    Get => get;
    Abilities => list;
    TakeAbility => take_ability;
    EnactGoldenEra => enact_golden_era;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;

    #[test]
    fn the_ability_command_names_the_country_and_the_ability_by_index() {
        assert_eq!(command(tur(), 7), r#"age_ability_command={ country="TUR" which=7 }"#);
        assert_eq!(golden_era_command(tur()), r#"enact_golden_age={ country="TUR" }"#);
    }
}
