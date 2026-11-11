//! Ideas: the idea groups of the ideas view, taking one, buying its ideas
//! and abandoning it, through the game's own commands
//! (docs/research/government.md, "Ideas").
use super::commands::{write, Block};
use super::content::checks;
use super::{action, conditions, content, power, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    objects::campaign::{country, idea_group},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::ideas::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    ideas::IdeaGroupOption,
};

/// The idea groups' database, made on first use by its getter: the groups
/// at `+0x20` and the ideas at `+0x38`, `std::vector`s of pointers.
const DATABASE: usize = 0x242bce0;
const GROUPS: usize = 0x20;
const IDEAS: usize = 0x38;
/// An idea group: its key, its ideas (a `std::vector` of pointers), the
/// conditions for taking it (`trigger`) and for buying its ideas
/// (`can_buy_idea`), its category (0 administrative, 1 diplomatic, 2
/// military) and its index; whether it is free is
/// [`idea_group::FREE`].
mod group {
    pub const KEY: usize = 0x10;
    pub const IDEAS: usize = 0x30;
    pub const TRIGGER: usize = 0x48;
    pub const CAN_BUY: usize = 0xc0;
    pub const CATEGORY: usize = 0x570;
    pub const INDEX: usize = 0x5b4;
}
/// An idea's key (an idea is of class `0x1cbaa30`).
const IDEA_KEY: usize = 0x158;
/// `(country) -> bool`: whether the country has an idea group slot free.
const SLOT_FREE: GameFunction<1> = GameFunction::at(0x32e2d0);
/// `(group, tag) -> bool`, whether the country may take the group, its
/// `trigger`: what every group's vtable holds at `+0x10`.
const GROUP_ALLOWED: GameFunction<2> = GameFunction::at(0x84f1c0);
/// The spending's kind for buying an idea.
const BUY_IDEA: i32 = 0;
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // The getter that makes the database and keeps it in the global.
        Point::keep(0x85075ausize, "48 8b 05 7f b5 bd 01 48 85 c0", "idea groups database").uses(&[DATABASE]),
        // The buy check finds the idea in the database's ideas and its
        // group, then whether the group is free.
        Point::keep(0x94f2fbusize, "8b 4e 18 48 8b 40 38 48 8b 0c c8 48 8b 81 98 01 00 00 80 b8 68 05 00 00 00", "buy idea check")
            .uses(&[IDEAS, idea_group::FREE]),
        // The country's groups taken, the first left out when free, and the
        // slots the technology gives.
        // The national ideas take no slot.
        Point::keep(
            SLOT_FREE,
            "48 8b 81 38 14 00 00 48 8b 91 40 14 00 00 48 2b d0 48 c1 fa 04 85 d2 7e 0e 48 8b 00 80 b8 68 05 00 00 00",
            "idea group slot free",
        )
        .uses(&[country::IDEA_GROUPS, country::IDEA_GROUPS + 8, idea_group::FREE]),
        // The take command's check: not free, allowed, not taken, a slot.
        Point::keep(0x2a7e79usize, "48 8b 49 58 80 b9 68 05 00 00 00 75 55 48 8b 01 48 8b 53 50 ff 50 10", "take idea group check"),
        Point::keep(0x2a7eb6usize, "48 63 90 b4 05 00 00 48 8b 81 a0 10 00 00", "idea group taken flag").uses(&[group::INDEX, country::ENTRY_BYTES]),
        // Whether the country has an idea: the group's ideas by the count
        // bought.
        Point::keep(0x332723usize, "48 8b 14 24 4c 8b 4a 30", "ideas of a group").uses(&[group::IDEAS]),
        // The group's reader: `trigger`, `can_buy_idea` and the category.
        Point::keep(0x851489usize, "45 89 75 5c 4d 8b 45 50", "idea group trigger"),
        Point::keep(0x851580usize, "45 89 b5 d4 00 00 00 49 8d 9d c0 00 00 00", "idea group can buy").uses(&[group::CAN_BUY]),
        Point::keep(0x851cdfusize, "45 89 b5 70 05 00 00", "idea group category").uses(&[group::CATEGORY]),
        // The group's constructor makes an idea and its key.
        Point::keep(0x84e65cusize, "48 8d 8b 58 01 00 00", "idea key").uses(&[IDEA_KEY]),
        Point::keep(GROUP_ALLOWED, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55", "idea group allowed"),
        // The spend check compares the country's points of the skill with
        // the cost.
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

/// The key of the database's placeholder group, which the ideas view does
/// not show.
const NONE: &str = "none";

fn groups(m: &Memory) -> GResult<Vec<Group>> {
    containers::pointers(m, database(m)? + GROUPS, containers::MOST)?
        .into_iter()
        .map(|at| {
            Ok(Group {
                at,
                key: game_string::key(m, at + group::KEY)?,
                category: m.u32(at + group::CATEGORY)?,
                ideas: containers::pointers(m, at + group::IDEAS, containers::MOST)?,
            })
        })
        .filter(|g: &GResult<Group>| g.as_ref().map_or(true, |g| g.key != NONE))
        .collect()
}

/// The player's idea groups taken, in the order taken, with the count of
/// ideas bought in each.
fn taken(m: &Memory, country: usize) -> GResult<Vec<(usize, u32)>> {
    let (begin, end) = (m.ptr(country + country::IDEA_GROUPS)?, m.ptr(country + country::IDEA_GROUPS + 8)?);
    (0..containers::span(begin, end, country::IDEA_GROUP_SIZE, MOST)?)
        .map(|i| Ok((m.ptr(begin + i * country::IDEA_GROUP_SIZE)?, m.i32(begin + i * country::IDEA_GROUP_SIZE + country::IDEA_GROUP_COUNT)?.max(0) as u32)))
        .collect()
}

fn category(category: u32) -> (&'static str, Resource) {
    match category {
        1 => ("dip", Resource::Dip),
        2 => ("mil", Resource::Mil),
        _ => ("adm", Resource::Adm),
    }
}

/// The kind of monarch power a group's ideas are bought with.
fn power_of(category: u32) -> PowerKind {
    match category {
        1 => PowerKind::Dip,
        2 => PowerKind::Mil,
        _ => PowerKind::Adm,
    }
}

/// The index of `idea` in the database's ideas, which the spend command
/// names it by.
fn idea_index(m: &Memory, idea: usize) -> GResult<u32> {
    let ideas = containers::pointers(m, database(m)? + IDEAS, containers::MOST)?;
    match ideas.iter().position(|&i| i == idea) {
        Some(i) => Ok(i as u32),
        None => Err("an idea not in the database".to_string().into()),
    }
}

/// Whether the player can buy `idea` of `group`, why not, and its cost.
fn buy_choice(m: &Memory, country: usize, player: Tag, group: &Group, idea: usize) -> GResult<Choice> {
    let mut spending = power::Spending::new(m, BUY_IDEA, power_of(group.category), idea_index(m, idea)? as i32)?;
    let cost = spending.cost(m, player)?;
    let cost = vec![Amount::new(category(group.category).1, cost as f64)];
    if spending.allowed(m, player)? {
        return Ok(Choice::can(cost));
    }
    let mut reasons = conditions::failing(m, group.at + group::CAN_BUY, player, 0)?;
    let (kind, points, needed) = (category(group.category).0, power::points_of(m, country, power_of(group.category))?, cost[0].amount);
    if (points as f64) < needed {
        reasons.push(format!("buying it takes {needed} {kind} points, and the player has {points}"));
    }
    Ok(Choice::cannot(reasons, cost))
}

/// Whether the player can take `group`, and why not.
fn take_choice(m: &Memory, country: usize, player: Tag, group: &Group) -> GResult<Choice> {
    let allowed = GROUP_ALLOWED.call(m, [group.at, player.0 as usize])? & 0xff != 0;
    let mut reasons = if allowed { Vec::new() } else { conditions::failing(m, group.at + group::TRIGGER, player, 0)? };
    if SLOT_FREE.call(m, [country])? & 0xff == 0 {
        reasons.push("the player has no free idea group slot".to_string());
    }
    Ok(match reasons.is_empty() && allowed {
        true => Choice::can(Vec::new()),
        false => Choice::cannot(reasons, Vec::new()),
    })
}

/// The idea groups of the ideas view, those taken first.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<IdeaGroupOption>> {
    let taken = taken(m, ctx.country)?;
    let mut groups: Vec<Group> = groups(m)?.into_iter().filter(|g| m.u8(g.at + idea_group::FREE).is_ok_and(|free| free == 0)).collect();
    groups.sort_by_key(|g| taken.iter().position(|&(t, _)| t == g.at).unwrap_or(usize::MAX));
    groups
        .iter()
        .map(|g| {
            let bought = taken.iter().find(|&&(t, _)| t == g.at).map(|&(_, n)| n);
            let next = bought.and_then(|n| g.ideas.get(n as usize).copied());
            let (button, choice) = match (bought, next) {
                (None, _) => (Some("take_group"), take_choice(m, ctx.country, ctx.player, g)?),
                (Some(_), Some(idea)) => (Some("buy"), buy_choice(m, ctx.country, ctx.player, g, idea)?),
                (Some(_), None) => (None, Choice::cannot(Vec::new(), Vec::new())),
            };
            Ok(IdeaGroupOption {
                group: g.key.clone(),
                name: content::name(m, &g.key)?,
                category: category(g.category).0.to_string(),
                taken: bought.is_some(),
                ideas: bought.unwrap_or(0),
                of: g.ideas.len() as u32,
                button: button.map(str::to_string),
                next: next.map(|idea| game_string::key(m, idea + IDEA_KEY)).transpose()?,
                choice,
            })
        })
        .collect()
}

