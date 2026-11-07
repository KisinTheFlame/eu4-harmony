//! Technology: the player's three technologies as the technology view shows
//! them, and researching the next level with its research button
//! (docs/research/power-spending.md, "Technology").
use super::{
    action,
    power::{self, ADVANCE_TECH},
    GResult, Session, Tag,
};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign::{country, technology as levels},
        gui::functions::ADD_MONTHS,
    },
    points::Point,
};
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind},
    technology::TechnologyOption,
};

/// The technology categories: `+0x18` and `+0x20` a vector of pointers.
const CATEGORIES: usize = 0x242be50;
const CATEGORY_LIST: usize = 0x18;
/// A category's index, its levels (a vector of pointers, one a level, the
/// first the level a country starts at) and its kind of monarch power.
const INDEX: usize = 0x50;
const CATEGORY_LEVELS: usize = 0x60;
const CATEGORY_LEVELS_END: usize = 0x68;
const KIND: usize = 0x78;
/// How many months until a country has `cost` points of a kind, at its
/// monthly gain: `(country, &cost in thousandths, kind)`; 0 or less when it
/// gains none.
const MONTHS_UNTIL: usize = 0x380910;
/// The most categories and levels believed.
const MOST: usize = 1000;

pub fn points() -> Vec<Point> {
    vec![
        // The technology view reads the player's level in its category and
        // the category's number of levels.
        Point::keep(
            0x10093bcusize,
            "49 8b 87 40 24 00 00 49 89 86 50 01 00 00 4d 8b 86 40 01 00 00 45 8b 48 50 48 8b 80 28 01 00 00 42 8b 0c 88 49 8b 40 68 49 2b 40 60",
            "technology view's level",
        )
        .uses(&[country::TECHNOLOGY, INDEX, levels::LEVELS, CATEGORY_LEVELS_END, CATEGORY_LEVELS]),
        Point::keep(0x9bdb3usize, "48 8b 1d 96 00 39 02 48 8b 43 20 48 2b 43 18", "technology categories").uses(&[CATEGORIES, CATEGORY_LIST]),
        Point::keep(MONTHS_UNTIL, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 20 57 48 83 ec 20 8b 1a 48 8b", "months until points"),
    ]
}

/// One technology of the player's: its category, its level and its last.
struct Category {
    kind: PowerKind,
    index: i32,
    level: i32,
    last: i32,
}

fn categories(m: &Memory, country: usize) -> GResult<Vec<Category>> {
    let database = m.ptr(m.at(CATEGORIES))?;
    if database == 0 {
        return Err("no technology categories".to_string().into());
    }
    let begin = m.ptr(database + CATEGORY_LIST)?;
    let count = super::span(begin, m.ptr(database + CATEGORY_LIST + 8)?, 8, MOST)?;
    let technology = m.ptr(country + country::TECHNOLOGY)?;
    let level_of = m.ptr(technology + levels::LEVELS)?;
    let known = super::span(level_of, m.ptr(technology + levels::LEVELS + 8)?, 4, MOST)?;
    let mut found = Vec::new();
    for i in 0..count {
        let category = m.ptr(begin + i * 8)?;
        let kind = match m.i32(category + KIND)? {
            0 => PowerKind::Adm,
            1 => PowerKind::Dip,
            2 => PowerKind::Mil,
            _ => continue,
        };
        let index = m.i32(category + INDEX)?;
        if index < 0 || index as usize >= known {
            return Err(format!("technology category {index} has no level").into());
        }
        let last = super::span(m.ptr(category + CATEGORY_LEVELS)?, m.ptr(category + CATEGORY_LEVELS_END)?, 8, MOST)? as i32 - 1;
        found.push(Category { kind, index, level: m.i32(level_of + index as usize * 4)?, last });
    }
    Ok(found)
}

fn category(m: &Memory, country: usize, kind: PowerKind) -> GResult<Category> {
    categories(m, country)?.into_iter().find(|c| c.kind == kind).ok_or_else(|| format!("no {kind:?} technology").into())
}

/// Whether `player` can research the next level of `category` now, as the
/// view's button says: at the last level it cannot, in the framework's words,
/// as research refuses it; with too few points the reasons are the tooltip's
/// lines saying so and when it will have them.
fn choice(m: &Memory, player: Tag, country: usize, date: i32, category: &Category) -> GResult<Choice> {
    if category.level >= category.last {
        return Ok(Choice::cannot(vec![eu4_harmony_checks::technology::last_level(key(category.kind)).message], Vec::new()));
    }
    let cost = power::cost(m, player, ADVANCE_TECH, category.kind, category.index)?;
    let price = vec![Amount::new(category.kind.resource(), cost as f64)];
    let have = power::points_of(m, country, category.kind)?;
    if cost <= have {
        return Ok(Choice::can(price));
    }
    let kind = crate::unicode::game_text::to_utf8(&game_string::localized(m, power_name(category.kind).as_ptr() as usize)?);
    let values = [(c"TYPE", kind), (c"POWER_COST", power::whole(cost)), (c"POWER_AVAILABLE", power::whole(have))];
    let mut reasons = power::tooltip(m, c"CV_RESEARCH_TECH_NOPOWER", &values)?;
    reasons.extend(when(m, country, date, cost, category.kind)?);
    Ok(Choice::cannot(reasons, price))
}

/// The localisation key of a kind of power's name, as the technology view
/// writes it in its tooltip (`0xa491f0`).
fn power_name(kind: PowerKind) -> &'static std::ffi::CStr {
    match kind {
        PowerKind::Adm => c"MON_ADM_LONG",
        PowerKind::Dip => c"MON_DIP_LONG",
        PowerKind::Mil => c"MON_MIL_LONG",
    }
}

