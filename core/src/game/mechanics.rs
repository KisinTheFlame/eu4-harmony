//! Government mechanics: the powers and interactions a government's reforms
//! give it, as the government view shows them, and using an interaction
//! through the game's own command (docs/research/government.md,
//! "Government mechanics"). Part of the government group.
use super::commands::{write, Block};
use super::content::checks;
use super::government::GOVERNMENT;
use super::{action, conditions, content, modifiers, thousandths, Ctx, GResult, Session, Tag};
use crate::{
    containers, game_string,
    memory::Memory,
    objects::campaign::modifier_set,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::government::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    countries::ModifierValue,
    government::{Mechanic, MechanicInteraction, MechanicPower},
    Date,
};

/// The government's mechanics, a `std::vector` of their states: each the
/// mechanic's definition and the country it is of.
const STATES: usize = 0x20;
mod state {
    pub const DEFINITION: usize = 0x10;
}
/// A mechanic's definition: its token and key, its powers and its
/// interactions, the game's arrays of pointers to them.
mod definition {
    pub const TOKEN: usize = 0x8;
    pub const KEY: usize = 0x10;
    pub const POWERS: usize = 0xc0;
    pub const INTERACTIONS: usize = 0xd8;
}
/// A power's definition: its token and key, its bounds in thousandths, and
/// its scaled modifiers and its reverse scaled ones, the game's arrays of
/// [`ENTRY`]-byte entries; a range modifier is a scaled one marked so.
mod power {
    pub const TOKEN: usize = 0x8;
    pub const KEY: usize = 0x10;
    pub const MIN: usize = 0x58;
    pub const MAX: usize = 0x5c;
    pub const SCALED: usize = 0x70;
    pub const REVERSE: usize = 0x88;
}
/// A scaled modifier: its conditions, a content trigger at its start; its
/// modifiers, a set of (modifier, thousandths) pairs; where it starts and
/// ends applying, in thousandths of the power; whether it applies whole
/// outside them; whether it is a range modifier.
mod entry {
    pub const STRIDE: usize = 0x280;
    pub const MODIFIERS: usize = 0x78;
    pub const PAIRS: usize = 0x58;
    pub const PAIR_COUNT: usize = 0x64;
    pub const START: usize = 0x270;
    pub const END: usize = 0x274;
    pub const EXTEND: usize = 0x278;
    pub const RANGE: usize = 0x279;
}
/// An interaction: its token and key, the power its cost is paid in (its
/// token, 0 for none) and the cost in thousandths, its conditions for being
/// used and for being offered, and the token its cooldown is kept under (0:
/// its own).
mod interaction {
    pub const TOKEN: usize = 0x8;
    pub const KEY: usize = 0x30;
    pub const COST_POWER: usize = 0x94;
    pub const COST: usize = 0x98;
    pub const TRIGGER: usize = 0xa0;
    pub const POTENTIAL: usize = 0x108;
    pub const COOLDOWN: usize = 0x1ac;
}
/// The country's cooldowns, a `std::vector` of (token, date) pairs.
const COOLDOWNS: usize = 0xeb8;
/// `(state, power token)`: the state's entry of the power, its value in
/// thousandths at [`VALUE`]; null when it has none.
const POWER: GameFunction<2> = GameFunction::at(0x7bc0c0);
const VALUE: usize = 0x20;
/// `(interaction, state, reasons or null) -> bool`: whether the country may
/// use the interaction now, writing the government view's reasons why not:
/// its conditions for being offered, its cost, its cooldown, its trigger.
const MAY_USE: GameFunction<3> = GameFunction::at(0x7b6ab0);
/// `(interaction, out, country) -> out` and `(power, out, country) -> out`:
/// the name the government view gives it.
const INTERACTION_NAME: GameFunction<3> = GameFunction::at(0x7b5df0);
const POWER_NAME: GameFunction<3> = GameFunction::at(0x7b4b00);
/// How many mechanics, powers, entries and pairs are read at most.
const MOST: usize = 1_000;

