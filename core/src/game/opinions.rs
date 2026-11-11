//! What one country thinks of another, as the diplomacy view shows it: the
//! opinion and its parts, the aggressive expansion it holds, its trust and
//! the favors it owes (docs/research/diplomacy.md, "Opinion").
use super::{countries, country, player::Eyes, thousandths, world, GResult, Tag, View, World, WorldSession};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{country as fields, country_diplomacy, opinion_modifier},
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::{
    countries::{args, Opinion, OpinionOf, OpinionPart},
    CountryTag,
};

/// A country's opinion of another as the game's panels show it, the one it
/// last counted: (country, the other's tag, a `std::string` for its tooltip
/// or 0), an i16.
const OPINION: GameFunction<3> = GameFunction::at(0x383860);
/// The opinion counted again with its parts written, one a line, each its
/// text, `: ` and its value: (country, the other's tag, the yearly change's
/// i32, the `std::string`, 0, 0), an i16.
const PARTS: GameFunction<6> = GameFunction::at(0x3d32b0);

/// The opinion modifier aggressive expansion is, by its key.
const AGGRESSIVE_EXPANSION: &str = "aggressive_expansion";
/// The most opinion modifiers one country is believed to have toward
/// another.
const MOST: i32 = 4096;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(OPINION, "48 89 54 24 10 53 56 48 83 ec 28 48 8b da 48 8b f1 4d 85 c0", "opinion"),
        Point::keep(PARTS, "48 89 5c 24 20 4c 89 44 24 18 48 89 54 24 10 55 56 57 41 54", "opinion parts"),
    ]
}

/// What `of` thinks of `toward`. In the player's view the trust and the
/// favors are left out unless one of the two is the country acted as.
pub fn opinion(WorldSession { m, world: ctx, view, .. }: WorldSession, args::Opinion { of, toward }: args::Opinion) -> GResult<Opinion> {
    let (from, to) = (countries::named(m, &ctx, of)?, countries::named(m, &ctx, toward)?);
    if let Some(refusal) = eu4_harmony_checks::countries::of_itself(&of.to_string(), &toward.to_string()) {
        return Err(refusal.into());
    }
    let c = country(m, &ctx, from)?;
    let stance = stance(m, &ctx, view, c, from, to)?;
    Ok(Opinion {
        of,
        toward,
        opinion: stance.opinion,
        parts: parts(m, c, to)?,
        aggressive_expansion: stance.aggressive_expansion,
        trust: stance.trust,
        favors: stance.favors,
    })
}

/// What every other country that owns a province thinks of `toward`.
pub fn opinions(WorldSession { m, world: ctx, view, .. }: WorldSession, args::Opinions { toward }: args::Opinions) -> GResult<Vec<OpinionOf>> {
    let to = countries::named(m, &ctx, toward)?;
    let mut out = Vec::new();
    for from in world::countries(m, &ctx)? {
        if from.index() == to.index() {
            continue;
        }
        let c = country(m, &ctx, from)?;
        if m.i32(c + fields::OWNED_COUNT)? <= 0 {
            continue;
        }
        let stance = stance(m, &ctx, view, c, from, to)?;
        out.push(OpinionOf {
            country: CountryTag(from.letters()),
            opinion: stance.opinion,
            aggressive_expansion: stance.aggressive_expansion,
            trust: stance.trust,
            favors: stance.favors,
        });
    }
    Ok(out)
}

/// What one country's record of another holds of the two, and its opinion.
struct Stance {
    opinion: i32,
    aggressive_expansion: f64,
    trust: Option<i32>,
    favors: Option<f64>,
}

/// The country at `c`, `from`'s, of `to`.
fn stance(m: &Memory, ctx: &World, view: View, c: usize, from: Tag, to: Tag) -> GResult<Stance> {
    let eyes = Eyes::of(view, ctx.player);
    let shown = eyes.sees_all_of(from) || eyes.sees_all_of(to);
    let record = m.ptr(c + fields::RELATIONS)? + to.index() as usize * fields::RELATION_SIZE;
    Ok(Stance {
        opinion: OPINION.call(m, [c, to.0 as usize, 0])? as u16 as i16 as i32,
        aggressive_expansion: thousandths(held(m, record, AGGRESSIVE_EXPANSION)?.saturating_neg()),
        trust: if shown { Some(m.u8(record + fields::RELATION_TRUST)? as i32) } else { None },
        favors: if shown { Some(thousandths(m.i32(record + fields::RELATION_FAVORS)?)) } else { None },
    })
}