/// The technology view's line of when the country will have `cost` points
/// of `kind`: the date at its monthly gain, or that it gains none.
fn when(m: &Memory, country: usize, date: i32, cost: i32, kind: PowerKind) -> GResult<Vec<String>> {
    let thousandths = cost.saturating_mul(1000);
    let months = m.call(m.at(MONTHS_UNTIL), &[country, &thousandths as *const i32 as usize, kind as usize])? as i32;
    if months <= 0 {
        return power::tooltip(m, c"RESEARCH_NEVER", &[]);
    }
    let mut day = date;
    m.call(m.at(ADD_MONTHS), &[&mut day as *mut i32 as usize, months as usize])?;
    power::tooltip(m, c"RESEARCH_WHEN", &[(c"DATE", power::date_text(m, day)?)])
}

/// The player's technologies, administrative, diplomatic and military.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Vec<TechnologyOption>> {
    let found = categories(m, ctx.country)?;
    PowerKind::ALL
        .into_iter()
        .map(|kind| {
            let category = found.iter().find(|c| c.kind == kind).ok_or_else(|| format!("no {kind:?} technology"))?;
            Ok(TechnologyOption { technology: kind, level: category.level.max(0) as u32, choice: choice(m, ctx.player, ctx.country, ctx.date, category)? })
        })
        .collect()
}

/// Researches the next level of `technology`, as the view's research button
/// does: the game's spend_power command with the category's index.
pub fn research(Session { m, ctx, commands, .. }: Session, technology: PowerKind) -> GResult<()> {
    let category = category(m, ctx.country, technology)?;
    if category.level >= category.last {
        return Err(eu4_harmony_checks::technology::last_level(key(technology)).into());
    }
    action::allowed(&choice(m, ctx.player, ctx.country, ctx.date, &category)?, "research it")?;
    power::spend(commands, ctx.player, ADVANCE_TECH, technology, category.index, "research it")
}

/// A kind of power as its argument names it.
pub fn key(kind: PowerKind) -> &'static str {
    match kind {
        PowerKind::Adm => "adm",
        PowerKind::Dip => "dip",
        PowerKind::Mil => "mil",
    }
}