fn find(m: &Memory, key: &str) -> GResult<Group> {
    match groups(m)?.into_iter().find(|g| g.key == key && m.u8(g.at + idea_group::FREE).is_ok_and(|free| free == 0)) {
        Some(group) => Ok(group),
        None => Err(checks::not_found("idea group", key)),
    }
}

/// The add and abandon commands: the country and the group by its key.
fn written(command: &str, country: Tag, group: &str) -> String {
    write(command, Block::new().tag("country", country.letters()).quoted("idea_group", group))
}

/// The spend command for buying the idea at `index` of the database, its
/// kind of monarch points `skill`; the game leaves `data` out when it is 0.
fn written_buy(country: Tag, index: u32, skill: u32) -> String {
    let power = if index == 0 { Block::new() } else { Block::new().word("data", index) };
    let power = power.word("category", BUY_IDEA).word("skill", skill);
    write("spend_power_command", Block::new().tag("country", country.letters()).block("power", power))
}

/// Takes an idea group for the player, as its button does.
pub fn take_group(Session { m, ctx, commands, .. }: Session, args::TakeGroup { group: key }: args::TakeGroup) -> GResult<()> {
    let group = find(m, &key)?;
    if taken(m, ctx.country)?.iter().any(|&(t, _)| t == group.at) {
        return Err(checks::not_now(format!("the player has taken the idea group {key} already")));
    }
    let choice = take_choice(m, ctx.country, ctx.player, &group)?;
    if !choice.can {
        return Err(action::not_allowed("take this idea group", &choice.reasons));
    }
    action::post(commands, &written("add_idea_group", ctx.player, &key), "take this idea group")
}

