//! Cultures: the player's primary culture and the cultures it accepts, as
//! the government view's cultures window lists them, and its select,
//! promote and demote buttons, each a use of diplomatic power the spend_power
//! command carries (docs/research/power-spending.md, "Cultures").
use super::{action, commands, content::checks, power, thousandths, world, Ctx, GResult, Session, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{country, province},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::cultures::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    cultures::{CultureAction, CultureActionKind, CultureEntry, Cultures, Slots},
};

/// The game's cultures, made on first use: `() -> database`, whose vector of
/// cultures by their index runs from [`ALL`] to [`ALL_END`].
const DATABASE: GameFunction<0> = GameFunction::at(0x27b4d0);
const ALL: usize = 0x28;
const ALL_END: usize = 0x30;
/// A culture's fields: its key's token (as a province's culture has it,
/// [`province::CULTURE_TOKEN`]), its name in the player language, a
/// `std::string`, and its index in the database, which a spending carries.
mod culture {
    pub const NAME: usize = 0x68;
    pub const INDEX: usize = 0x90;
}
/// A country's cultures: the cultures it has promoted to accepted, a list of
/// nodes holding the culture at `+0` and the next node at
/// [`ACCEPTED_NEXT`]; and how many accepted culture slots they take, i32.
const ACCEPTED: usize = 0x1230;
const ACCEPTED_NEXT: usize = 0x10;
const SLOTS_USED: usize = 0x1240;
/// `(country, culture) -> bool`: whether the country accepts the culture:
/// its primary culture, one of the primary culture's group the game accepts
/// with it, or one it promoted.
const ACCEPTS: GameFunction<2> = GameFunction::at(0x2fc3e0);
/// `(country, AI's extra slot) -> i32`: how many accepted culture slots the
/// country has; the window passes 0.
const SLOTS_MAX: GameFunction<2> = GameFunction::at(0x2faaf0);
/// The buttons' checks, which write the tooltip's reasons, the points among
/// them, and answer whether the country may: `(country, culture, text or
/// null)` to make it the primary culture and to demote it, `(country,
/// culture, text or null, AI's extra slot)` to promote it, which the window
/// passes 0. Each refuses without a reason a culture its button is not
/// shown for.
const MAY_SET_PRIMARY: GameFunction<3> = GameFunction::at(0x2fa1f0);
const MAY_PROMOTE: GameFunction<4> = GameFunction::at(0x2fae60);
const MAY_DEMOTE: GameFunction<3> = GameFunction::at(0x2fb700);

