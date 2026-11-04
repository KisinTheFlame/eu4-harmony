//! Colonists: what the province view offers the player for colonizing a
//! province, and sending and recalling a colonist through the game's own
//! commands (docs/research/envoys.md, "Colonists").
use super::{
    context,
    envoys::{self, quoted},
    game, with_state, Caller, Ctx, GResult, Tag,
};
use crate::{game_string, memory::Memory, points::Point};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    colonists::ColonistOptions,
    ProvinceId,
};

/// Whether the country may send a colonist to the province:
/// `(country, province) -> bool`.
const MAY_SEND: usize = 0x34e7f0;
/// Why not, as the province view's tooltip says: `(country, out, province,
/// _)`, constructing `out`.
const WHY_NOT: usize = 0x34e890;
/// The province's colonist construction, or null: `(province)`.
const CONSTRUCTION: usize = 0xa1ba30;
/// The colonist construction's country.
const SENDER: usize = 0x40;
/// The colony's settlers, thousandths.
const SIZE: usize = 0x3f8;
/// The province's flags, and the one of a city.
const FLAGS: usize = 0x958;
const CITY: u8 = 0x10;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_SEND, "40 53 48 83 ec 20 48 8b 81 80 14 00 00 48 8b da", "may send a colonist"),
        Point::keep(WHY_NOT, "44 88 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53 56 57", "why no colonist"),
        Point::keep(CONSTRUCTION, "48 89 5c 24 08 57 48 83 ec 20 48 8b 99 d8 04 00 00 48 85 db 74 23 66 66 0f 1f 84 00 00 00 00 00 48 8b 3b 48 8b 5b 10 48 8b cf 48 8b 07 ff 50 70 3c 0a", "colonist construction"),
        // The cancel colonist command's check: the location, then the
        // construction's country.
        Point::keep(0x446850usize, "48 63 41 58 85 c0 0f 88 83 00 00 00 80 79 5c 00 75 74 4c 8b 41 50", "cancel colonist check"),
        Point::keep(0x9d7962usize, "41 8b 9f f8 03 00 00", "colony size").uses(&[SIZE]),
    ]
}

struct Province {
    address: usize,
}

fn province(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Province> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    Ok(Province { address: with_state(|s| s.geography.province_address(m, ctx, id))? })
}

/// What the province view offers the player in province `id`.
pub fn options(_: Caller, id: u32) -> GResult<ColonistOptions> {
    let m = game()?;
    let ctx = context(m)?;
    read(m, &ctx, &province(m, &ctx, id)?, id)
}

fn read(m: &Memory, ctx: &Ctx, p: &Province, id: u32) -> GResult<ColonistOptions> {
    let construction = m.call(m.at(CONSTRUCTION), &[p.address])?;
    let working = construction != 0 && Tag(m.u64(construction + SENDER)?) == ctx.player;
    let size = m.i32(p.address + SIZE)?;
    let colony = size > 0 && m.u8(p.address + FLAGS)? & CITY == 0;
    let cost = vec![Amount::new(Resource::Colonist, 1.0)];
    let send = if m.call(m.at(MAY_SEND), &[ctx.country, p.address])? as u8 != 0 {
        Choice::can(cost)
    } else {
        let (_, text) = game_string::written(m, |out| m.call(m.at(WHY_NOT), &[ctx.country, out, p.address, 0]))?;
        Choice::cannot(envoys::lines(&text), cost)
    };
    let recall = if working { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    Ok(ColonistOptions { province: ProvinceId(id), working, size: colony.then_some(size as f64 / 1000.0), send, recall })
}

/// The command the province view's buttons post: sending the player's
/// colonist to the province, as the send button does, or recalling it.
fn written(player: Tag, province: u32, recall: bool) -> String {
    if recall {
        format!("cancel_colonist={{ country={} location={province} }}", quoted(player))
    } else {
        format!("send_colonist={{ country={} location={province} }}", quoted(player))
    }
}

/// Sends one of the player's colonists to province `id`.
pub fn send(_: Caller, id: u32) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let options = read(m, &ctx, &province(m, &ctx, id)?, id)?;
    if !options.send.can {
        return Err(eu4_harmony_checks::envoys::not_allowed("send a colonist there", &options.send.reasons).into());
    }
    envoys::post(&written(ctx.player, id, false), "the colonist command")
}

/// Recalls the player's colonist from province `id`.
pub fn recall(_: Caller, id: u32) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    if !read(m, &ctx, &province(m, &ctx, id)?, id)?.working {
        return Err(eu4_harmony_checks::envoys::none_there("colonist").into());
    }
    envoys::post(&written(ctx.player, id, true), "the cancel colonist command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_name_the_country_and_the_province() {
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(written(fra, 1000, false), r#"send_colonist={ country="FRA" location=1000 }"#);
        assert_eq!(written(fra, 1000, true), r#"cancel_colonist={ country="FRA" location=1000 }"#);
    }
}
