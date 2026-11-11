//! The Celestial Empire's view: its reforms and its emperor's decrees, each
//! with whether the player can pass or enact it as the game's own check
//! says, and passing and enacting them through the game's own commands
//! (docs/research/empires.md).
use super::commands::{write, Block};
use super::empire::{self, CELESTIAL, CELESTIAL_TOKEN};
use super::{action, conditions, content, thousandths, GResult, Session};
use crate::{
    containers, game_string,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::celestial_empire::{args, CelestialDecrees, CelestialReforms, Decree};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::Date;

/// `(empire, tag, reform, reasons or null) -> bool`: whether the country
/// may pass the reform now, writing the view's reasons why not: the
/// Celestial Empire's check (its vtable's `+0x60`).
const MAY_PASS: GameFunction<4> = GameFunction::at(0x7130e0);
/// What a reform takes: the least mandate it may be passed at, the mandate
/// it costs and the stability it costs, i32 thousandths.
const LEAST_MANDATE: usize = 0x233fb2c;
const MANDATE_COST: usize = 0x233fb30;
const STABILITY_COST: usize = 0x233fb08;

/// The decree in force and the day it was enacted, in the Celestial
/// Empire; it ends its duration later.
const IN_FORCE: usize = 0x68;
const ENACTED: usize = 0x70;
/// The game's dates count hours.
const HOURS_A_DAY: i32 = 24;
/// The decrees' database, an `std::unordered_map` from key to decree,
/// whose list is at `+0x08`; a node's decree is at `+0x30`.
const DECREES: usize = 0x242bb08;
const LIST: usize = 0x8;
const NODE_DECREE: usize = 0x30;
/// A decree's fields.
mod decree {
    /// Its key.
    pub const KEY: usize = 0x10;
    /// Its cost in meritocracy, i32 thousandths.
    pub const COST: usize = 0x34;
    /// How many days it is in force (`duration`).
    pub const DURATION: usize = 0x38;
    /// The content trigger for whether the view offers it.
    pub const POTENTIAL: usize = 0x260;
}
/// `(decree, tag, reasons or null) -> bool`: whether the country may enact
/// the decree now, writing the view's reasons why not.
const MAY_ENACT: GameFunction<3> = GameFunction::at(0x478c60);
/// How many decrees are read at most.
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x7135e3usize, "8b 05 43 c5 c2 01", "least mandate for a reform").uses(&[LEAST_MANDATE]),
        Point::keep(0x7135efusize, "8b 3d 3b c5 c2 01", "mandate a reform costs").uses(&[MANDATE_COST]),
        Point::keep(0x713760usize, "8b 1d a2 c3 c2 01", "stability a reform costs").uses(&[STABILITY_COST]),
        Point::keep(MAY_PASS, "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 10 fe ff ff", "may pass a celestial reform"),
        // The decree check reads the decree in force and its end, and the
        // decree's potential and cost.
        Point::keep(0x478d5eusize, "4c 8b af a8 1b 00 00", "decree in force").uses(&[CELESTIAL + IN_FORCE]),
        // The decree in force ends its duration in days, times 24 hours,
        // after the day it was enacted.
        Point::keep(0x478d77usize, "41 8b 45 38 8d 14 40", "decree duration").uses(&[decree::DURATION]),
        Point::keep(0x478d7eusize, "8b 87 b0 1b 00 00", "decree enacted").uses(&[CELESTIAL + ENACTED]),
        Point::keep(0x478faeusize, "49 8b 9f 68 02 00 00", "decree potential").uses(&[decree::POTENTIAL + 8]),
        Point::keep(0x4794b9usize, "41 8b 7f 34", "decree cost").uses(&[decree::COST]),
        Point::keep(0x7f3e5cusize, "48 8b 5b 50", "enact command's decree"),
        Point::keep(0x7f3e68usize, "48 83 c3 10", "decree key").uses(&[decree::KEY]),
        Point::keep(0x47977ausize, "48 8b 05 87 23 fb 01", "decrees database").uses(&[DECREES]),
        Point::keep(MAY_ENACT, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55 41 54 41 55 41 56 41 57", "may enact a decree"),
    ]
}

/// What a reform costs, as the view says.
fn reform_cost(m: &crate::memory::Memory) -> GResult<Vec<Amount>> {
    Ok(vec![
        Amount::new(Resource::Mandate, thousandths(m.i32(m.at(MANDATE_COST))?)),
        Amount::new(Resource::Stability, thousandths(m.i32(m.at(STABILITY_COST))?)),
    ])
}

