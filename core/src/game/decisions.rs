//! Decisions: what the decisions view lists for the player, and whether the
//! player can take each, with the conditions that do not hold as its tooltip
//! lists them (docs/research/triggers.md, "Decisions"); and taking one,
//! through the game's own command (docs/research/government.md,
//! "Decisions").
use super::{action, conditions, content, GResult, Session};
use crate::{game_string, memory::Memory, points::Point, unicode::game_text};
use eu4_harmony_remote::decisions::args;
use eu4_harmony_remote::{choices::Choice, decisions::Decision};

/// The decisions' database: `+0x10` and `+0x18` the decisions in load
/// order, a `std::vector` of pointers.
const DATABASE: usize = 0x242bb18;
const ORDER: usize = 0x10;
/// A decision's fields.
const KEY: usize = 0x10;
/// The content trigger for whether the view lists it.
const POTENTIAL: usize = 0x30;
/// The content trigger for whether the player can take it.
const ALLOW: usize = 0xa8;
const MAJOR: usize = 0x20c;
/// The most decisions read.
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // The database's maker, which keeps it in the global.
        Point::keep(0x488160usize, "48 89 5c 24 10 57 48 83 ec 20 48 8b 05 a7 39 fa 01 48 85 c0", "decisions database").uses(&[DATABASE]),
        // The decisions view walks the vector, asks `potential` then `allow`
        // (each a content trigger's children, at its `+8` and `+0x14`), and
        // sorts out the major ones.
        Point::keep(0xfcb295usize, "e8 c6 ce 4b ff 48 83 c0 10", "decisions view's walk").uses(&[ORDER]),
        Point::keep(0xfcb2f6usize, "49 8b 76 38 49 63 46 44", "decisions view's potential"),
        Point::keep(0xfcb33busize, "49 8b b6 b0 00 00 00 49 63 86 bc 00 00 00", "decisions view's allow"),
        Point::keep(0xfcb376usize, "41 80 be 0c 02 00 00 00", "decisions view's major").uses(&[MAJOR]),
    ]
}

/// The decisions the view lists for the player now, major ones first.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<Decision>> {
    let database = m.ptr(m.at(DATABASE))?;
    if database == 0 {
        return Ok(Vec::new());
    }
    let (begin, end) = (m.ptr(database + ORDER)?, m.ptr(database + ORDER + 8)?);
    let count = super::span(begin, end, 8, MOST)?;
    let mut listed = Vec::new();
    for i in 0..count {
        let decision = m.ptr(begin + i * 8)?;
        if !conditions::holds(m, decision + POTENTIAL, ctx.player, 0)? {
            continue;
        }
        listed.push(read(m, decision, ctx.player)?);
    }
    listed.sort_by_key(|d| !d.major);
    Ok(listed)
}

/// The decide command: the decision by its key and the scope it is taken
/// in, the player's country, with the seed its random effects draw from, as
/// the decisions view writes it.
fn written(country: super::Tag, decision: &str, seed: u32) -> String {
    let country = content::letters(country);
    format!("execute_decision_command={{ decision=\"{decision}\" scope={{ country=\"{country}\" scope_is_valid=yes seed={seed} random={seed} }} }}")
}

/// A seed for a decision's random effects, from the date and its key, as
/// the game's own seeds change from one taking to the next.
fn seed(date: i32, decision: &str) -> u32 {
    decision.bytes().fold(0x811c_9dc5u32 ^ date as u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193)) & 0x7fff_ffff
}

/// Takes a decision the view lists for the player, as its button does.
pub fn take(Session { m, ctx, commands, .. }: Session, args::Take { decision: key }: args::Take) -> GResult<()> {
    let database = m.ptr(m.at(DATABASE))?;
    if database == 0 {
        return Err(content::checks::not_found("decision", &key));
    }
    let (begin, end) = (m.ptr(database + ORDER)?, m.ptr(database + ORDER + 8)?);
    for i in 0..super::span(begin, end, 8, MOST)? {
        let decision = m.ptr(begin + i * 8)?;
        if content::key(m, decision + KEY)? != key || !conditions::holds(m, decision + POTENTIAL, ctx.player, 0)? {
            continue;
        }
        if !conditions::holds(m, decision + ALLOW, ctx.player, 0)? {
            return Err(action::not_allowed("take this decision", &conditions::failing(m, decision + ALLOW, ctx.player, 0)?));
        }
        return action::post(commands, &written(ctx.player, &key, seed(ctx.date, &key)), "take this decision");
    }
    Err(content::checks::not_found("decision", &key))
}

fn read(m: &Memory, decision: usize, player: super::Tag) -> GResult<Decision> {
    let key = game_text::to_utf8(&game_string::read(m, decision + KEY)?);
    let title = std::ffi::CString::new(format!("{key}_title")).map_err(|e| e.to_string())?;
    let name = game_text::plain(&game_string::localized(m, title.as_ptr() as usize)?);
    let choice = if conditions::holds(m, decision + ALLOW, player, 0)? {
        Choice::can(Vec::new())
    } else {
        Choice::cannot(conditions::failing(m, decision + ALLOW, player, 0)?, Vec::new())
    };
    Ok(Decision { decision: key, name, major: m.u8(decision + MAJOR)? != 0, choice })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_decision_and_the_player_s_scope() {
        let fra = super::super::Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(
            super::written(fra, "move_capital", 17),
            r#"execute_decision_command={ decision="move_capital" scope={ country="FRA" scope_is_valid=yes seed=17 random=17 } }"#
        );
        assert_ne!(super::seed(100, "move_capital"), super::seed(101, "move_capital"));
    }
}