/// The spendings' kinds, as the game names them (`0x94e1d0`):
/// `set_primary_culture`, `add_accepted_culture` and
/// `remove_accepted_culture`, all paid in diplomatic points.
const SET_PRIMARY: i32 = 32;
const PROMOTE: i32 = 33;
const DEMOTE: i32 = 34;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(DATABASE, "48 89 5c 24 10 57 48 83 ec 20 48 8b 05 b7 05 1b 02", "cultures"),
        Point::keep(ACCEPTS, "48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9 e8 1b 04 00 00", "accepts culture"),
        Point::keep(SLOTS_MAX, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec b0 00 00 00", "accepted culture slots"),
        Point::keep(MAY_SET_PRIMARY, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 a0 fe ff ff 48 81 ec 60 02 00 00", "may set the primary culture"),
        Point::keep(MAY_PROMOTE, "44 88 4c 24 20 4c 89 44 24 18 48 89 54 24 10 55 53 56 57 41 54 41 55 41 56 41 57", "may promote a culture"),
        Point::keep(MAY_DEMOTE, "48 89 5c 24 18 48 89 74 24 20 55 57 41 54 41 56 41 57 48 8d ac 24 40 ff ff ff", "may demote a culture"),
        // The spend_power check finds the culture by the spending's first
        // value in the cultures' vector.
        Point::keep(0x94f345usize, "e8 86 c1 92 ff 8b 4e 18 48 8b 40 28", "culture by index").uses(&[ALL]),
        Point::keep(0x94074ausize, "48 8b 58 30 48 2b 58 28", "cultures' count").uses(&[ALL, ALL_END]),
        // Whether the country accepts a culture walks its promoted ones.
        Point::keep(0x2fc414usize, "48 8b 87 30 12 00 00 48 85 c0 74 11 48 39 18 74 09 48 8b 40 10", "accepted cultures").uses(&[ACCEPTED, ACCEPTED_NEXT]),
        // The promote check compares the slots taken with the country's.
        Point::keep(0x2fb2c9usize, "0f b6 cb 41 8b 95 40 12 00 00", "accepted culture slots taken").uses(&[SLOTS_USED]),
        // The window's buttons build their spendings from the culture's
        // index: select, promote and the demote button's enabling.
        Point::keep(0xf2fd2cusize, "c7 45 80 20 00 00 00 c7 45 84 01 00 00 00", "set primary culture's spending"),
        Point::keep(0xf3005cusize, "c7 45 80 21 00 00 00 c7 45 84 01 00 00 00", "promote culture's spending"),
        Point::keep(0xf2ed86usize, "c7 45 d0 22 00 00 00 c7 45 d4 01 00 00 00", "demote culture's spending"),
        Point::keep(0xf2fd54usize, "8b 82 90 00 00 00", "culture's index").uses(&[culture::INDEX]),
        // The demote tooltip names the culture by its name.
        Point::keep(0xf2fc74usize, "49 83 c1 68", "culture's name").uses(&[culture::NAME]),
    ]
}

/// The cultures of the game, by their index.
fn all(m: &Memory) -> GResult<Vec<usize>> {
    let database = DATABASE.call(m, [])?;
    let (first, end) = (m.ptr(database + ALL)?, m.ptr(database + ALL_END)?);
    let count = crate::containers::span(first, end, 8, 10_000)?;
    (0..count).map(|i| Ok(m.ptr(first + 8 * i)?)).collect()
}

/// A culture's content key.
fn key(m: &Memory, culture: usize) -> GResult<String> {
    let token = m.u32(culture + province::CULTURE_TOKEN)?;
    commands::type_name(m, token).ok_or_else(|| format!("culture token {token:#x} has no name").into())
}

/// The culture whose content key is `name`.
fn named(m: &Memory, name: &str) -> GResult<usize> {
    for culture in all(m)? {
        if key(m, culture).is_ok_and(|k| k == name) {
            return Ok(culture);
        }
    }
    Err(checks::not_found("culture", name))
}

/// The cultures the country at `country` has promoted, in its list's order.
fn promoted(m: &Memory, country: usize) -> GResult<Vec<usize>> {
    let mut out = Vec::new();
    let mut node = m.ptr(country + ACCEPTED)?;
    while node != 0 {
        out.push(m.ptr(node)?);
        if out.len() > 10_000 {
            return Err("the accepted cultures' list does not end".to_string().into());
        }
        node = m.ptr(node + ACCEPTED_NEXT)?;
    }
    Ok(out)
}

fn accepts(m: &Memory, country: usize, culture: usize) -> GResult<bool> {
    Ok(ACCEPTS.call(m, [country, culture])? & 0xff != 0)
}

/// What a button's spending for `culture` costs the player, in diplomatic
/// points.
fn cost(m: &Memory, player: Tag, kind: i32, culture: usize) -> GResult<Vec<Amount>> {
    let points = power::Spending::new(m, kind, PowerKind::Dip, m.i32(culture + culture::INDEX)?)?.cost(m, player)?;
    Ok(vec![Amount::new(Resource::Dip, f64::from(points))])
}

