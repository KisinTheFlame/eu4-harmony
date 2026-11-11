//! Decisions: what the decisions view lists for the player, and whether the
//! player can take each, with the conditions that do not hold as its tooltip
//! lists them (docs/research/triggers.md, "Decisions"); and taking one,
//! through the game's own command (docs/research/government.md,
//! "Decisions").
use super::commands::{write, Block};
use super::{action, conditions, content, GResult, Session};
use crate::containers;
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
    let count = containers::span(begin, end, 8, MOST)?;
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
    let scope = Block::new().tag("country", country.letters()).word("scope_is_valid", "yes").word("seed", seed).word("random", seed);
    write("execute_decision_command", Block::new().quoted("decision", decision).block("scope", scope))
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
    for i in 0..containers::span(begin, end, 8, MOST)? {
        let decision = m.ptr(begin + i * 8)?;
        if game_string::key(m, decision + KEY)? != key || !conditions::holds(m, decision + POTENTIAL, ctx.player, 0)? {
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
    let key = game_string::key(m, decision + KEY)?;
    let title = std::ffi::CString::new(format!("{key}_title")).map_err(|e| e.to_string())?;
    let name = game_text::plain(&game_string::localized(m, title.as_ptr() as usize)?);
    let choice = if conditions::holds(m, decision + ALLOW, player, 0)? {
        Choice::can(Vec::new())
    } else {
        Choice::cannot(conditions::failing(m, decision + ALLOW, player, 0)?, Vec::new())
    };
    Ok(Decision { decision: key, name, major: m.u8(decision + MAJOR)? != 0, choice })
}

crate::catalogue::bind! {
    eu4_harmony_remote::decisions: DecisionRequest => DecisionAnswer;
    List => list;
    Take => take;
}

#[cfg(test)]
mod tests {
    use crate::game::As;
    #[test]
    fn the_command_names_the_decision_and_the_player_s_scope() {
        let fra = super::super::Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(
            super::written(fra, "move_capital", 17),
            r#"execute_decision_command={ decision="move_capital" scope={ country="FRA" scope_is_valid=yes seed=17 random=17 } }"#
        );
        assert_ne!(super::seed(100, "move_capital"), super::seed(101, "move_capital"));
    }

    use super::*;
    use crate::catalogue::fake::tur;
    use crate::game::{
        buffers::{self, Image, Object},
        commands::record,
        Caller, Fail, Session, Status,
    };

    /// The decisions' database of `decisions`, each a key, whether its
    /// `potential` holds, whether its `allow` holds and whether it is major;
    /// the Ottomans' session of it.
    struct Decisions {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    fn decisions(decisions: &[(&str, bool, bool, bool)]) -> Decisions {
        let image = Image::new();
        image.conditions().localisation();
        let made: Vec<Object> = decisions
            .iter()
            .map(|&(key, potential, allow, major)| {
                Object::new(MAJOR + 8).string(KEY, key).u8(POTENTIAL, potential.into()).u8(ALLOW, allow.into()).u8(MAJOR, major.into())
            })
            .collect();
        let database = Object::new(ORDER + 16).vector(ORDER, &made.iter().map(|d| d.at() as u64).collect::<Vec<_>>());
        image.u64(DATABASE, database.at() as u64);
        let player = Object::country(tur(), 0);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), 0, tur(), player.at(), countries.at(), Caller::Remote(As::Player));
        let mut objects = made;
        objects.extend([database, player, countries]);
        Decisions { _image: image, _objects: objects, session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    const SOME: [(&str, bool, bool, bool); 3] = [("move_capital", true, true, false), ("hidden_one", false, true, false), ("form_rome", true, false, true)];

    #[test]
    fn the_view_lists_the_potential_decisions_major_first_with_what_does_not_hold() {
        let listed = list(decisions(&SOME).session).unwrap();
        let summary: Vec<_> = listed.iter().map(|d| (d.decision.as_str(), d.name.as_str(), d.major, d.choice.can)).collect();
        assert_eq!(summary, [("form_rome", "form_rome_title", true, false), ("move_capital", "move_capital_title", false, true)]);
        assert_eq!(listed[0].choice.reasons, ["unmet"]);
    }

    #[test]
    fn a_decision_is_taken_by_the_view_s_command() {
        take(decisions(&SOME).session, args::Take { decision: "move_capital".into() }).unwrap();
        let seed = super::seed(0, "move_capital");
        assert_eq!(
            record::posted(),
            [format!(r#"execute_decision_command={{ decision="move_capital" scope={{ country="TUR" scope_is_valid=yes seed={seed} random={seed} }} }}"#)]
        );
    }

    #[test]
    fn a_decision_not_listed_unknown_or_not_allowed_is_refused() {
        let decisions = decisions(&SOME);
        assert_eq!(refused(take(decisions.session, args::Take { decision: "form_rome".into() })), (Status::Rejected, "unmet".into()));
        assert_eq!(refused(take(decisions.session, args::Take { decision: "hidden_one".into() })).0, Status::NotFound);
        assert_eq!(refused(take(decisions.session, args::Take { decision: "no_such".into() })).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }
}
