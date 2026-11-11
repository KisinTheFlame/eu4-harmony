//! A government's factions: their influence, as the government view's
//! factions panel shows them, and boosting one, the spending its button
//! makes (docs/research/power-spending.md, "The country's other
//! spendings").
use super::content::{self, checks};
use super::{power, thousandths, Ctx, GResult, Session};
use crate::{containers, game_string, memory::Memory, points::Point};
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind},
    government::{args, Faction},
};

/// The spending that boosts a faction: its first value the faction type's
/// index, paid in the monarch points of the faction type's kind.
const BOOST_FACTION: i32 = 23;
/// The country's factions, a list: each entry's faction type, its influence
/// and the next entry.
const FACTIONS: usize = 0x2198;
const ENTRY_TYPE: usize = 0x10;
const ENTRY_INFLUENCE: usize = 0x18;
const ENTRY_NEXT: usize = 0x28;
/// The most influence a faction has, i32 thousandths of a percent: the
/// spend_power command's check refuses a boost there.
const MOST_INFLUENCE: i32 = 100_000;
/// A faction type's content key, a `std::string`, its name's localisation
/// key too; its index, the spending's first value; its kind of monarch
/// points, 0 administrative, 1 diplomatic, 2 military.
const TYPE_KEY: usize = 0x10;
const TYPE_INDEX: usize = 0x238;
const TYPE_POWER: usize = 0x448;
const MOST: usize = 64;

pub fn points() -> Vec<Point> {
    vec![
        // The spend_power command's check for a boost: the faction among
        // the country's, by its type, refused at the most influence.
        Point::keep(
            0x94f710usize,
            "48 8b 8f 98 21 00 00 48 85 c9 0f 84 e3 02 00 00 48 39 41 10 74 0e 48 8b 49 28 48 85 c9 75 f1 e9 cf 02 00 00 81 79 18 a0 86 01 00",
            "faction boost check",
        )
        .uses(&[FACTIONS, ENTRY_TYPE, ENTRY_NEXT, ENTRY_INFLUENCE, MOST_INFLUENCE as usize]),
        // The panel's button builds the spending from the faction's type:
        // its index, then its kind of points.
        Point::keep(0x10c4340usize, "48 8b 42 10 8b 88 38 02 00 00 89 4c 24 38 48 8b 42 10 4c 63 80 48 04 00 00", "faction boost spending")
            .uses(&[TYPE_INDEX, TYPE_POWER]),
        // The panel names the faction by its type's key.
        Point::keep(0x10c3c4busize, "48 8b 86 38 01 00 00 48 8b 50 10 48 83 c2 10", "faction type key").uses(&[TYPE_KEY]),
    ]
}

/// A faction of the player's: its type and its influence.
struct Entry {
    kind: usize,
    influence: i32,
}

fn entries(m: &Memory, country_at: usize) -> GResult<Vec<Entry>> {
    let first = m.ptr(country_at + FACTIONS)?;
    let mut out = Vec::new();
    for at in containers::chain(m, first, ENTRY_NEXT, MOST)? {
        out.push(Entry { kind: m.ptr(at + ENTRY_TYPE)?, influence: m.i32(at + ENTRY_INFLUENCE)? });
    }
    Ok(out)
}

fn power_kind(m: &Memory, kind: usize) -> GResult<PowerKind> {
    match m.i32(kind + TYPE_POWER)? {
        0 => Ok(PowerKind::Adm),
        1 => Ok(PowerKind::Dip),
        2 => Ok(PowerKind::Mil),
        other => Err(format!("a faction of monarch points {other}").into()),
    }
}

/// Whether the player can boost the faction now, as its button lets it,
/// and the cost: the framework words the refusals, which the panel shows
/// none of.
fn choice(m: &Memory, ctx: &Ctx, entry: &Entry) -> GResult<Choice> {
    let skill = power_kind(m, entry.kind)?;
    let cost = power::cost(m, ctx.player, BOOST_FACTION, skill, m.i32(entry.kind + TYPE_INDEX)?)?;
    let price = vec![Amount::new(skill.resource(), cost as f64)];
    if entry.influence >= MOST_INFLUENCE {
        return Ok(Choice::cannot(vec![wording::AT_MOST.to_string()], price));
    }
    let have = power::points_of(m, ctx.country, skill)?;
    if cost > have {
        return Ok(Choice::cannot(vec![wording::too_few(cost, have)], price));
    }
    Ok(Choice::can(price))
}

fn listed(m: &Memory, ctx: &Ctx, entry: &Entry) -> GResult<Faction> {
    let key = game_string::key(m, entry.kind + TYPE_KEY)?;
    Ok(Faction { name: content::name(m, &key)?, faction: key, influence: thousandths(entry.influence), choice: choice(m, ctx, entry)? })
}

/// The player's government's factions, as the panel shows them.
pub fn factions(Session { m, ctx, .. }: Session) -> GResult<Vec<Faction>> {
    entries(m, ctx.country)?.iter().map(|e| listed(m, &ctx, e)).collect()
}

/// Boosts a faction, as its button does.
pub fn boost_faction(Session { m, ctx, commands, .. }: Session, args::BoostFaction { faction }: args::BoostFaction) -> GResult<()> {
    for entry in entries(m, ctx.country)? {
        if game_string::key(m, entry.kind + TYPE_KEY)? != faction {
            continue;
        }
        let doing = format!("boost the faction {faction}");
        super::action::allowed(&choice(m, &ctx, &entry)?, &doing)?;
        let skill = power_kind(m, entry.kind)?;
        return power::spend(commands, ctx.player, BOOST_FACTION, skill, m.i32(entry.kind + TYPE_INDEX)?, &doing);
    }
    Err(checks::not_found("faction", &faction))
}

/// The refusals the framework words itself in the factions panel.
pub mod wording {
    /// A faction at the most influence: the game's check refuses it
    /// without a word.
    pub const AT_MOST: &str = "This faction's influence is at its most.";

    /// Too few points: the button does nothing, without a word.
    pub fn too_few(cost: i32, have: i32) -> String {
        format!("Boosting this faction costs {cost} points; the player has {have}.")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn too_few_points_say_the_cost_and_what_the_player_has() {
        assert_eq!(super::wording::too_few(30, 12), "Boosting this faction costs 30 points; the player has 12.");
    }
}
