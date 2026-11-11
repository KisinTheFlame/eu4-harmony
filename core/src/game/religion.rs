//! The religion view: following a personal deity or a cult, through the
//! game's own commands, with the view's own checks (docs/research/religion.md).
use super::{action, conditions, content, GResult, Session, Tag};
use crate::objects::campaign::country;
use crate::{
    containers, game_string,
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::religion::{args, ReligionOption};
use eu4_harmony_remote::choices::Choice;

/// The personal deities' database, made on first use by its getter
/// (`0x7e6910`): the deities at `+0x10`, a `std::vector` of pointers, in file
/// order.
const DEITIES: usize = 0x242bca8;
const DEITY_LIST: usize = 0x10;
/// A deity: its key, its conditions for being offered (`potential`) and for
/// being followed (`trigger`). A condition's reader is its part at
/// [`READER`], which the deity's reader hands the block.
const DEITY_KEY: usize = 0x10;
const DEITY_POTENTIAL: usize = 0x278;
const DEITY_TRIGGER: usize = 0x2f0;
const READER: usize = 0x38;
/// The game's no-deity, which a country follows while it follows none, and
/// which the deities' database holds among the deities.
const NO_DEITY: usize = 0x233fe90;
/// The cults: a `std::vector` of pointers of the game's own, its first and
/// its end, in file order, and the game's no-cult object (`nocult`), which a
/// country that uses cults follows while it has none, and which the vector
/// holds among the cults.
const CULTS: usize = 0x2328a78;
const NO_CULT: usize = 0x245b190;
/// A cult: its key and its conditions for being embraced (`allow`).
const CULT_KEY: usize = 0x10;
const CULT_ALLOW: usize = 0x200;
/// `(religion) -> bool`: whether the religion has personal deities, and
/// cults (and the expansion that brings them).
const HAS_DEITIES: GameFunction<1> = GameFunction::at(0xa7eda0);
const HAS_CULTS: GameFunction<1> = GameFunction::at(0xa7ee00);
/// `(country, reasons or null) -> bool`: whether the country may follow
/// another deity now (not during a regency), and embrace a cult now (not
/// during a regency, and only while it follows none), writing the regency's
/// line into the `std::string`.
const MAY_CHANGE_DEITY: GameFunction<2> = GameFunction::at(0x3d6e70);
const MAY_CHANGE_CULT: GameFunction<2> = GameFunction::at(0x3d7850);

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x7e691ausize, "48 8b 05 87 53 c4 01 48 85 c0", "personal deities database").uses(&[DEITIES]),
        Point::keep(0x7e6961usize, "48 89 7b 10 48 89 7b 18 48 89 7b 20", "personal deities' vector").uses(&[DEITY_LIST]),
        Point::keep(0xaae096usize, "48 83 c3 10", "personal deity key").uses(&[DEITY_KEY]),
        Point::keep(0x7e5cd0usize, "48 81 c1 b0 02 00 00", "personal deity potential").uses(&[DEITY_POTENTIAL + READER]),
        Point::keep(0x7e5c39usize, "48 81 c1 28 03 00 00", "personal deity trigger").uses(&[DEITY_TRIGGER + READER]),
        Point::keep(0x7e5766usize, "48 89 1d 23 a7 b5 01", "no deity").uses(&[NO_DEITY]),
        Point::keep(0x7faf40usize, "48 8d 05 31 db b2 01", "cults").uses(&[CULTS]),
        Point::keep(0x1b5ec79usize, "48 3b 1d 00 9e 7c 00", "cults' end").uses(&[CULTS + 8]),
        Point::keep(0x7fae40usize, "48 8d 05 49 03 c6 01", "no cult").uses(&[NO_CULT]),
        Point::keep(0xaae5a6usize, "48 83 c3 10", "cult key").uses(&[CULT_KEY]),
        Point::keep(0x7fad78usize, "48 81 c1 38 02 00 00", "cult allow").uses(&[CULT_ALLOW + READER]),
        Point::keep(HAS_DEITIES, "48 83 ec 28 80 b9 17 01 00 00 00 74 4a", "religion has deities"),
        Point::keep(HAS_CULTS, "48 83 ec 28 80 b9 18 01 00 00 00 74 4a", "religion has cults"),
        Point::keep(MAY_CHANGE_DEITY, "40 53 48 83 ec 40 48 8b da 48 8b 81 80 12 00 00 48 85 c0 74 55 48 83 78 78 00", "may change deity"),
        Point::keep(MAY_CHANGE_CULT, "40 53 48 83 ec 40 48 8b da 48 8b 81 80 12 00 00 48 85 c0 74 55 48 83 78 78 00", "may change cult"),
        // The deity command's execute, and the cult check: the country's
        // deity and cult.
        Point::keep(0x3d6c12usize, "48 8b 89 a0 0f 00 00", "country's personal deity").uses(&[country::PERSONAL_DEITY]),
        Point::keep(0x3d78bausize, "48 8b 99 a8 0f 00 00", "country's cult").uses(&[country::CULT]),
    ]
}