/// Whether the player can press the button `action` for `culture` now, with
/// the game's reasons and its cost.
fn choice(m: &Memory, ctx: &Ctx, culture: usize, action: CultureActionKind) -> GResult<Choice> {
    let (kind, country) = (spending(action), ctx.country);
    let cost = cost(m, ctx.player, kind, culture)?;
    let (yes, text) = game_string::written(m, |t| match action {
        CultureActionKind::SetPrimary => MAY_SET_PRIMARY.call(m, [country, culture, t]),
        CultureActionKind::Promote => MAY_PROMOTE.call(m, [country, culture, t, 0]),
        CultureActionKind::Demote => MAY_DEMOTE.call(m, [country, culture, t]),
    })?;
    if yes & 0xff != 0 {
        return Ok(Choice::can(cost));
    }
    let mut reasons = super::reasons(&text);
    // The primary culture's check ends with the line telling the price
    // (`SET_PRIMARY_CULTURE_COST`) whether it allows or not: not a reason.
    if action == CultureActionKind::SetPrimary {
        reasons.pop();
    }
    Ok(Choice::cannot(reasons, cost))
}

fn spending(action: CultureActionKind) -> i32 {
    match action {
        CultureActionKind::SetPrimary => SET_PRIMARY,
        CultureActionKind::Promote => PROMOTE,
        CultureActionKind::Demote => DEMOTE,
    }
}

/// The buttons the window shows for a culture: select for every one but the
/// primary culture, promote for one not accepted, demote for an accepted one.
fn buttons(primary: bool, accepted: bool) -> Vec<CultureActionKind> {
    let mut out = Vec::new();
    if !primary {
        out.push(CultureActionKind::SetPrimary);
        out.push(if accepted { CultureActionKind::Demote } else { CultureActionKind::Promote });
    }
    out
}

/// The development of each culture among the player's provinces, in
/// thousandths, by the culture's address.
fn development(m: &Memory, ctx: &Ctx) -> GResult<Vec<(usize, i64)>> {
    let mut out: Vec<(usize, i64)> = Vec::new();
    for at in world::provinces_owned_by(m, ctx.world, ctx.player)? {
        let culture = m.ptr(at + province::CULTURE)?;
        if culture == 0 {
            continue;
        }
        let dev = [province::BASE_TAX, province::BASE_PRODUCTION, province::BASE_MANPOWER]
            .into_iter()
            .map(|field| m.i32(at + field).map(i64::from))
            .sum::<Result<i64, _>>()?;
        match out.iter_mut().find(|(c, _)| *c == culture) {
            Some((_, total)) => *total += dev,
            None => out.push((culture, dev)),
        }
    }
    Ok(out)
}

/// The player's cultures and what the cultures window offers.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Cultures> {
    let primary = m.ptr(ctx.country + country::PRIMARY_CULTURE)?;
    let promoted = promoted(m, ctx.country)?;
    let mut developed = development(m, &ctx)?;
    developed.sort_by_key(|&(_, dev)| std::cmp::Reverse(dev));
    // The primary culture, the promoted ones, then the rest by development.
    let mut listed: Vec<usize> = Vec::new();
    for culture in std::iter::once(primary).chain(promoted.iter().copied()).chain(developed.iter().map(|(c, _)| *c)) {
        if culture != 0 && !listed.contains(&culture) {
            listed.push(culture);
        }
    }
    let cultures = listed
        .into_iter()
        .map(|culture| {
            let is_primary = culture == primary;
            let accepted = accepts(m, ctx.country, culture)?;
            let actions = buttons(is_primary, accepted)
                .into_iter()
                .map(|action| Ok(CultureAction { action, choice: choice(m, &ctx, culture, action)? }))
                .collect::<GResult<Vec<_>>>()?;
            let dev = developed.iter().find(|(c, _)| *c == culture).map_or(0, |(_, d)| *d);
            Ok(CultureEntry {
                culture: key(m, culture)?,
                name: game_string::shown(m, culture + culture::NAME)?,
                development: thousandths(dev as i32),
                primary: is_primary,
                accepted,
                actions,
            })
        })
        .collect::<GResult<Vec<_>>>()?;
    Ok(Cultures {
        primary: if primary == 0 { None } else { Some(key(m, primary)?) },
        accepted: promoted.iter().map(|&c| key(m, c)).collect::<GResult<_>>()?,
        slots: Slots { used: m.i32(ctx.country + SLOTS_USED)?, max: SLOTS_MAX.call(m, [ctx.country, 0])? as i32 },
        cultures,
    })
}