/// Buys the next idea of an idea group the player has taken.
pub fn buy(Session { m, ctx, commands, .. }: Session, args::Buy { group: key }: args::Buy) -> GResult<()> {
    let group = find(m, &key)?;
    let Some(&(_, bought)) = taken(m, ctx.country)?.iter().find(|&&(t, _)| t == group.at) else {
        return Err(checks::not_now(format!("the player has not taken the idea group {key}")));
    };
    let Some(&idea) = group.ideas.get(bought as usize) else {
        return Err(checks::not_now(format!("every idea of {key} is bought")));
    };
    let choice = buy_choice(m, ctx.country, ctx.player, &group, idea)?;
    if !choice.can {
        return Err(action::not_allowed("buy this idea", &choice.reasons));
    }
    action::post(commands, &written_buy(ctx.player, idea_index(m, idea)?, group.category), "buy this idea")
}

/// Abandons an idea group the player has taken.
pub fn abandon_group(Session { m, ctx, commands, .. }: Session, args::AbandonGroup { group: key }: args::AbandonGroup) -> GResult<()> {
    let group = find(m, &key)?;
    if !taken(m, ctx.country)?.iter().any(|&(t, _)| t == group.at) {
        return Err(checks::not_now(format!("the player has not taken the idea group {key}")));
    }
    action::post(commands, &written("abandon_idea_group", ctx.player, &key), "abandon this idea group")
}

