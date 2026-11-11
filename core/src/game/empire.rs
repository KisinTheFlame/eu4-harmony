//! The Holy Roman Empire's view: its reforms, each with whether the player
//! can pass it as the game's own check says, and passing and revoking them
//! through the game's own commands; and what the Celestial Empire's view
//! shares with it, an empire's reforms (docs/research/empires.md).
use super::{action, conditions, content, thousandths, Ctx, GResult, Session, Tag};
use crate::{
    containers, game_string,
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::empire::{args, EmpireReforms, ImperialReform};
use eu4_harmony_remote::CountryTag;

/// The Holy Roman Empire and the Celestial Empire, in the world itself.
pub(super) const HRE: usize = 0x15d0;
pub(super) const CELESTIAL: usize = 0x1b40;

/// An empire's fields, the Holy Roman Empire's and the Celestial
/// Empire's alike.
pub(super) mod field {
    /// Its emperor's tag.
    pub const EMPEROR: usize = 0x10;
    /// Its imperial authority, or the Celestial Empire's mandate, i32
    /// thousandths.
    pub const VALUE: usize = 0x18;
    /// The reforms it has passed, in order, a game array of pointers.
    pub const PASSED: usize = 0x20;
    /// Whether this emperor has revoked a reform (the Holy Roman Empire's).
    pub const REVOKED: usize = 0x504;
}

/// The imperial reforms' database: at `+0x10` the reforms in load order, a
/// `std::vector` of pointers.
const REFORMS: usize = 0x242bcd0;
const ORDER: usize = 0x10;
/// An imperial reform's fields.
pub(super) mod reform {
    /// Its key.
    pub const KEY: usize = 0x10;
    /// The content trigger for whether the empire's view offers it.
    pub const POTENTIAL: usize = 0xae8;
    /// The token of the empire it is a reform of (`empire = hre`).
    pub const EMPIRE: usize = 0xbdc;
    /// The part of the view it is in (`gui_container`), a `std::string`.
    pub const CONTAINER: usize = 0xc28;
}
/// The empires' tokens, as a reform names its empire.
pub(super) const HRE_TOKEN: u32 = 0x2749;
pub(super) const CELESTIAL_TOKEN: u32 = 0x35b7;

/// `(reform, out) -> out`: the name the empire's view gives the reform.
const REFORM_NAME: GameFunction<2> = GameFunction::at(0x800300);
/// `(empire, tag, reform, reasons or null) -> bool`: whether the country
/// may pass the reform now, writing the view's reasons why not (the
/// empire's own check, its vtable's `+0x60`): the Holy Roman Empire's.
const MAY_PASS: GameFunction<4> = GameFunction::at(0x706890);
/// `(empire, reasons or null) -> bool`: whether the emperor may revoke the
/// last reform passed now.
const MAY_REVOKE: GameFunction<2> = GameFunction::at(0x7061e0);
/// The least imperial authority a reform is passed at
/// (`IMPERIAL_REFORM_COST`), an i32 of whole points: passing one takes all
/// the authority there is. What revoking one costs, an i32 of thousandths.
const REFORM_COST: usize = 0x233dad8;
const REVOKE_COST: usize = 0x233fb10;
/// How many reforms are read at most.
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // The database's maker, which keeps it in the global.
        Point::keep(0x80144ausize, "48 8b 05 7f a8 c2 01", "imperial reforms database").uses(&[REFORMS]),
        // The empire's reforms walk the database's vector, keeping those
        // of its token.
        Point::keep(0x701baeusize, "48 8b 50 18 4c 8b 40 10", "imperial reforms in order").uses(&[ORDER]),
        Point::keep(0x701be8usize, "39 87 dc 0b 00 00", "reform's empire").uses(&[reform::EMPIRE]),
        // The enact command's check picks the empire by the reform's token.
        Point::keep(0x7f3d5dusize, "41 8b 80 dc 0b 00 00 3d 49 27 00 00", "reform of the Holy Roman Empire"),
        Point::keep(0x7f3d7busize, "3d b7 35 00 00", "reform of the Celestial Empire"),
        Point::keep(0x7f3d72usize, "48 81 c1 d0 15 00 00", "the Holy Roman Empire in the world").uses(&[HRE]),
        Point::keep(0x7f3d89usize, "48 81 c1 40 1b 00 00", "the Celestial Empire in the world").uses(&[CELESTIAL]),
        Point::keep(0x7f3d9busize, "48 8b 51 10", "emperor").uses(&[field::EMPEROR]),
        // The Holy Roman Empire's check reads the passed reforms and the
        // authority, and asks the cost.
        Point::keep(0x706a47usize, "49 63 45 34", "passed reforms count").uses(&[field::PASSED + containers::ARRAY_COUNT]),
        Point::keep(0x706a58usize, "49 8b 45 28", "passed reforms").uses(&[field::PASSED + containers::ARRAY_DATA]),
        Point::keep(0x706f88usize, "45 8b 45 18", "imperial authority").uses(&[field::VALUE]),
        Point::keep(0x706f60usize, "8b 0d 72 6b c3 01", "imperial reform cost").uses(&[REFORM_COST]),
        Point::keep(0x706368usize, "44 8b 05 a1 97 c3 01", "revoke cost").uses(&[REVOKE_COST]),
        Point::keep(0x7062c9usize, "80 b9 04 05 00 00 00", "reform revoked").uses(&[field::REVOKED]),
        // The Celestial Empire's check asks the reform's potential, and the
        // Empire's view sorts reforms by their container.
        Point::keep(0x7135afusize, "48 8d 8b e8 0a 00 00", "reform potential").uses(&[reform::POTENTIAL]),
        Point::keep(0x115ece3usize, "49 8d 8e 28 0c 00 00", "reform container").uses(&[reform::CONTAINER]),
        Point::keep(0x7f3b75usize, "48 8b 5b 50", "enact command's reform"),
        Point::keep(0x7f3b81usize, "48 83 c3 10", "reform key").uses(&[reform::KEY]),
        Point::keep(REFORM_NAME, "48 89 5c 24 08 57 48 83 ec 50 48 8b da 48 8b f9", "reform name"),
        Point::keep(MAY_PASS, "48 89 5c 24 08 4c 89 4c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55", "may pass an imperial reform"),
        Point::keep(MAY_REVOKE, "48 89 5c 24 08 55 48 8b ec 48 81 ec 80 00 00 00 48 8b da", "may revoke an imperial reform"),
    ]
}

