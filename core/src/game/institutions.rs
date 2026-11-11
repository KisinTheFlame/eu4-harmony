//! Institutions: those that have appeared in the world, as the institutions
//! view lists them, and embracing one through the game's own command
//! (docs/research/institutions-ages-monuments.md, "Institutions").
use super::content::{self, checks};
use super::provinces::{i32s, INSTITUTIONS};
use super::{action, thousandths, GResult, Session, Tag};
use crate::containers;
use crate::game_string;
use crate::{
    memory::Memory,
    objects::campaign::{country, institution, world},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::institutions::{args, InstitutionOption};

/// `(country, institution, reasons, with the cost) -> bool`: whether the
/// country may embrace the institution, writing why not into the
/// `std::string` reasons, as the institutions view and the command's check
/// ask it.
const CAN_EMBRACE: GameFunction<4> = GameFunction::at(0x3f6340);
/// `(country, out, institution, province or null)`: what embracing the
/// institution costs the country, i32 thousandths of a ducat into `out`.
const COST: GameFunction<4> = GameFunction::at(0x3f6b50);
/// `(country, out, institution, null)`: how much of the country's
/// development has the institution, i32 thousandths into `out`.
const SHARE: GameFunction<4> = GameFunction::at(0x3f5510);
const MOST: usize = 1_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CAN_EMBRACE, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55", "embrace institution check"),
        Point::keep(COST, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57", "institution embrace cost"),
        Point::keep(SHARE, "4c 89 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08", "institution share of development"),
        // The country's save writer writes the embraced institutions.
        Point::keep(0x2dc992usize, "4d 8d 87 68 0c 00 00 ba 47 34 00 00", "embraced institutions").uses(&[country::EMBRACED]),
    ]
}

/// The institutions of the game's database, in the order of their index,
/// with the world's appearances.
fn database(m: &Memory, world_at: usize) -> GResult<Vec<(usize, bool)>> {
    let database = INSTITUTIONS.call(m, [])?;
    let all = containers::pointers(m, database + institution::DATABASE_LIST, MOST)?;
    let appeared = i32s(m, world_at + world::INSTITUTIONS)?;
    Ok(all.into_iter().enumerate().map(|(i, at)| (at, appeared.get(i).copied().unwrap_or(0) != 0)).collect())
}

fn embraced(m: &Memory, country_at: usize, index: i32) -> GResult<bool> {
    Ok(i32s(m, country_at + country::EMBRACED)?.get(index as usize).copied().unwrap_or(0) != 0)
}

/// An i32 a function of the game's writes into the place it is given.
fn written(m: &Memory, function: GameFunction<4>, country_at: usize, it: usize) -> GResult<i32> {
    let mut out = 0i32;
    function.call(m, [country_at, &mut out as *mut i32 as usize, it, 0])?;
    Ok(out)
}

/// What the view offers for one institution: nothing once embraced.
fn offered(m: &Memory, country_at: usize, it: usize) -> GResult<InstitutionOption> {
    let key = game_string::key(m, it + institution::KEY)?;
    let index = m.i32(it + institution::INDEX)?;
    let embraced = embraced(m, country_at, index)?;
    let share = thousandths(written(m, SHARE, country_at, it)?) * 100.0;
    let choice = if embraced {
        Choice::cannot(Vec::new(), Vec::new())
    } else {
        let cost = vec![Amount::new(Resource::Ducats, thousandths(written(m, COST, country_at, it)?))];
        let (can, reasons) = content::checked(m, |reasons| Ok(CAN_EMBRACE.call(m, [country_at, it, reasons, 1])?))?;
        if can {
            Choice::can(cost)
        } else {
            Choice::cannot(reasons, cost)
        }
    };
    Ok(InstitutionOption { name: content::name(m, &key)?, institution: key, embraced, share, choice })
}

/// The institutions that have appeared in the world, as the view lists
/// them.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<InstitutionOption>> {
    database(m, ctx.world)?.into_iter().filter(|&(_, appeared)| appeared).map(|(it, _)| offered(m, ctx.country, it)).collect()
}

/// The embrace command, the country and the institution by their indices,
/// as the view's button sends it.
fn command(player: Tag, index: i32) -> String {
    format!("embrace_institution_command={{ country={} institutions={index} message=yes }}", player.index())
}

/// Embraces an institution, as the view's button does.
pub fn embrace(Session { m, ctx, commands, .. }: Session, args::Embrace { institution: wanted }: args::Embrace) -> GResult<()> {
    for (it, appeared) in database(m, ctx.world)? {
        if game_string::key(m, it + institution::KEY)? != wanted {
            continue;
        }
        let index = m.i32(it + institution::INDEX)?;
        if embraced(m, ctx.country, index)? {
            return Err(checks::not_now(format!("the player has embraced {wanted} already")));
        }
        let choice = match appeared {
            true => offered(m, ctx.country, it)?.choice,
            false => {
                let (_, reasons) = content::checked(m, |reasons| Ok(CAN_EMBRACE.call(m, [ctx.country, it, reasons, 1])?))?;
                Choice::cannot(reasons, Vec::new())
            }
        };
        return action::carry_out(commands, &choice, &format!("embrace {wanted}"), || Ok(command(ctx.player, index)));
    }
    Err(checks::not_found("institution", &wanted))
}

crate::catalogue::bind! {
    eu4_harmony_remote::institutions: InstitutionRequest => InstitutionAnswer;
    List => list;
    Embrace => embrace;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;

    #[test]
    fn the_embrace_command_names_the_country_and_the_institution_by_index() {
        assert_eq!(command(tur(), 2), format!("embrace_institution_command={{ country={} institutions=2 message=yes }}", tur().index()));
    }
}