crate::catalogue::bind! {
    eu4_harmony_remote::ideas: IdeaRequest => IdeaAnswer;
    List => list;
    TakeGroup => take_group;
    Buy => buy;
    AbandonGroup => abandon_group;
}

#[cfg(test)]
mod tests {
    use crate::game::As;
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

    use super::*;
    use crate::game::{
        buffers::{self, Image, Object},
        commands::record,
        Caller, Fail, Status,
    };

    /// France, of the idea groups `defensive_ideas` and `trade_ideas`, of
    /// two ideas each, the first taken with `bought` of them bought; the
    /// groups' database a global of the game's. Runs `action` over its
    /// session.
    fn france(bought: i32, action: impl FnOnce(Session) -> GResult<()>) -> GResult<()> {
        let image = Image::new();
        let group = |key: &str| Object::new(0x600).string(group::KEY, key).vector(group::IDEAS, &[1, 2]);
        let (defensive, trade) = (group("defensive_ideas"), group("trade_ideas"));
        let database = Object::new(GROUPS + 0x10).vector(GROUPS, &[defensive.at() as u64, trade.at() as u64]);
        image.u64(DATABASE, database.at() as u64);
        let entry = Object::new(country::IDEA_GROUP_SIZE).u64(0, defensive.at() as u64).i32(country::IDEA_GROUP_COUNT, bought);
        let end = (entry.at() + country::IDEA_GROUP_SIZE) as u64;
        let country = Object::country(fra(), country::IDEA_GROUPS + 0x10).points_to(country::IDEA_GROUPS, entry).u64(country::IDEA_GROUPS + 8, end);
        let countries = Object::countries(&[&country]);
        let result = action(buffers::session(image.memory(), 0, fra(), country.at(), countries.at(), Caller::Remote(As::Player)));
        drop((defensive, trade, database, country, countries, image));
        result
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the action was carried out") };
        (status, message)
    }

    #[test]
    fn an_idea_group_is_abandoned_only_when_taken_and_refusals_say_why() {
        let group = |key: &str| key.to_string();
        france(0, |s| abandon_group(s, args::AbandonGroup { group: group("defensive_ideas") })).unwrap();
        assert_eq!(record::posted(), [r#"abandon_idea_group={ country="FRA" idea_group="defensive_ideas" }"#]);
        let refusals = [
            refused(france(0, |s| abandon_group(s, args::AbandonGroup { group: group("trade_ideas") }))),
            refused(france(0, |s| take_group(s, args::TakeGroup { group: group("defensive_ideas") }))),
            refused(france(0, |s| buy(s, args::Buy { group: group("trade_ideas") }))),
            refused(france(2, |s| buy(s, args::Buy { group: group("defensive_ideas") }))),
            refused(france(0, |s| take_group(s, args::TakeGroup { group: group("flying_ideas") }))),
        ];
        assert_eq!(
            refusals,
            [
                (Status::Rejected, "the player has not taken the idea group trade_ideas".into()),
                (Status::Rejected, "the player has taken the idea group defensive_ideas already".into()),
                (Status::Rejected, "the player has not taken the idea group trade_ideas".into()),
                (Status::Rejected, "every idea of defensive_ideas is bought".into()),
                (Status::NotFound, "no idea group \"flying_ideas\" for the player".into()),
            ]
        );
        assert!(record::posted().is_empty());
    }
}