/// The reforms of the empire of `token` whose conditions for being offered
/// hold for `player`, in load order.
pub(super) fn offered(m: &Memory, player: Tag, token: u32) -> GResult<Vec<usize>> {
    let database = m.ptr(m.at(REFORMS))?;
    if database == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for reform in containers::pointers(m, database + ORDER, MOST)? {
        if m.u32(reform + reform::EMPIRE)? == token && conditions::holds(m, reform + reform::POTENTIAL, player, 0)? {
            out.push(reform);
        }
    }
    Ok(out)
}

/// The reforms the empire at `empire` has passed.
pub(super) fn passed(m: &Memory, empire: usize) -> GResult<Vec<usize>> {
    Ok(containers::array(m, empire + field::PASSED, MOST)?)
}

/// The empire's emperor, when it has one.
pub(super) fn emperor(m: &Memory, empire: usize) -> GResult<Option<CountryTag>> {
    let tag = Tag(m.u64(empire + field::EMPEROR)?);
    Ok(tag.normal().then(|| CountryTag(tag.letters())))
}

/// A reform as the view lists it, with whether the player may pass it as
/// `may_pass` says, at `cost`.
pub(super) fn listed(m: &Memory, ctx: &Ctx, empire: usize, at: usize, passed: &[usize], may_pass: GameFunction<4>, cost: Vec<Amount>) -> GResult<ImperialReform> {
    let reform = game_string::key(m, at + reform::KEY)?;
    let (_, name) = game_string::written(m, |out| REFORM_NAME.call(m, [at, out]))?;
    let container = game_string::key(m, at + reform::CONTAINER)?;
    let is_passed = passed.contains(&at);
    let choice = if is_passed {
        Choice::cannot(Vec::new(), cost)
    } else {
        let (can, reasons) = content::checked(m, |reasons| Ok(may_pass.call(m, [empire, ctx.player.0 as usize, at, reasons])?))?;
        if can {
            Choice::can(cost)
        } else {
            Choice::cannot(reasons, cost)
        }
    };
    Ok(ImperialReform {
        reform,
        name: crate::unicode::game_text::plain(&name),
        branch: (!container.is_empty()).then_some(container),
        passed: is_passed,
        choice,
    })
}