pub fn points() -> Vec<Point> {
    vec![
        // The interaction command's check finds the mechanic's state among
        // the government's and the interaction among the definition's.
        Point::keep(0x2bfdadusize, "48 8b 88 20 14 00 00 4c 8b 41 28 48 8b 51 20", "government mechanic states").uses(&[GOVERNMENT, STATES]),
        Point::keep(0x2bfdc5usize, "4c 8b 12 41 8b 41 58 49 8b 4a 10 39 41 08", "mechanic definition token").uses(&[state::DEFINITION, definition::TOKEN]),
        Point::keep(0x2bfddbusize, "48 8b 81 e0 00 00 00 48 63 89 ec 00 00 00", "mechanic interactions")
            .uses(&[definition::INTERACTIONS + containers::ARRAY_DATA, definition::INTERACTIONS + containers::ARRAY_COUNT]),
        Point::keep(0x7bbab0usize, "49 8b 46 10 48 8b 88 c8 00 00 00 48 63 80 d4 00 00 00", "mechanic powers")
            .uses(&[definition::POWERS + containers::ARRAY_DATA, definition::POWERS + containers::ARRAY_COUNT]),
        // The interaction's constructor copies the mechanic's key and its own.
        Point::keep(0x7b5930usize, "49 8d 50 10", "mechanic key").uses(&[definition::KEY]),
        Point::keep(0x7b599fusize, "48 8d 4f 30", "interaction key").uses(&[interaction::KEY]),
        // The interaction check: its offer, cooldown, cost and trigger.
        Point::keep(0x7b6afausize, "48 8d 8b 08 01 00 00", "interaction potential").uses(&[interaction::POTENTIAL]),
        Point::keep(0x7b6b34usize, "44 8b 83 ac 01 00 00", "interaction cooldown token").uses(&[interaction::COOLDOWN]),
        Point::keep(0x7b6b6busize, "4c 8b 89 c0 0e 00 00 48 8b 89 b8 0e 00 00", "country cooldowns").uses(&[COOLDOWNS]),
        Point::keep(0x7b6e8eusize, "48 8b 83 a0 00 00 00", "interaction trigger").uses(&[interaction::TRIGGER]),
        Point::keep(0x7b6f7ausize, "8b 91 94 00 00 00", "interaction cost power").uses(&[interaction::COST_POWER]),
        Point::keep(0x7b6f9cusize, "8b 58 20 3b 9e 98 00 00 00", "power value and interaction cost").uses(&[VALUE, interaction::COST]),
        // The power's reader: its bounds; its constructor: its key.
        Point::keep(0x7b3f8cusize, "48 8d 51 58", "power min").uses(&[power::MIN]),
        Point::keep(0x7b3f77usize, "48 8d 51 5c", "power max").uses(&[power::MAX]),
        Point::keep(0x7b79b9usize, "48 8d 4f 10 e8", "power key").uses(&[power::KEY]),
        // The powers' scaled modifiers, as the mechanic adds them up.
        Point::keep(0x7b9bb4usize, "48 8b 77 78 48 89 74 24 28 48 63 87 84 00 00 00", "power scaled modifiers")
            .uses(&[power::SCALED + containers::ARRAY_DATA, power::SCALED + containers::ARRAY_COUNT]),
        Point::keep(0x7ba2c5usize, "4c 8b b7 90 00 00 00 48 63 87 9c 00 00 00", "power reverse scaled modifiers")
            .uses(&[power::REVERSE + containers::ARRAY_DATA, power::REVERSE + containers::ARRAY_COUNT]),
        Point::keep(0x7b9c2busize, "4c 8d 6e 78", "scaled modifier set").uses(&[entry::MODIFIERS]),
        Point::keep(0x7b9d93usize, "41 39 7d 64", "scaled modifier pair count").uses(&[entry::PAIR_COUNT]),
        Point::keep(0x7b9db0usize, "49 8b 7d 58", "scaled modifier pairs").uses(&[entry::PAIRS]),
        Point::keep(0x7b9e87usize, "80 be 79 02 00 00 00", "range modifier").uses(&[entry::RANGE]),
        Point::keep(0x7b9e94usize, "8b 96 74 02 00 00 44 8b 86 70 02 00 00", "scaled modifier span").uses(&[entry::START, entry::END]),
        Point::keep(0x7ba01dusize, "0f b6 86 78 02 00 00", "scaled modifier beyond its span").uses(&[entry::EXTEND]),
        Point::keep(POWER, "89 54 24 10 53 48 83 ec 20 44 8b d2 4c 8d 4c 24 30 48 8b d9", "mechanic power"),
        Point::keep(MAY_USE, "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8d ac 24 20 fe", "may use an interaction"),
        Point::keep(INTERACTION_NAME, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 48 8d ac 24 b0 fc ff ff", "interaction name"),
        Point::keep(POWER_NAME, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 48 8d ac 24 b0 fc ff ff", "power name"),
    ]
}

/// The game's array of [`entry::STRIDE`]-byte entries at `array`.
fn entries(m: &Memory, array: usize) -> GResult<Vec<usize>> {
    let data = m.ptr(array + containers::ARRAY_DATA)?;
    let count = containers::bounded(m.i32(array + containers::ARRAY_COUNT)?, MOST)?;
    Ok((0..count).map(|i| data + i * entry::STRIDE).collect())
}

/// The states of the player's government's mechanics.
fn states(m: &Memory, country: usize) -> GResult<Vec<usize>> {
    let government = m.ptr(country + GOVERNMENT)?;
    if government == 0 {
        return Ok(Vec::new());
    }
    Ok(containers::pointers(m, government + STATES, MOST)?)
}

/// How much of its modifiers a scaled modifier gives at `value`, in
/// thousandths, as the game works it out (`0x7b9b50`): a range modifier all
/// within its span and none outside; a scaled one in proportion within its
/// span, none below it and above it all or none as it extends beyond it; a
/// reverse scaled one the rest of that, all below its span or none.
fn factor(value: i32, start: i32, end: i32, extend: bool, range: bool, reverse: bool) -> i64 {
    if range && !reverse {
        return if (start..=end).contains(&value) { 1000 } else { 0 };
    }
    let share = if end == start {
        if value == end {
            1000
        } else {
            0
        }
    } else {
        (value as i64 - start as i64) * 1000 / (end as i64 - start as i64)
    };
    if reverse {
        let share = if share > 1000 {
            1000
        } else if share < 0 {
            if extend {
                0
            } else {
                1000
            }
        } else {
            share
        };
        1000 - share
    } else if share < 0 {
        0
    } else if share > 1000 {
        if extend {
            1000
        } else {
            0
        }
    } else {
        share
    }
}

/// What a power adds to the country's modifiers now, each modifier once, in
/// the order the power's script first names it.
fn effects(m: &Memory, player: Tag, definition: usize, value: i32) -> GResult<Vec<ModifierValue>> {
    let mut sums: Vec<(i32, i64)> = Vec::new();
    for (list, reverse) in [(power::SCALED, false), (power::REVERSE, true)] {
        for at in entries(m, definition + list)? {
            if !conditions::holds(m, at, player, 0)? {
                continue;
            }
            let share =
                factor(value, m.i32(at + entry::START)?, m.i32(at + entry::END)?, m.u8(at + entry::EXTEND)? != 0, m.u8(at + entry::RANGE)? != 0, reverse);
            let set = at + entry::MODIFIERS;
            let pairs = m.ptr(set + entry::PAIRS)?;
            for i in 0..containers::bounded(m.i32(set + entry::PAIR_COUNT)?, MOST)? {
                let (modifier, amount) = (m.i32(pairs + i * 8)?, m.i32(pairs + i * 8 + 4)?);
                if modifier == modifier_set::NONE {
                    continue;
                }
                let added = amount as i64 * share / 1000;
                match sums.iter_mut().find(|(k, _)| *k == modifier) {
                    Some((_, sum)) => *sum += added,
                    None => sums.push((modifier, added)),
                }
            }
        }
    }
    sums.into_iter()
        .filter(|(_, sum)| *sum != 0)
        .map(|(modifier, sum)| Ok(ModifierValue { modifier: modifiers::key(m, modifier)?, value: thousandths(sum as i32) }))
        .collect()
}

/// A name the government view gives, written by `name`.
fn named(m: &Memory, name: GameFunction<3>, object: usize, country: usize) -> GResult<String> {
    let (_, text) = game_string::written(m, |out| name.call(m, [object, out, country]))?;
    Ok(crate::unicode::game_text::plain(&text))
}

/// The day the interaction's cooldown ends, while one runs.
fn ready_on(m: &Memory, country: usize, today: i32, interaction: usize) -> GResult<Option<Date>> {
    let token = match m.u32(interaction + self::interaction::COOLDOWN)? {
        0 => m.u32(interaction + self::interaction::TOKEN)?,
        token => token,
    };
    for pair in containers::entries(m, country + COOLDOWNS, 8, containers::MOST)? {
        let date = m.i32(pair + 4)?;
        if m.u32(pair)? == token && date > today {
            return Ok(Some(Date(date)));
        }
    }
    Ok(None)
}

/// The key of the power the definition `definition` has with the token
/// `token`.
fn power_key(m: &Memory, definition: usize, token: u32) -> GResult<Option<String>> {
    for power in containers::array(m, definition + self::definition::POWERS, MOST)? {
        if m.u32(power + self::power::TOKEN)? == token {
            return Ok(Some(game_string::key(m, power + self::power::KEY)?));
        }
    }
    Ok(None)
}

/// An interaction the mechanic offers, with whether the player may use it.
fn offered(m: &Memory, ctx: &Ctx, state: usize, definition: usize, at: usize) -> GResult<MechanicInteraction> {
    let (can, reasons) = content::checked(m, |reasons| Ok(MAY_USE.call(m, [at, state, reasons])?))?;
    let token = m.u32(at + interaction::COST_POWER)?;
    let cost_power = if token == 0 { None } else { power_key(m, definition, token)? };
    let cost = if token == 0 { Vec::new() } else { vec![Amount::new(Resource::MechanicPower, thousandths(m.i32(at + interaction::COST)?))] };
    Ok(MechanicInteraction {
        interaction: game_string::key(m, at + interaction::KEY)?,
        name: named(m, INTERACTION_NAME, at, ctx.country)?,
        choice: if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) },
        cost_power,
        ready_on: ready_on(m, ctx.country, ctx.date, at)?,
    })
}