/// The spend_power command a button posts for the culture of index `index`.
fn written(tag: Tag, action: CultureActionKind, index: i32) -> String {
    power::written(tag.letters(), spending(action), PowerKind::Dip, index)
}

/// Presses the button `action` for the culture named `name`, as the window
/// does once the player confirms.
fn press(Session { m, ctx, commands, .. }: Session, name: &str, action: CultureActionKind) -> GResult<()> {
    let culture = named(m, name)?;
    let primary = culture == m.ptr(ctx.country + country::PRIMARY_CULTURE)?;
    let accepted = accepts(m, ctx.country, culture)?;
    if !buttons(primary, accepted).contains(&action) {
        return Err(checks::not_now(refusal(name, action, primary)));
    }
    let choice = choice(m, &ctx, culture, action)?;
    let index = m.i32(culture + culture::INDEX)?;
    action::carry_out(commands, &choice, doing(action), || Ok(written(ctx.player, action, index)))
}

fn doing(action: CultureActionKind) -> &'static str {
    match action {
        CultureActionKind::SetPrimary => "make this culture the primary one",
        CultureActionKind::Promote => "promote this culture",
        CultureActionKind::Demote => "demote this culture",
    }
}

/// Why the window shows no such button for the culture: the framework's
/// own words, the game giving none.
fn refusal(name: &str, action: CultureActionKind, primary: bool) -> String {
    match (action, primary) {
        (_, true) => format!("{name} is the player's primary culture"),
        (CultureActionKind::Promote, false) => format!("the player accepts {name} already"),
        _ => format!("the player does not accept {name}"),
    }
}

pub fn set_primary(session: Session, args::SetPrimary { culture }: args::SetPrimary) -> GResult<()> {
    press(session, &culture, CultureActionKind::SetPrimary)
}

pub fn promote(session: Session, args::Promote { culture }: args::Promote) -> GResult<()> {
    press(session, &culture, CultureActionKind::Promote)
}

pub fn demote(session: Session, args::Demote { culture }: args::Demote) -> GResult<()> {
    press(session, &culture, CultureActionKind::Demote)
}

crate::catalogue::bind! {
    eu4_harmony_remote::cultures: CultureRequest => CultureAnswer;
    Options => options;
    SetPrimary => set_primary;
    Promote => promote;
    Demote => demote;
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRA: Tag = Tag(0x0100_0042_0041_5246);

    #[test]
    fn each_button_posts_its_spending_with_the_culture_s_index() {
        assert_eq!(written(FRA, CultureActionKind::SetPrimary, 12), r#"spend_power_command={ country="FRA" power={ data=12 category=32 skill=1 } }"#);
        assert_eq!(written(FRA, CultureActionKind::Promote, 7), r#"spend_power_command={ country="FRA" power={ data=7 category=33 skill=1 } }"#);
        assert_eq!(written(FRA, CultureActionKind::Demote, 0), r#"spend_power_command={ country="FRA" power={ category=34 skill=1 } }"#);
    }

    #[test]
    fn the_window_shows_promote_or_demote_and_select_but_for_the_primary_culture() {
        assert_eq!(buttons(true, true), Vec::<CultureActionKind>::new());
        assert_eq!(buttons(false, true), [CultureActionKind::SetPrimary, CultureActionKind::Demote]);
        assert_eq!(buttons(false, false), [CultureActionKind::SetPrimary, CultureActionKind::Promote]);
    }

    #[test]
    fn a_button_the_window_does_not_show_is_refused_in_the_framework_s_words() {
        assert_eq!(refusal("castilian", CultureActionKind::Demote, true), "castilian is the player's primary culture");
        assert_eq!(refusal("castilian", CultureActionKind::Promote, false), "the player accepts castilian already");
        assert_eq!(refusal("castilian", CultureActionKind::Demote, false), "the player does not accept castilian");
    }
}
