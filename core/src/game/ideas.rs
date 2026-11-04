//! Ideas: the idea groups of the ideas view, taking one, buying its ideas
//! and abandoning it, through the game's own commands
//! (docs/research/government.md, "Ideas").
use super::{conditions, content, context, game, Caller, GResult, Tag};
use crate::{memory::Memory, points::Point};
use eu4_harmony_checks::government as checks;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    ideas::IdeaGroup,
};

/// The idea groups' database, made on first use by its getter: the groups
/// at `+0x20` and the ideas at `+0x38`, `std::vector`s of pointers.
const DATABASE: usize = 0x242bce0;
const GROUPS: usize = 0x20;
const IDEAS: usize = 0x38;
/// An idea group: its key, its ideas (a `std::vector` of pointers), the
/// conditions for taking it (`trigger`) and for buying its ideas
/// (`can_buy_idea`), whether it is free (the national ideas), its category
/// (0 administrative, 1 diplomatic, 2 military) and its index.
mod group {
    pub const KEY: usize = 0x10;
    pub const IDEAS: usize = 0x30;
    pub const TRIGGER: usize = 0x48;
    pub const CAN_BUY: usize = 0xc0;
    pub const FREE: usize = 0x568;
    pub const CATEGORY: usize = 0x570;
    pub const INDEX: usize = 0x5b4;
}
/// An idea's key.
const IDEA_KEY: usize = 0x10;
/// The country's idea groups taken: a `std::vector` of 16-byte items, the
/// group and the count of its ideas bought; and a byte for each group, by
/// its index, set when it is taken.
const TAKEN: usize = 0x1438;
const TAKEN_ITEM: usize = 0x10;
const TAKEN_COUNT: usize = 0x8;
const TAKEN_FLAGS: usize = 0x10a0;
/// `(country) -> bool`: whether the country has an idea group slot free.
const SLOT_FREE: usize = 0x32e2d0;
/// An idea group's vtable slot `(group, tag) -> bool`: whether the country
/// may take it, its `trigger`.
const ALLOWED: usize = 0x10;
/// The power spending: its vtable, and the global its `+0x28` takes as the
/// game's own callers make one; `(spending, tag, text, 0)`, the cost in whole
/// monarch points; `(spending, tag) -> bool`, the spend command's check.
const SPENDING_VTABLE: usize = 0x1c55480;
const SPENDING_STAMP: usize = 0x233fea8;
const SPENDING_COST: usize = 0x9526d0;
const SPENDING_CHECK: usize = 0x94f250;
/// The spending's kind for buying an idea.
const BUY_IDEA: u32 = 0;
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // The getter that makes the database and keeps it in the global.
        Point::keep(0x85075ausize, "48 8b 05 7f b5 bd 01 48 85 c0", "idea groups database").uses(&[DATABASE]),
        // The buy check finds the idea in the database's ideas and its
        // group, then whether the group is free.
        Point::keep(0x94f2fbusize, "8b 4e 18 48 8b 40 38 48 8b 0c c8 48 8b 81 98 01 00 00 80 b8 68 05 00 00 00", "buy idea check").uses(&[IDEAS, group::FREE]),
        // The country's groups taken, the first left out when free, and the
        // slots the technology gives.
        Point::keep(SLOT_FREE, "48 8b 81 38 14 00 00 48 8b 91 40 14 00 00", "idea group slot free").uses(&[TAKEN]),
        // The take command's check: not free, allowed, not taken, a slot.
        Point::keep(0x2a7e79usize, "48 8b 49 58 80 b9 68 05 00 00 00 75 55 48 8b 01 48 8b 53 50 ff 50 10", "take idea group check").uses(&[ALLOWED]),
        Point::keep(0x2a7eb6usize, "48 63 90 b4 05 00 00 48 8b 81 a0 10 00 00", "idea group taken flag").uses(&[group::INDEX, TAKEN_FLAGS]),
        // Whether the country has an idea: the group's ideas by the count
        // bought.
        Point::keep(0x332723usize, "48 8b 14 24 4c 8b 4a 30", "ideas of a group").uses(&[group::IDEAS]),
        // The group's reader: `trigger`, `can_buy_idea` and the category.
        Point::keep(0x851489usize, "45 89 75 5c 4d 8b 45 50", "idea group trigger"),
        Point::keep(0x851580usize, "45 89 b5 d4 00 00 00 49 8d 9d c0 00 00 00", "idea group can buy").uses(&[group::CAN_BUY]),
        Point::keep(0x851cdfusize, "45 89 b5 70 05 00 00", "idea group category").uses(&[group::CATEGORY]),
        Point::keep(SPENDING_COST, "48 8b c4 48 89 58 10 4c 89 48 20 4c 89 40 18 48", "power spending cost"),
        Point::keep(SPENDING_CHECK, "48 89 5c 24 10 48 89 74 24 18 55 57 41 56 48 8b ec 48 81 ec 80 00 00 00", "power spending check"),
        // A spending the game makes on its stack: its vtable and stamp.
        Point::keep(0x94f3bfusize, "48 8d 05 ba 60 30 01 48 89 45 c8", "power spending vtable").uses(&[SPENDING_VTABLE]),
        Point::keep(0x94f3e5usize, "48 8b 05 bc 0a 9f 01 48 89 45 f0", "power spending stamp").uses(&[SPENDING_STAMP]),
    ]
}

