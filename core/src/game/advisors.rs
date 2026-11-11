//! Advisors: the player's advisors and the pool the court view offers to
//! hire from, and hiring, firing and promoting them through the game's own
//! commands, as the court view posts them (docs/research/advisors.md).
use super::commands::{write, Block};
use super::army::FIND_ID;
use super::{action, commands, content, thousandths, Ctx, GResult, Session, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{country as country_fields, functions, province as culture_fields},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::advisors::args;
use eu4_harmony_remote::{
    advisors::{Advisor, AdvisorOption, HiredAdvisor},
    choices::{Amount, Choice, PowerKind, Resource},
};

/// The country's advisors, one pointer for each kind of monarch power, by
/// its index; a seat with none holds the game's empty advisor, whose
/// [`REAL`] method answers no.
const SEATS: usize = 0x14e0;
/// The world's advisor pools, an array of pointers by the country's tag
/// index; a pool's list of the advisors offered, from [`POOL_FIRST`], is of
/// nodes holding the advisor (filled in as found) and its ID at
/// [`NODE_ID`], the next node at [`NODE_NEXT`].
const POOLS: usize = 0x1c30;
const POOL_FIRST: usize = 0x10;
const NODE_ID: usize = 0x8;
const NODE_NEXT: usize = 0x18;
/// An advisor's fields: its ID (type at `+0x10`, number at `+0x14`), name,
/// skill, type, discount (what its salary is worked out from), culture and
/// religion; the method that says whether it is an advisor at all.
mod advisor {
    pub const ID: usize = 0x10;
    pub const NUMBER: usize = 0x14;
    pub const NAME: usize = 0x20;
    pub const SKILL: usize = 0x40;
    pub const TYPE: usize = 0x2a0;
    pub const DISCOUNT: usize = 0x2c0;
    pub const CULTURE: usize = 0x2c8;
    pub const RELIGION: usize = 0x2d0;
    pub const REAL: usize = 0x50;
    /// The type every advisor's ID has.
    pub const ID_TYPE: u32 = 51;
}
/// An advisor type's kind of monarch power, by its index, and its key.
mod kind {
    pub const POWER: usize = 0xfc;
    pub const KEY: usize = 0x120;
}
/// `(advisor, out, tag by value) -> out`: what hiring the advisor costs, in
/// thousandths of a ducat, as the court view's hire button checks it and
/// the hire command pays it.
const HIRE_COST: GameFunction<3> = GameFunction::at(0x1ac4c0);
/// `(out, tag by value, skill, power, culture, religion, &discount, text or
/// null)`: the advisor's monthly salary in thousandths of a ducat.
const SALARY: GameFunction<8> = GameFunction::at(0x1ac650);
/// `(advisor, out, tag by value) -> out`: what promoting the advisor costs.
const PROMOTE_COST: GameFunction<3> = GameFunction::at(0x1ad2d0);
/// `(advisor, tag by value, reasons or null) -> bool`: whether the country
/// may promote the advisor now, writing the court view's reasons why not.
const MAY_PROMOTE: GameFunction<3> = GameFunction::at(0x1ad440);
/// `(advisor, reasons or null) -> bool`: whether the advisor may be fired
/// now, writing the court view's reasons why not.
const MAY_FIRE: GameFunction<2> = GameFunction::at(0x1aebb0);
/// The kinds of monarch power, by the game's index.
const POWERS: [PowerKind; 3] = [PowerKind::Adm, PowerKind::Dip, PowerKind::Mil];
/// The most advisors a pool's list is read for.
const MOST: usize = 1_000;

pub fn points() -> Vec<Point> {
    vec![
        // The AI's spending reads the country's seats and each advisor's
        // power.
        Point::keep(0x11ad2eusize, "48 8d ba e0 14 00 00", "country advisors").uses(&[SEATS]),
        Point::keep(0x11ad56usize, "48 8b 86 a0 02 00 00 48 63 90 fc 00 00 00", "advisor type and power").uses(&[advisor::TYPE, kind::POWER]),
        // The pool trigger finds the country's pool; the pool's lookup walks it.
        Point::keep(0xbec432usize, "48 8b 82 30 1c 00 00", "advisor pools").uses(&[POOLS]),
        Point::keep(0x1b58fausize, "48 8b 59 10", "advisor pool list").uses(&[POOL_FIRST]),
        Point::keep(0x1b5913usize, "48 8b 5b 18 49 8d 4b 08", "advisor pool node").uses(&[NODE_NEXT, NODE_ID]),
        // The hire cost reads the salary's arguments.
        Point::keep(0x1ac4e1usize, "44 8b 81 c0 02 00 00", "advisor discount").uses(&[advisor::DISCOUNT]),
        Point::keep(0x1ac50fusize, "4c 8b 83 c8 02 00 00", "advisor culture").uses(&[advisor::CULTURE]),
        Point::keep(0x1ac4f3usize, "4c 8b 83 d0 02 00 00", "advisor religion").uses(&[advisor::RELIGION]),
        Point::keep(0x1ac51ausize, "44 8b 43 40", "advisor skill").uses(&[advisor::SKILL]),
        // The advisor type's constructor copies its key.
        Point::keep(0x1af93fusize, "48 8d 8b 20 01 00 00", "advisor type key").uses(&[kind::KEY]),
        Point::keep(HIRE_COST, "4c 8b dc 49 89 5b 10 57 48 83 ec 40 4c 8b 89 a0 02 00 00", "advisor hire cost"),
        Point::keep(SALARY, "48 89 4c 24 08 55 53 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 08 ff ff", "advisor salary"),
        Point::keep(PROMOTE_COST, "4c 8b dc 53 48 83 ec 40 4c 8b 89 a0 02 00 00 49 8b c0 44 8b 81 c0 02 00", "advisor promotion cost"),
        Point::keep(MAY_PROMOTE, "48 89 5c 24 08 48 89 74 24 18 48 89 54 24 10 55 57 41 56 48 8b ec 48 81", "may promote an advisor"),
        Point::keep(MAY_FIRE, "48 89 5c 24 10 48 89 7c 24 18 55 48 8d 6c 24 a9 48 81 ec a0 00 00 00", "may fire an advisor"),
    ]
}

/// The refusals the framework words itself for advisors.
pub mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of an advisor the player does not have, or that the court
    /// view does not offer.
    pub fn no_advisor(id: u32, whose: &str) -> Fail {
        Fail::new(Status::NotFound, format!("no advisor {id} {whose}"))
    }
}

