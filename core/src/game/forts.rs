//! Forts: the player's forts, and mothballing and reopening them with the
//! game's command that the province view's fort maintenance box posts
//! (docs/research/sieges.md, "Mothballing and reopening a fort").
use super::units::checks;
use super::{action, with_state, Ctx, GResult, Session};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{
        country,
        province::{FORT_LEVEL, MOTHBALL, MOTHBALLED},
    },
    points::{GameFunction, Point},
};
use eu4_harmony_remote::forts::args;
use eu4_harmony_remote::{choices::Choice, forts::PlayerFort, ProvinceId};

/// A province's byte that says it has a fort.
const HAS_FORT: usize = 0x2dd8;
/// `(tag by value, province ID, text or null)`: whether the country may
/// mothball or reopen the fort, with the box's reasons.
const MAY_MOTHBALL: GameFunction<3> = GameFunction::at(0x5db5a0);
/// The most provinces read.
const MOST: i32 = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // The military view's "mothball all" walks the country's provinces
        // and asks each whether it has a fort.
        Point::keep(0x5dbc48usize, "49 8b bd 38 13 00 00 49 63 85 44 13 00 00", "country provinces").uses(&[country::OWNED, country::OWNED_COUNT]),
        Point::keep(0x5dbc71usize, "80 bb d8 2d 00 00 00", "province has a fort").uses(&[HAS_FORT]),
        Point::keep(MAY_MOTHBALL, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 40 49 8b d8 41 b0 01", "may mothball a fort"),
    ]
}

/// The command the box posts, which mothballs an open fort and reopens a
/// mothballed one.
fn toggle_text(country: [u8; 3], province: u32) -> String {
    format!("mothball_fort={{ province={province} country=\"{}\" }}", String::from_utf8_lossy(&country))
}

fn read(m: &Memory, ctx: &Ctx, id: u32, address: usize) -> GResult<PlayerFort> {
    let (may, text) = game_string::written(m, |t| MAY_MOTHBALL.call(m, [ctx.player.0 as usize, id as usize, t]))?;
    let choice = if may as u8 != 0 { Choice::can(Vec::new()) } else { Choice::cannot(super::reasons(&text), Vec::new()) };
    Ok(PlayerFort { province: ProvinceId(id), level: m.i32(address + FORT_LEVEL)?, mothballed: m.u8(address + MOTHBALL)? == MOTHBALLED, choice })
}

/// The player's provinces with a fort.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<PlayerFort>> {
    let count = m.i32(ctx.country + country::OWNED_COUNT)?;
    if !(0..MOST).contains(&count) {
        return Err(format!("{count} provinces").into());
    }
    let ids = m.ptr(ctx.country + country::OWNED)?;
    let mut out = Vec::new();
    for i in 0..count as usize {
        let id = m.u32(ids + i * 4)?;
        let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
        if m.u8(address + HAS_FORT)? != 0 {
            out.push(read(m, &ctx, id, address)?);
        }
    }
    Ok(out)
}

/// Mothballs the fort (`mothball` true) or reopens it.
fn set(Session { m, ctx, commands, .. }: Session, id: u32, mothball: bool) -> GResult<()> {
    let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
    if m.u8(address + HAS_FORT)? == 0 || m.u64(address + crate::objects::campaign::province::OWNER)? != ctx.player.0 {
        return Err(checks::no_fort(id));
    }
    let fort = read(m, &ctx, id, address)?;
    if fort.mothballed == mothball {
        return Err(checks::fort_already(id, mothball));
    }
    let doing = if mothball { "mothball this fort" } else { "reopen this fort" };
    action::carry_out(commands, &fort.choice, doing, || Ok(toggle_text(ctx.player.letters(), id)))
}

pub fn mothball(session: Session, args::Mothball { province: ProvinceId(province) }: args::Mothball) -> GResult<()> {
    set(session, province, true)
}

pub fn reopen(session: Session, args::Reopen { province: ProvinceId(province) }: args::Reopen) -> GResult<()> {
    set(session, province, false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_province_and_the_country() {
        assert_eq!(super::toggle_text(*b"FRA", 183), r#"mothball_fort={ province=183 country="FRA" }"#);
    }
}