/// An idea group of the database.
struct Group {
    at: usize,
    key: String,
    category: u32,
    ideas: Vec<usize>,
}

fn database(m: &Memory) -> GResult<usize> {
    match m.ptr(m.at(DATABASE))? {
        0 => Err("the idea groups are not read yet".to_string().into()),
        database => Ok(database),
    }
}

fn groups(m: &Memory) -> GResult<Vec<Group>> {
    content::vector(m, database(m)? + GROUPS)?
        .into_iter()
        .map(|at| {
            Ok(Group { at, key: content::key(m, at + group::KEY)?, category: m.u32(at + group::CATEGORY)?, ideas: content::vector(m, at + group::IDEAS)? })
        })
        .collect()
}

/// The player's idea groups taken, in the order taken, with the count of
/// ideas bought in each.
fn taken(m: &Memory, country: usize) -> GResult<Vec<(usize, u32)>> {
    let (begin, end) = (m.ptr(country + TAKEN)?, m.ptr(country + TAKEN + 8)?);
    (0..super::span(begin, end, TAKEN_ITEM, MOST)?)
        .map(|i| Ok((m.ptr(begin + i * TAKEN_ITEM)?, m.i32(begin + i * TAKEN_ITEM + TAKEN_COUNT)?.max(0) as u32)))
        .collect()
}

fn category(category: u32) -> (&'static str, Resource) {
    match category {
        1 => ("dip", Resource::Dip),
        2 => ("mil", Resource::Mil),
        _ => ("adm", Resource::Adm),
    }
}

/// A power spending of the game's for buying `idea`, in the framework's
/// memory, as the game's own callers make one.
fn spending(m: &Memory, idea: u32, skill: u32) -> GResult<[u64; 8]> {
    let mut s = [0u64; 8];
    s[0] = m.at(SPENDING_VTABLE) as u64;
    s[1] = 0x165;
    s[2] = BUY_IDEA as u64 | (skill as u64) << 32;
    s[3] = idea as u64 | 0xffff_ffff << 32;
    s[5] = m.ptr(m.at(SPENDING_STAMP))? as u64;
    Ok(s)
}

/// The index of `idea` in the database's ideas, which the spend command
/// names it by.
fn idea_index(m: &Memory, idea: usize) -> GResult<u32> {
    let ideas = content::vector(m, database(m)? + IDEAS)?;
    match ideas.iter().position(|&i| i == idea) {
        Some(i) => Ok(i as u32),
        None => Err("an idea not in the database".to_string().into()),
    }
}

/// Whether the player can buy `idea` of `group`, why not, and its cost.
fn buy_choice(m: &Memory, player: Tag, group: &Group, idea: usize) -> GResult<Choice> {
    let mut spending = spending(m, idea_index(m, idea)?, group.category)?;
    let at = spending.as_mut_ptr() as usize;
    let cost = m.call(m.at(SPENDING_COST), &[at, player.0 as usize, 0, 0])? as u32 as i32;
    let cost = vec![Amount::new(category(group.category).1, cost as f64)];
    if m.call(m.at(SPENDING_CHECK), &[at, player.0 as usize])? & 0xff != 0 {
        return Ok(Choice::can(cost));
    }
    Ok(Choice::cannot(conditions::failing(m, group.at + group::CAN_BUY, player, 0)?, cost))
}

/// Whether the player can take `group`, and why not.
fn take_choice(m: &Memory, country: usize, player: Tag, group: &Group) -> GResult<Choice> {
    let allowed = m.call(m.vslot(group.at, ALLOWED)?, &[group.at, player.0 as usize])? & 0xff != 0;
    let mut reasons = if allowed { Vec::new() } else { conditions::failing(m, group.at + group::TRIGGER, player, 0)? };
    if m.call(m.at(SLOT_FREE), &[country])? & 0xff == 0 {
        reasons.push("the player has no free idea group slot".to_string());
    }
    Ok(match reasons.is_empty() && allowed {
        true => Choice::can(Vec::new()),
        false => Choice::cannot(reasons, Vec::new()),
    })
}