/// Whether the object at `advisor` is an advisor, not the game's empty one.
fn real(m: &Memory, advisor: usize) -> GResult<bool> {
    Ok(advisor != 0 && m.vslot(advisor, advisor::REAL)? == m.at(functions::YES))
}

/// The key of a culture's or a religion's token, none for none.
fn token_key(m: &Memory, object: usize, token: usize) -> GResult<Option<String>> {
    if object == 0 || m.call(m.vslot(object, culture_fields::VALID)?, &[object])? as u8 == 0 {
        return Ok(None);
    }
    let token = m.u32(object + token)?;
    Ok(Some(commands::type_name(m, token).ok_or_else(|| format!("token {token:#x} has no name"))?))
}

/// The advisor's kind of monarch power.
fn power(m: &Memory, advisor: usize) -> GResult<PowerKind> {
    let index = m.i32(m.ptr(advisor + advisor::TYPE)? + kind::POWER)?;
    POWERS.get(index as usize).copied().ok_or_else(|| format!("an advisor of the power {index}").into())
}

/// The game's index of a kind of monarch power.
fn seat(power: PowerKind) -> usize {
    POWERS.iter().position(|p| *p == power).unwrap_or(0)
}

/// A cost the game writes into an i32 of the framework's: `(advisor, out,
/// tag)`.
fn cost(m: &Memory, function: GameFunction<3>, advisor: usize, tag: Tag) -> GResult<i32> {
    let mut out = 0i32;
    function.call(m, [advisor, &mut out as *mut i32 as usize, tag.0 as usize])?;
    Ok(out)
}