/// Passes a reform of the empire of `token` the view offers, as its button
/// does, through the game's own command.
pub(super) fn pass(Session { m, ctx, commands, .. }: Session, empire: usize, token: u32, may_pass: GameFunction<4>, key: &str) -> GResult<()> {
    let found = offered(m, ctx.player, token)?.into_iter().map(|at| Ok((game_string::key(m, at + reform::KEY)?, at))).collect::<GResult<Vec<_>>>()?;
    let Some(&(_, at)) = found.iter().find(|(k, _)| k == key) else {
        return Err(content::checks::not_found("reform", key));
    };
    let (can, reasons) = content::checked(m, |reasons| Ok(may_pass.call(m, [empire, ctx.player.0 as usize, at, reasons])?))?;
    if !can || passed(m, empire)?.contains(&at) {
        return Err(action::not_allowed("pass this reform", &reasons));
    }
    action::post(commands, &written_pass(key), "pass this reform")
}

/// The enact command: the reform by its key. The command names no country:
/// the game enacts it as the empire's emperor.
fn written_pass(reform: &str) -> String {
    format!("enact_imperial_reform={{ which=\"{reform}\" }}")
}

/// The revoke command: the emperor's tag.
fn written_revoke(emperor: Tag) -> String {
    format!("revoke_imperial_reform={{ members={{ dummy=\"{}\" }} }}", content::letters(emperor))
}

/// The Empire as its view shows it.
pub fn reforms(Session { m, ctx, .. }: Session) -> GResult<EmpireReforms> {
    let hre = ctx.world + HRE;
    // Passing a reform takes all the authority, which is at least the
    // least it may be passed at.
    let least = m.i32(m.at(REFORM_COST))?.saturating_mul(1000);
    let cost = vec![Amount::new(Resource::ImperialAuthority, thousandths(m.i32(hre + field::VALUE)?.max(least)))];
    let passed = passed(m, hre)?;
    let reforms = offered(m, ctx.player, HRE_TOKEN)?.into_iter().map(|at| listed(m, &ctx, hre, at, &passed, MAY_PASS, cost.clone())).collect::<GResult<_>>()?;
    Ok(EmpireReforms { emperor: emperor(m, hre)?, authority: thousandths(m.i32(hre + field::VALUE)?), reforms, revoke: revoke_choice(m, &ctx)? })
}

/// Whether the player may revoke the last reform passed now.
fn revoke_choice(m: &Memory, ctx: &Ctx) -> GResult<Choice> {
    let hre = ctx.world + HRE;
    let cost = vec![Amount::new(Resource::ImperialAuthority, thousandths(m.i32(m.at(REVOKE_COST))?))];
    if Tag(m.u64(hre + field::EMPEROR)?).index() != ctx.player.index() {
        return Ok(Choice::cannot(vec![checks::NOT_EMPEROR.to_string()], cost));
    }
    let (can, reasons) = content::checked(m, |reasons| Ok(MAY_REVOKE.call(m, [hre, reasons])?))?;
    Ok(if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// Passes one of the Empire's reforms.
pub fn pass_reform(session: Session, args::PassReform { reform }: args::PassReform) -> GResult<()> {
    let hre = session.ctx.world + HRE;
    pass(session, hre, HRE_TOKEN, MAY_PASS, &reform)
}

/// Revokes the last reform the Empire passed.
pub fn revoke_reform(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    let choice = revoke_choice(m, &ctx)?;
    if !choice.can {
        return Err(action::not_allowed("revoke a reform", &choice.reasons));
    }
    action::post(commands, &written_revoke(ctx.player), "revoke a reform")
}

/// The refusals the framework words itself.
pub mod checks {
    /// Revoking is the emperor's alone; the game's check does not say so,
    /// since its button is not shown to another.
    pub const NOT_EMPEROR: &str = "Only the emperor can revoke a reform.";
}

crate::catalogue::bind! {
    eu4_harmony_remote::empire: EmpireRequest => EmpireAnswer;
    Reforms => reforms;
    PassReform => pass_reform;
    RevokeReform => revoke_reform;
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_commands_name_the_reform_and_the_emperor() {
        assert_eq!(super::written_pass("emperor_reichsreform"), r#"enact_imperial_reform={ which="emperor_reichsreform" }"#);
        let hab = super::Tag(u64::from_le_bytes(*b"HAB\0\0\0\0\0"));
        assert_eq!(super::written_revoke(hab), r#"revoke_imperial_reform={ members={ dummy="HAB" } }"#);
    }
}
