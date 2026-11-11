//! The defender of the faith: the country that defends the player's
//! religion, and claiming it as the religion view's button does, through
//! the game's own command (docs/research/religion.md, "The defender of the
//! faith").
use super::{action, country as country_at, Ctx, GResult, Session, Tag};
use crate::{
    memory::Memory,
    objects::campaign::{country, religion, world},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    religion::DefenderOfTheFaith,
    CountryTag,
};

/// `(country, _, text) -> bool`: whether the country may claim to be the
/// defender of the faith of its religion, writing why not into the
/// `std::string` text when it is not null, as the religion view's button
/// asks it; the `claimdefender` command's check calls it with none. The
/// second argument is not read.
const MAY_CLAIM: GameFunction<3> = GameFunction::at(0x300b40);
/// What claiming it costs, a define in whole ducats, i32.
const COST: usize = 0x233e274;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_CLAIM, "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 88 54 24 10 55 41 54 41 55 41 56 41 57", "may claim defender of the faith"),
        // The check: the cost in thousandths against the treasury.
        Point::keep(0x300e73usize, "69 1d f7 d3 03 02 e8 03 00 00 41 39 9f b0 0b 00 00", "defender of the faith cost").uses(&[COST, country::TREASURY]),
        // The check: the country's religion, its record among the world's,
        // and whether the record's defender is valid.
        Point::keep(
            0x3012c1usize,
            "49 8b 9f 30 0f 00 00 45 32 f6 48 8b 03 48 8b cb ff 50 40 84 c0 74 23 48 8b 05 99 eb 03 02 48 8b 88 a8 25 00 00 8b 43 70 ff c8 48 98 48 8d 1c 80 48 c1 e3 04 48 03 59 30 eb 07 48 8b 1d be 7a 15 02 80 7b 3b 00",
            "religion's defender",
        )
        .uses(&[country::STATE_RELIGION, world::RELIGION_STATES, religion::INDEX, world::RELIGION_STATES_DATA, world::RELIGION_DEFENDER + 7]),
        // The claimdefender command's check asks it with no text.
        Point::keep(0xaacbf0usize, "48 8b 41 50 45 33 c0 48 c1 e8 20", "claimdefender command check"),
    ]
}

/// The country that defends the player's religion; none while none does,
/// or the player's religion is none.
fn defender_of(m: &Memory, ctx: &Ctx) -> GResult<Option<Tag>> {
    let faith = m.ptr(ctx.country + country::STATE_RELIGION)?;
    if faith == 0 {
        return Ok(None);
    }
    let index = m.i32(faith + religion::INDEX)?;
    if index <= 0 {
        return Ok(None);
    }
    let states = m.ptr(m.ptr(ctx.world + world::RELIGION_STATES)? + world::RELIGION_STATES_DATA)?;
    let tag = Tag(m.u64(states + (index as usize - 1) * world::RELIGION_STATE_SIZE + world::RELIGION_DEFENDER)?);
    Ok(tag.valid().then_some(tag))
}

/// Whether the player can claim it now, as the button's tooltip says.
fn choice(m: &Memory, ctx: &Ctx) -> GResult<Choice> {
    let cost = vec![Amount::new(Resource::Ducats, m.i32(m.at(COST))? as f64)];
    let (can, reasons) = super::content::checked(m, |text| Ok(MAY_CLAIM.call(m, [ctx.country, 0, text])?))?;
    Ok(if can { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// The defender of the faith of the player's religion, and what claiming
/// it offers now.
pub fn defender(Session { m, ctx, .. }: Session) -> GResult<DefenderOfTheFaith> {
    let defender = match defender_of(m, &ctx)? {
        Some(tag) => {
            country_at(m, &ctx, tag)?;
            Some(CountryTag(tag.letters()))
        }
        None => None,
    };
    Ok(DefenderOfTheFaith { defender, choice: choice(m, &ctx)? })
}

/// The command claiming it: the country alone.
fn command(player: Tag) -> String {
    format!("claimdefender={{ country=\"{}\" }}", super::content::letters(player))
}

/// Claims to be the defender of the faith, as the button does.
pub fn claim(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::carry_out(commands, &choice(m, &ctx)?, "claim to be the defender of the faith", || Ok(command(ctx.player)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::tur;

    #[test]
    fn the_command_names_the_country_alone() {
        assert_eq!(command(tur()), r#"claimdefender={ country="TUR" }"#);
    }
}