/// The advisor as the court view shows it.
fn read(m: &Memory, ctx: &Ctx, advisor: usize) -> GResult<Advisor> {
    let skill = m.i32(advisor + advisor::SKILL)?;
    let power = power(m, advisor)?;
    let (culture, religion) = (m.ptr(advisor + advisor::CULTURE)?, m.ptr(advisor + advisor::RELIGION)?);
    let mut discount = m.i32(advisor + advisor::DISCOUNT)?;
    let mut salary = 0i32;
    SALARY.call(m, [&mut salary as *mut i32 as usize, ctx.player.0 as usize, skill as usize, seat(power), culture, religion, &mut discount as *mut i32 as usize, 0])?;
    Ok(Advisor {
        id: m.u32(advisor + advisor::NUMBER)?,
        name: game_string::shown(m, advisor + advisor::NAME)?,
        kind: game_string::key(m, m.ptr(advisor + advisor::TYPE)? + kind::KEY)?,
        power,
        skill: skill.max(0) as u32,
        culture: token_key(m, culture, culture_fields::CULTURE_TOKEN)?,
        religion: token_key(m, religion, culture_fields::RELIGION_TOKEN)?,
        salary: thousandths(salary),
    })
}

/// The player's advisors, in the order of their seats.
fn seated(m: &Memory, ctx: &Ctx) -> GResult<Vec<usize>> {
    let mut found = Vec::new();
    for power in POWERS {
        let advisor = m.ptr(ctx.country + SEATS + seat(power) * 8)?;
        if real(m, advisor)? {
            found.push(advisor);
        }
    }
    Ok(found)
}

/// The advisors the player's pool offers, those not seated.
fn pool(m: &Memory, ctx: &Ctx) -> GResult<Vec<usize>> {
    let pools = m.ptr(ctx.world + POOLS)?;
    let index = ctx.player.index();
    if pools == 0 || index < 0 {
        return Ok(Vec::new());
    }
    let pool = m.ptr(pools + index as usize * 8)?;
    if pool == 0 {
        return Ok(Vec::new());
    }
    let seats = seated(m, ctx)?;
    let mut found = Vec::new();
    let mut node = m.ptr(pool + POOL_FIRST)?;
    while node != 0 {
        if found.len() >= MOST {
            return Err("an advisor pool longer than the framework reads".to_string().into());
        }
        let advisor = FIND_ID.call(m, [node + NODE_ID])?;
        if real(m, advisor)? && !seats.contains(&advisor) {
            found.push(advisor);
        }
        node = m.ptr(node + NODE_NEXT)?;
    }
    Ok(found)
}