/// The refusals the framework words itself.
pub mod checks {
    use crate::game::{Fail, Status};

    /// Of a deity or a cult when the player's religion has none.
    pub fn not_of_religion(what: &str) -> Fail {
        Fail::new(Status::Rejected, format!("the player's religion has no {what}"))
    }

    /// The line for a cult not embraced while one is followed, which the
    /// game's check says without words.
    pub const CULT_FOLLOWED: &str = "A cult is followed until the monarch changes.";
}

/// The deities, the game's no-deity left out.
fn deities(m: &Memory) -> GResult<Vec<usize>> {
    let none = m.ptr(m.at(NO_DEITY))?;
    match m.ptr(m.at(DEITIES))? {
        0 => Ok(Vec::new()),
        database => Ok(containers::pointers(m, database + DEITY_LIST, containers::MOST)?.into_iter().filter(|&d| d != none).collect()),
    }
}

/// The cults, the game's no-cult left out.
fn cults(m: &Memory) -> GResult<Vec<usize>> {
    let none = m.at(NO_CULT);
    Ok(containers::pointers(m, m.at(CULTS), containers::MOST)?.into_iter().filter(|&c| c != none).collect())
}

fn religion(m: &Memory, country: usize) -> GResult<usize> {
    Ok(m.ptr(country + country::STATE_RELIGION)?)
}

fn has(m: &Memory, function: GameFunction<1>, country: usize) -> GResult<bool> {
    match religion(m, country)? {
        0 => Ok(false),
        religion => Ok(function.call(m, [religion])? & 0xff != 0),
    }
}