/// What the opinion modifiers of the type `key` add up to, in thousandths,
/// among those of the relation's record at `record`: the record's diplomacy
/// keeps them by the country at the record's [`fields::RELATION_OTHER`], as
/// the record's writer in saves finds them.
fn held(m: &Memory, record: usize, key: &str) -> GResult<i32> {
    let diplomacy = m.ptr(record)?;
    let other = Tag(m.u64(record + fields::RELATION_OTHER)?);
    let items = m.i32(diplomacy + country_diplomacy::OPINIONS_ITEMS)?;
    if !(0..=32767).contains(&items) {
        return Err(format!("{items} countries' opinion modifiers").into());
    }
    if other.index() < 0 || other.index() as i32 >= items {
        return Ok(0);
    }
    let item = m.ptr(diplomacy + country_diplomacy::OPINIONS)? + other.index() as usize * country_diplomacy::OPINIONS_SIZE;
    let count = m.i32(item + country_diplomacy::OPINIONS_COUNT)?;
    if !(0..=MOST).contains(&count) {
        return Err(format!("{count} opinion modifiers toward one country").into());
    }
    let list = if count > 0 { m.ptr(item + country_diplomacy::OPINIONS_LIST)? } else { 0 };
    let mut sum = 0i32;
    for i in 0..count as usize {
        let modifier = m.ptr(list + i * 8)?;
        if game_string::key(m, m.ptr(modifier + opinion_modifier::TYPE)? + opinion_modifier::TYPE_KEY)? == key {
            sum = sum.saturating_add(m.i32(modifier + opinion_modifier::VALUE)?);
        }
    }
    Ok(sum)
}

/// The parts of the opinion of the country at `c` of `to`, as its tooltip
/// lists them.
fn parts(m: &Memory, c: usize, to: Tag) -> GResult<Vec<OpinionPart>> {
    let mut yearly = 0i32;
    let at = &mut yearly as *mut i32 as usize;
    let (_, text) = game_string::written(m, |out| PARTS.call(m, [c, to.0 as usize, at, out, 0, 0]))?;
    lines(&game_text::plain(&text))
}

/// The parts the tooltip's lines name, each its text, `: `, its value and,
/// for a part that changes with time, how much a year in brackets
/// (`改善关系: +91 (-3.0 每年)`); a line with no value says more of the part
/// above it, and is kept in its text.
fn lines(text: &str) -> GResult<Vec<OpinionPart>> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let part = line
            .rmatch_indices(": ")
            .find_map(|(at, sep)| valued(&line[at + sep.len()..]).map(|(value, yearly)| OpinionPart { text: line[..at].trim().to_string(), value, yearly }));
        match (part, out.last_mut()) {
            (Some(part), _) => out.push(part),
            // A line of its own under a part says more of it, such as the
            // provinces another country wants.
            (None, Some(last)) => {
                last.text.push('\n');
                last.text.push_str(line);
            }
            (None, None) => return Err(format!("an opinion's part without a value: {line:?}").into()),
        }
    }
    Ok(out)
}

/// A part's value and its change a year, from what follows its text: a
/// whole number with its sign, then perhaps the change in brackets.
fn valued(rest: &str) -> Option<(i32, Option<f64>)> {
    let rest = rest.trim();
    let (number, after) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let value = number.trim_start_matches('+').parse::<i32>().ok()?;
    let after = after.trim();
    if after.is_empty() {
        return Some((value, None));
    }
    let inside = after.strip_prefix('(')?.strip_suffix(')')?;
    let yearly = inside.split_whitespace().next()?.trim_start_matches('+').parse::<f64>().ok()?;
    Some((value, Some(yearly)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tooltip_s_lines_are_its_parts() {
        let parts = lines("Opinion of same Religion: +10\n侵略扩张: -25 (+2.5 每年)\n\n  Note: rival: -50  \n(they want Kruje)\n").unwrap();
        let as_tuples: Vec<_> = parts.iter().map(|p| (p.text.as_str(), p.value, p.yearly)).collect();
        assert_eq!(as_tuples, vec![("Opinion of same Religion", 10, None), ("侵略扩张", -25, Some(2.5)), ("Note: rival\n(they want Kruje)", -50, None)]);
    }

    #[test]
    fn a_line_with_no_value_is_refused() {
        assert!(lines("(they want Kruje)").is_err());
        assert!(lines("Opinion: ten").is_err());
        assert!(lines("Opinion: +10 (soon)").is_err());
    }
}