/// The Celestial Empire as its view shows it: its reforms.
pub fn reforms(Session { m, ctx, .. }: Session) -> GResult<CelestialReforms> {
    let celestial = ctx.world + CELESTIAL;
    let cost = reform_cost(m)?;
    let passed = empire::passed(m, celestial)?;
    let reforms =
        empire::offered(m, ctx.player, CELESTIAL_TOKEN)?.into_iter().map(|at| empire::listed(m, &ctx, celestial, at, &passed, MAY_PASS, cost.clone())).collect::<GResult<_>>()?;
    Ok(CelestialReforms { emperor: empire::emperor(m, celestial)?, mandate: thousandths(m.i32(celestial + empire::field::VALUE)?), reforms })
}

/// Passes one of the Celestial Empire's reforms.
pub fn pass_reform(session: Session, args::PassReform { reform }: args::PassReform) -> GResult<()> {
    let celestial = session.ctx.world + CELESTIAL;
    empire::pass(session, celestial, CELESTIAL_TOKEN, MAY_PASS, &reform)
}

/// The decrees in the database, in its order.
fn all_decrees(m: &crate::memory::Memory) -> GResult<Vec<usize>> {
    let database = m.ptr(m.at(DECREES))?;
    if database == 0 {
        return Ok(Vec::new());
    }
    containers::linked(m, m.ptr(database + LIST)?, MOST)?.into_iter().map(|node| Ok(m.ptr(node + NODE_DECREE)?)).collect()
}

/// The decrees the view offers the player, with their keys.
fn offered_decrees(m: &crate::memory::Memory, player: super::Tag) -> GResult<Vec<(String, usize)>> {
    let mut out = Vec::new();
    for at in all_decrees(m)? {
        if at != 0 && conditions::holds(m, at + decree::POTENTIAL, player, 0)? {
            out.push((game_string::key(m, at + decree::KEY)?, at));
        }
    }
    Ok(out)
}

/// The emperor's decrees as the view lists them.
pub fn decrees(Session { m, ctx, .. }: Session) -> GResult<CelestialDecrees> {
    let celestial = ctx.world + CELESTIAL;
    let in_force = m.ptr(celestial + IN_FORCE)?;
    let (in_force, until) = if in_force == 0 { (None, None) } else { (Some(game_string::key(m, in_force + decree::KEY)?), Some(Date(m.i32(celestial + ENACTED)? + m.i32(in_force + decree::DURATION)? * HOURS_A_DAY))) };
    let mut decrees = Vec::new();
    for (key, at) in offered_decrees(m, ctx.player)? {
        let cost = vec![Amount::new(Resource::Meritocracy, thousandths(m.i32(at + decree::COST)?))];
        let (can, reasons) = content::checked(m, |reasons| Ok(MAY_ENACT.call(m, [at, ctx.player.0 as usize, reasons])?))?;
        let name = content::name(m, &key)?;
        decrees.push(Decree { decree: key, name, choice: if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) } });
    }
    Ok(CelestialDecrees { in_force, until, decrees })
}

/// The enact command: the decree by its key. The command names no country:
/// the game enacts it as the Celestial Emperor.
fn written_enact(decree: &str) -> String {
    write("enact_decree", Block::new().quoted("decree_name", decree))
}

/// Enacts one of the emperor's decrees.
pub fn enact_decree(Session { m, ctx, commands, .. }: Session, args::EnactDecree { decree: key }: args::EnactDecree) -> GResult<()> {
    let Some((_, at)) = offered_decrees(m, ctx.player)?.into_iter().find(|(k, _)| *k == key) else {
        return Err(content::checks::not_found("decree", &key));
    };
    let (can, reasons) = content::checked(m, |reasons| Ok(MAY_ENACT.call(m, [at, ctx.player.0 as usize, reasons])?))?;
    if !can {
        return Err(action::not_allowed("enact this decree", &reasons));
    }
    action::post(commands, &written_enact(&key), "enact this decree")
}

crate::catalogue::bind! {
    eu4_harmony_remote::celestial_empire: CelestialEmpireRequest => CelestialEmpireAnswer;
    Reforms => reforms;
    PassReform => pass_reform;
    Decrees => decrees;
    EnactDecree => enact_decree;
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_decree() {
        assert_eq!(super::written_enact("expand_bureaucracy_decree"), r#"enact_decree={ decree_name="expand_bureaucracy_decree" }"#);
    }
}