/// Whether the player may follow `deity` now, and why not: the regency, then
/// the deity's conditions that do not hold.
fn deity_choice(m: &Memory, country: usize, player: Tag, deity: usize) -> GResult<Choice> {
    let (may, mut reasons) = content::checked(m, |t| Ok(MAY_CHANGE_DEITY.call(m, [country, t])?))?;
    let allowed = conditions::holds(m, deity + DEITY_TRIGGER, player, 0)?;
    if !allowed {
        reasons.extend(conditions::failing(m, deity + DEITY_TRIGGER, player, 0)?);
    }
    Ok(if may && allowed { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

/// Whether the player may embrace `cult` now, and why not: the regency or a
/// cult followed already, then the cult's conditions that do not hold.
fn cult_choice(m: &Memory, country: usize, player: Tag, cult: usize) -> GResult<Choice> {
    let (may, mut reasons) = content::checked(m, |t| Ok(MAY_CHANGE_CULT.call(m, [country, t])?))?;
    if !may && reasons.is_empty() {
        reasons.push(checks::CULT_FOLLOWED.to_string());
    }
    let allowed = conditions::holds(m, cult + CULT_ALLOW, player, 0)?;
    if !allowed {
        reasons.extend(conditions::failing(m, cult + CULT_ALLOW, player, 0)?);
    }
    Ok(if may && allowed { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) })
}

fn option(m: &Memory, action: &str, key: String, current: bool, choice: Choice) -> GResult<ReligionOption> {
    Ok(ReligionOption { action: action.into(), name: content::name(m, &key)?, key, current, choice })
}

/// The deities and cults the religion view offers the player.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Vec<ReligionOption>> {
    let mut out = Vec::new();
    if has(m, HAS_DEITIES, ctx.country)? {
        let current = m.ptr(ctx.country + country::PERSONAL_DEITY)?;
        for deity in deities(m)? {
            if deity != current && !conditions::holds(m, deity + DEITY_POTENTIAL, ctx.player, 0)? {
                continue;
            }
            let key = game_string::key(m, deity + DEITY_KEY)?;
            out.push(option(m, "choose_deity", key, deity == current, deity_choice(m, ctx.country, ctx.player, deity)?)?);
        }
    }
    if has(m, HAS_CULTS, ctx.country)? {
        let current = m.ptr(ctx.country + country::CULT)?;
        for cult in cults(m)? {
            let key = game_string::key(m, cult + CULT_KEY)?;
            out.push(option(m, "choose_cult", key, cult == current, cult_choice(m, ctx.country, ctx.player, cult)?)?);
        }
    }
    Ok(out)
}

fn find(m: &Memory, list: Vec<usize>, key_at: usize, kind: &str, key: &str) -> GResult<usize> {
    for item in list {
        if game_string::key(m, item + key_at)? == key {
            return Ok(item);
        }
    }
    Err(content::checks::not_found(kind, key))
}

/// The commands the view's buttons post: the deity or the cult by its key
/// and the country.
fn deity_text(country: Tag, deity: &str) -> String {
    format!("personal_deity={{ personal_deity=\"{deity}\" country=\"{}\" }}", content::letters(country))
}

fn cult_text(country: Tag, cult: &str) -> String {
    format!("fetishist_cult={{ country=\"{}\" fetishist_cult=\"{cult}\" }}", content::letters(country))
}

/// Follows a personal deity.
pub fn choose_deity(Session { m, ctx, commands, .. }: Session, args::ChooseDeity { deity: key }: args::ChooseDeity) -> GResult<()> {
    if !has(m, HAS_DEITIES, ctx.country)? {
        return Err(checks::not_of_religion("personal deities"));
    }
    let deity = find(m, deities(m)?, DEITY_KEY, "personal deity", &key)?;
    if !conditions::holds(m, deity + DEITY_POTENTIAL, ctx.player, 0)? {
        return Err(content::checks::not_found("personal deity", &key));
    }
    action::carry_out(commands, &deity_choice(m, ctx.country, ctx.player, deity)?, "follow this deity", || Ok(deity_text(ctx.player, &key)))
}

/// Embraces a cult.
pub fn choose_cult(Session { m, ctx, commands, .. }: Session, args::ChooseCult { cult: key }: args::ChooseCult) -> GResult<()> {
    if !has(m, HAS_CULTS, ctx.country)? {
        return Err(checks::not_of_religion("cults"));
    }
    let cult = find(m, cults(m)?, CULT_KEY, "cult", &key)?;
    action::carry_out(commands, &cult_choice(m, ctx.country, ctx.player, cult)?, "embrace this cult", || Ok(cult_text(ctx.player, &key)))
}

crate::catalogue::bind! {
    eu4_harmony_remote::religion: ReligionRequest => ReligionAnswer;
    Options => options;
    ChooseDeity => choose_deity;
    ChooseCult => choose_cult;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        buffers::{self, Image, Object},
        commands::record,
        As, Caller, Fail, Status,
    };

    fn tag(letters: &[u8; 3]) -> Tag {
        let mut bytes = [0u8; 8];
        bytes[..3].copy_from_slice(letters);
        Tag(u64::from_le_bytes(bytes))
    }

    #[test]
    fn the_commands_name_the_deity_or_the_cult_and_the_country() {
        assert_eq!(deity_text(tag(b"VIJ"), "shiva"), r#"personal_deity={ personal_deity="shiva" country="VIJ" }"#);
        assert_eq!(cult_text(tag(b"KON"), "yemoja_cult"), r#"fetishist_cult={ country="KON" fetishist_cult="yemoja_cult" }"#);
    }

    /// A country whose state religion is none: neither deities nor cults are
    /// offered, and choosing one is refused before anything is posted.
    #[test]
    fn a_religion_without_them_offers_none() {
        let image = Image::new();
        let fra = tag(b"FRA");
        let country = Object::country(fra, country::CULT + 8);
        let countries = Object::countries(&[&country]);
        let session = || buffers::session(image.memory(), 0, fra, country.at(), countries.at(), Caller::Remote(As::Player));
        assert!(options(session()).unwrap().is_empty());
        let Err(Fail { status, message }) = choose_deity(session(), args::ChooseDeity { deity: "shiva".into() }) else { panic!("a deity was followed") };
        assert_eq!((status, message.as_str()), (Status::Rejected, "the player's religion has no personal deities"));
        let Err(Fail { status, .. }) = choose_cult(session(), args::ChooseCult { cult: "yemoja_cult".into() }) else { panic!("a cult was embraced") };
        assert_eq!(status, Status::Rejected);
        assert!(record::posted().is_empty());
        drop((country, countries, image));
    }
}