/// The interactions the mechanic of `definition` offers the player: those
/// whose conditions for being offered hold.
fn interactions(m: &Memory, player: Tag, definition: usize) -> GResult<Vec<usize>> {
    let mut offered = Vec::new();
    for at in containers::array(m, definition + self::definition::INTERACTIONS, MOST)? {
        if conditions::holds(m, at + interaction::POTENTIAL, player, 0)? {
            offered.push(at);
        }
    }
    Ok(offered)
}

/// The player's government mechanics, as the government view shows them.
pub fn mechanics(Session { m, ctx, .. }: Session) -> GResult<Vec<Mechanic>> {
    let mut listed = Vec::new();
    for state in states(m, ctx.country)? {
        let definition = m.ptr(state + state::DEFINITION)?;
        let key = game_string::key(m, definition + self::definition::KEY)?;
        let mut powers = Vec::new();
        for power in containers::array(m, definition + self::definition::POWERS, MOST)? {
            let entry = POWER.call(m, [state, m.u32(power + self::power::TOKEN)? as usize])?;
            let value = if entry == 0 { 0 } else { m.i32(entry + VALUE)? };
            powers.push(MechanicPower {
                power: game_string::key(m, power + self::power::KEY)?,
                name: named(m, POWER_NAME, power, ctx.country)?,
                value: thousandths(value),
                min: thousandths(m.i32(power + self::power::MIN)?),
                max: thousandths(m.i32(power + self::power::MAX)?),
                effects: effects(m, ctx.player, power, value)?,
            });
        }
        let interactions = interactions(m, ctx.player, definition)?.into_iter().map(|at| offered(m, &ctx, state, definition, at)).collect::<GResult<_>>()?;
        // The government view names a mechanic by `ability_` and its key
        // (`0x7a4a29`).
        listed.push(Mechanic { name: content::name(m, &format!("ability_{key}"))?, mechanic: key, powers, interactions });
    }
    Ok(listed)
}