/// The idea groups of the ideas view, those taken first.
pub fn list(_: Caller) -> GResult<Vec<IdeaGroup>> {
    let m = game()?;
    let ctx = context(m)?;
    let taken = taken(m, ctx.country)?;
    let mut groups: Vec<Group> = groups(m)?.into_iter().filter(|g| m.u8(g.at + group::FREE).is_ok_and(|free| free == 0)).collect();
    groups.sort_by_key(|g| taken.iter().position(|&(t, _)| t == g.at).unwrap_or(usize::MAX));
    groups
        .iter()
        .map(|g| {
            let bought = taken.iter().find(|&&(t, _)| t == g.at).map(|&(_, n)| n);
            let next = bought.and_then(|n| g.ideas.get(n as usize).copied());
            let (button, choice) = match (bought, next) {
                (None, _) => (Some("take_group"), take_choice(m, ctx.country, ctx.player, g)?),
                (Some(_), Some(idea)) => (Some("buy"), buy_choice(m, ctx.player, g, idea)?),
                (Some(_), None) => (None, Choice::cannot(Vec::new(), Vec::new())),
            };
            Ok(IdeaGroup {
                group: g.key.clone(),
                name: content::name(m, &g.key)?,
                category: category(g.category).0.to_string(),
                taken: bought.is_some(),
                ideas: bought.unwrap_or(0),
                of: g.ideas.len() as u32,
                button: button.map(str::to_string),
                next: next.map(|idea| content::key(m, idea + IDEA_KEY)).transpose()?,
                choice,
            })
        })
        .collect()
}

fn find(m: &Memory, key: &str) -> GResult<Group> {
    match groups(m)?.into_iter().find(|g| g.key == key && m.u8(g.at + group::FREE).is_ok_and(|free| free == 0)) {
        Some(group) => Ok(group),
        None => Err(checks::not_found("idea group", key).into()),
    }
}

/// The add and abandon commands: the country and the group by its key.
fn written(command: &str, country: Tag, group: &str) -> String {
    format!("{command}={{ country=\"{}\" idea_group=\"{group}\" }}", content::letters(country))
}

/// The spend command for buying the idea at `index` of the database, its
/// kind of monarch points `skill`; the game leaves `data` out when it is 0.
fn written_buy(country: Tag, index: u32, skill: u32) -> String {
    let data = if index == 0 { String::new() } else { format!("data={index} ") };
    format!("spend_power_command={{ country=\"{}\" power={{ {data}category={BUY_IDEA} skill={skill} }} }}", content::letters(country))
}

/// Takes an idea group for the player, as its button does.
pub fn take_group(_: Caller, key: String) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let group = find(m, &key)?;
    if taken(m, ctx.country)?.iter().any(|&(t, _)| t == group.at) {
        return Err(checks::not_now(format!("the player has taken the idea group {key} already")).into());
    }
    let choice = take_choice(m, ctx.country, ctx.player, &group)?;
    if !choice.can {
        return content::refused(&choice.reasons, "taking this idea group");
    }
    content::post(&written("add_idea_group", ctx.player, &key), "taking this idea group")
}

/// Buys the next idea of an idea group the player has taken.
pub fn buy(_: Caller, key: String) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let group = find(m, &key)?;
    let Some(&(_, bought)) = taken(m, ctx.country)?.iter().find(|&&(t, _)| t == group.at) else {
        return Err(checks::not_now(format!("the player has not taken the idea group {key}")).into());
    };
    let Some(&idea) = group.ideas.get(bought as usize) else {
        return Err(checks::not_now(format!("every idea of {key} is bought")).into());
    };
    let choice = buy_choice(m, ctx.player, &group, idea)?;
    if !choice.can {
        return content::refused(&choice.reasons, "buying this idea");
    }
    content::post(&written_buy(ctx.player, idea_index(m, idea)?, group.category), "buying this idea")
}

/// Abandons an idea group the player has taken.
pub fn abandon_group(_: Caller, key: String) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let group = find(m, &key)?;
    if !taken(m, ctx.country)?.iter().any(|&(t, _)| t == group.at) {
        return Err(checks::not_now(format!("the player has not taken the idea group {key}")).into());
    }
    content::post(&written("abandon_idea_group", ctx.player, &key), "abandoning this idea group")
}

#[cfg(test)]
mod tests {
    use super::{written, written_buy, Tag};

    fn fra() -> Tag {
        Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"))
    }

    #[test]
    fn the_commands_name_the_country_and_the_group() {
        assert_eq!(written("add_idea_group", fra(), "defensive_ideas"), r#"add_idea_group={ country="FRA" idea_group="defensive_ideas" }"#);
    }

    #[test]
    fn buying_an_idea_spends_power_on_it_by_its_index() {
        assert_eq!(written_buy(fra(), 89, 2), r#"spend_power_command={ country="FRA" power={ data=89 category=0 skill=2 } }"#);
        assert_eq!(written_buy(fra(), 0, 0), r#"spend_power_command={ country="FRA" power={ category=0 skill=0 } }"#);
    }
}