/// Whether the player may fire the advisor now, and why not.
fn fire_choice(m: &Memory, advisor: usize) -> GResult<Choice> {
    let (can, reasons) = content::checked(m, |reasons| Ok(MAY_FIRE.call(m, [advisor, reasons])?))?;
    Ok(if can { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// Whether the player may promote the advisor now, why not, and its cost.
fn promote_choice(m: &Memory, ctx: &Ctx, advisor: usize) -> GResult<Choice> {
    let cost = vec![Amount::new(Resource::Ducats, thousandths(cost(m, PROMOTE_COST, advisor, ctx.player)?))];
    let (can, reasons) = content::checked(m, |reasons| Ok(MAY_PROMOTE.call(m, [advisor, ctx.player.0 as usize, reasons])?))?;
    Ok(if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// Whether the player may hire the advisor now, why not, and its cost, as
/// the court view's hire button decides (`0xecc8a3`): a treasury above the
/// cost, and an advisor of its power that may be fired, if there is one.
fn hire_choice(m: &Memory, ctx: &Ctx, advisor: usize) -> GResult<Choice> {
    let price = cost(m, HIRE_COST, advisor, ctx.player)?;
    let cost = vec![Amount::new(Resource::Ducats, thousandths(price))];
    let mut reasons = Vec::new();
    let treasury = m.i32(ctx.country + country_fields::TREASURY)?;
    if price >= treasury {
        reasons.push(format!("the treasury is {} ducats, and hiring this advisor costs {}", thousandths(treasury), thousandths(price)));
    }
    let current = m.ptr(ctx.country + SEATS + seat(power(m, advisor)?) * 8)?;
    if real(m, current)? {
        let fire = fire_choice(m, current)?;
        if !fire.can && fire.reasons.is_empty() {
            reasons.push("the advisor in its seat cannot be fired now".to_string());
        }
        reasons.extend(fire.reasons);
    }
    Ok(if reasons.is_empty() { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// The player's advisors.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<HiredAdvisor>> {
    seated(m, &ctx)?
        .into_iter()
        .map(|a| Ok(HiredAdvisor { advisor: read(m, &ctx, a)?, fire: fire_choice(m, a)?, promote: promote_choice(m, &ctx, a)? }))
        .collect()
}

/// The advisors the court view offers to hire.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Vec<AdvisorOption>> {
    pool(m, &ctx)?.into_iter().map(|a| Ok(AdvisorOption { advisor: read(m, &ctx, a)?, choice: hire_choice(m, &ctx, a)? })).collect()
}

/// An advisor's ID as the game writes it.
fn id_block(number: u32) -> Block {
    Block::new().word("id", number).word("type", advisor::ID_TYPE)
}

/// Hiring into the seat of a power, or, with no advisor, emptying it.
fn change_text(country: Tag, power: PowerKind, advisor: Option<u32>) -> String {
    let block = Block::new().tag("country", country.letters()).word("index", seat(power));
    write(
        "change_advisor",
        match advisor {
            Some(n) => block.block("advisor", id_block(n)),
            None => block,
        },
    )
}

fn promote_text(country: Tag, advisor: u32) -> String {
    write("promote_advisor", Block::new().block("members", Block::new().tag("dummy", country.letters()).block("dummy", id_block(advisor))))
}

/// The advisor among `advisors` with the number `id`.
fn find(m: &Memory, advisors: Vec<usize>, id: u32, whose: &str) -> GResult<usize> {
    for advisor in advisors {
        if m.u32(advisor + advisor::NUMBER)? == id && m.u32(advisor + advisor::ID)? == advisor::ID_TYPE {
            return Ok(advisor);
        }
    }
    Err(checks::no_advisor(id, whose))
}

/// Hires an advisor the court view offers.
pub fn hire(Session { m, ctx, commands, .. }: Session, args::Hire { advisor: id }: args::Hire) -> GResult<()> {
    let advisor = find(m, pool(m, &ctx)?, id, "offered to the player")?;
    let choice = hire_choice(m, &ctx, advisor)?;
    action::carry_out(commands, &choice, "hire this advisor", || Ok(change_text(ctx.player, power(m, advisor)?, Some(id))))
}

/// Fires one of the player's advisors.
pub fn fire(Session { m, ctx, commands, .. }: Session, args::Fire { advisor: id }: args::Fire) -> GResult<()> {
    let advisor = find(m, seated(m, &ctx)?, id, "of the player's")?;
    let choice = fire_choice(m, advisor)?;
    action::carry_out(commands, &choice, "fire this advisor", || Ok(change_text(ctx.player, power(m, advisor)?, None)))
}

/// Promotes one of the player's advisors.
pub fn promote(Session { m, ctx, commands, .. }: Session, args::Promote { advisor: id }: args::Promote) -> GResult<()> {
    let advisor = find(m, seated(m, &ctx)?, id, "of the player's")?;
    let choice = promote_choice(m, &ctx, advisor)?;
    action::carry_out(commands, &choice, "promote this advisor", || Ok(promote_text(ctx.player, id)))
}

crate::catalogue::bind! {
    eu4_harmony_remote::advisors: AdvisorRequest => AdvisorAnswer;
    List => list;
    Options => options;
    Hire => hire;
    Fire => fire;
    Promote => promote;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mam() -> Tag {
        Tag(u64::from_le_bytes(*b"MAM\0\0\0\0\0"))
    }

    #[test]
    fn hiring_and_firing_name_the_seat_and_the_advisor() {
        assert_eq!(change_text(mam(), PowerKind::Dip, Some(3550)), r#"change_advisor={ country="MAM" index=1 advisor={ id=3550 type=51 } }"#);
        assert_eq!(change_text(mam(), PowerKind::Mil, None), r#"change_advisor={ country="MAM" index=2 }"#);
    }

    #[test]
    fn promoting_carries_the_country_and_the_advisor_as_members() {
        assert_eq!(promote_text(mam(), 3549), r#"promote_advisor={ members={ dummy="MAM" dummy={ id=3549 type=51 } } }"#);
    }
}