/// The interaction command, the mechanic and the interaction by their keys.
fn written(country: Tag, mechanic: &str, interaction: &str) -> String {
    write("government_interaction_command", Block::new().tag("country", country.letters()).word("parent", mechanic).word("index", interaction))
}

/// Uses an interaction of one of the player's government mechanics.
pub fn interact(Session { m, ctx, commands, .. }: Session, args::Interact { mechanic, interaction: wanted }: args::Interact) -> GResult<()> {
    let mut found = None;
    for state in states(m, ctx.country)? {
        let definition = m.ptr(state + state::DEFINITION)?;
        if game_string::key(m, definition + self::definition::KEY)? == mechanic {
            found = Some((state, definition));
        }
    }
    let Some((state, definition)) = found else {
        return Err(checks::not_found("government mechanic", &mechanic));
    };
    for at in interactions(m, ctx.player, definition)? {
        if game_string::key(m, at + interaction::KEY)? != wanted {
            continue;
        }
        let choice = offered(m, &ctx, state, definition, at)?.choice;
        let doing = format!("use the interaction {wanted}");
        return action::carry_out(commands, &choice, &doing, || Ok(written(ctx.player, &mechanic, &wanted)));
    }
    Err(checks::not_found("mechanic interaction", &wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mam() -> Tag {
        Tag(u64::from_le_bytes(*b"MAM\0\0\0\0\0"))
    }

    #[test]
    fn the_interaction_command_names_the_mechanic_and_the_interaction_by_key() {
        assert_eq!(
            written(mam(), "mamluk_mechanic", "recruit_from_culture_lands"),
            r#"government_interaction_command={ country="MAM" parent=mamluk_mechanic index=recruit_from_culture_lands }"#
        );
    }

    #[test]
    fn a_scaled_modifier_grows_with_the_power_within_its_span() {
        // OMA's arabic trade influence at 1.4 of 100: 1.4% of its modifiers.
        assert_eq!(factor(1400, 0, 100_000, true, false, false), 14);
        assert_eq!(factor(-5, 0, 100_000, true, false, false), 0);
        assert_eq!(factor(150_000, 0, 100_000, true, false, false), 1000);
        assert_eq!(factor(150_000, 0, 100_000, false, false, false), 0);
        assert_eq!(factor(7, 7, 7, true, false, false), 1000);
        assert_eq!(factor(6, 7, 7, true, false, false), 0);
    }

    #[test]
    fn a_range_modifier_applies_whole_within_its_span_alone() {
        assert_eq!(factor(50_000, 0, 100_000, true, true, false), 1000);
        assert_eq!(factor(100_001, 0, 100_000, true, true, false), 0);
        assert_eq!(factor(-1, 0, 100_000, true, true, false), 0);
    }

    #[test]
    fn a_reverse_scaled_modifier_gives_the_rest() {
        // Persia's asha vahishta below 0: the reverse modifier from -100 to 0.
        assert_eq!(factor(-100_000, -100_000, 0, true, false, true), 1000);
        assert_eq!(factor(-50_000, -100_000, 0, true, false, true), 500);
        assert_eq!(factor(10_000, -100_000, 0, true, false, true), 0);
        assert_eq!(factor(-200_000, -100_000, 0, true, false, true), 1000);
        assert_eq!(factor(-200_000, -100_000, 0, false, false, true), 0);
    }
}
