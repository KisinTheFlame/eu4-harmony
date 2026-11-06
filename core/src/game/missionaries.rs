//! Missionaries: what the province view's missionary button offers the
//! player in a province, sending and recalling one through the game's own
//! command, and the number of missionaries the game sends by itself
//! (docs/research/envoys.md, "Missionaries").
use super::{
    envoys::{self, quoted, Kind},
    with_state, Ctx, GResult, Session, Tag,
};
use crate::{game_string, memory::Memory, objects::campaign::province as province_fields, points::Point};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    missionaries::{AutomaticMissionaries, MissionaryOptions},
    ProvinceId,
};

/// Whether the country may send a missionary to the province:
/// `(country, province) -> bool`.
const MAY_SEND: usize = 0x3546b0;
/// Why not, as the province view's tooltip says: `(country, out, province)`,
/// writing nothing when every rule holds.
const WHY_NOT: usize = 0x354a00;
/// The yearly upkeep of a missionary the country sends there, before its
/// multiplier: `(country, out i32*, province) -> out`.
const UPKEEP: usize = 0x34c730;
/// The province's missionary construction, or null: `(province)`.
const CONSTRUCTION: usize = 0xa1a360;
/// The missionary construction's country.
const SENDER: usize = 0x80;
/// The country's multiplier of the upkeep, thousandths.
const UPKEEP_FACTOR: usize = 0xedc;
/// The province's conversion progress, thousandths of a percent.
const PROGRESS: usize = 0x894;
/// How many of the country's missionaries the game sends by itself.
const AUTOMATIC: usize = 0x14a0;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_SEND, "4c 8b dc 49 89 5b 08 49 89 6b 10 49 89 73 18 49 89 7b 20", "may send a missionary"),
        Point::keep(WHY_NOT, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20 55 41 54 41 55 41 56 41 57 48 8d 6c 24 c9 48 81 ec", "why no missionary"),
        Point::keep(UPKEEP, "40 53 56 57 41 56 48 83 ec 28 49 8b f0 4c 8b f2 48 8b d9", "missionary upkeep"),
        // The province's constructions, walked for the missionary's.
        Point::keep(0xa1a320usize, "40 53 48 83 ec 20 48 8b 99 d8 04 00 00 48 85 db 74 16 48 8b 0b 48 8b 5b 10 48 8b 01 ff 50 70 3c 05", "has a missionary"),
        Point::keep(CONSTRUCTION, "48 89 5c 24 08 57 48 83 ec 20 48 8b 99 d8 04 00 00 48 85 db 74 23 66 66 0f 1f 84 00 00 00 00 00 48 8b 3b 48 8b 5b 10 48 8b cf 48 8b 07 ff 50 70 3c 05", "missionary construction"),
        // The missionary command's check compares the construction's country
        // with the sender's.
        Point::keep(0xaab0c5usize, "48 8b 88 80 00 00 00", "missionary's country").uses(&[SENDER]),
        // The send missionary tooltip's upkeep a month.
        Point::keep(0x1206e80usize, "48 63 8e dc 0e 00 00", "missionary upkeep factor").uses(&[UPKEEP_FACTOR]),
        Point::keep(0x13797e3usize, "49 8b 45 20 48 63 88 94 08 00 00", "missionary progress").uses(&[PROGRESS]),
        // The automatic missionaries command's execute.
        Point::keep(0x2bf79busize, "41 8b 91 a0 14 00 00", "automatic missionaries").uses(&[AUTOMATIC]),
    ]
}

struct Province {
    address: usize,
    owner: Tag,
}

fn province(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Province> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    let address = with_state(|s| s.geography.province_address(m, ctx, id))?;
    Ok(Province { address, owner: Tag(m.u64(address + province_fields::OWNER)?) })
}

/// What the province view offers the player in province `id`; in the
/// player's view another's missionary's progress in another country's
/// province is left out, as `province.get` leaves it out.
pub fn options(Session { m, ctx, view, .. }: Session, id: u32) -> GResult<MissionaryOptions> {
    let p = province(m, &ctx, id)?;
    let (working, progress, send, recall) = read(m, &ctx, &p)?;
    let owner = Some(p.owner).filter(|t| t.valid());
    let progress = progress.filter(|_| working || super::player::Eyes::acting(view, ctx.player).sees_inside(owner));
    Ok(MissionaryOptions { province: ProvinceId(id), working, progress, send, recall })
}

/// Whether the player's missionary works there, the conversion's progress,
/// and the send and recall choices.
fn read(m: &Memory, ctx: &Ctx, p: &Province) -> GResult<(bool, Option<f64>, Choice, Choice)> {
    let construction = m.call(m.at(CONSTRUCTION), &[p.address])?;
    let working = construction != 0 && m.u64(construction + SENDER)? == ctx.player.0;
    let progress = (construction != 0).then(|| m.i32(p.address + PROGRESS).map(|v| v as f64 / 1000.0)).transpose()?;
    let (_, text) = game_string::written(m, |out| m.call(m.at(WHY_NOT), &[ctx.country, out, p.address]))?;
    let reasons = envoys::lines(&text);
    let may = m.call(m.at(MAY_SEND), &[ctx.country, p.address])? as u8 != 0;
    let cost = vec![Amount::new(Resource::Missionary, 1.0), Amount::new(Resource::MonthlyDucats, monthly_upkeep(m, ctx, p.address)?)];
    let send = if construction == 0 && may && reasons.is_empty() { Choice::can(cost) } else { Choice::cannot(reasons, cost) };
    let recall = if working { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    Ok((working, progress, send, recall))
}

/// The upkeep a month of a missionary the player sends to the province, in
/// ducats, as the send missionary tooltip shows it.
fn monthly_upkeep(m: &Memory, ctx: &Ctx, province: usize) -> GResult<f64> {
    let mut yearly = 0i32;
    m.call(m.at(UPKEEP), &[ctx.country, &mut yearly as *mut i32 as usize, province])?;
    let factor = m.i32(ctx.country + UPKEEP_FACTOR)? as i64;
    Ok((yearly as i64 * factor / 1000) as f64 / 1000.0 / 12.0)
}

/// The command the province view's button posts: sending the player's
/// missionary to the province, or recalling it.
fn written(owner: Tag, player: Tag, province: u32, recall: bool) -> String {
    format!("send_missionary={{ country={} owned_by={} location={province}{} }}", quoted(owner), quoted(player), if recall { " cancel=yes" } else { "" })
}

/// Sends one of the player's missionaries to province `id`.
pub fn send(Session { m, ctx, .. }: Session, id: u32) -> GResult<()> {
    let p = province(m, &ctx, id)?;
    let (_, _, send, _) = read(m, &ctx, &p)?;
    if !send.can {
        return Err(eu4_harmony_checks::envoys::not_allowed("send a missionary there", &send.reasons).into());
    }
    envoys::post(&written(p.owner, ctx.player, id, false), "the missionary command")
}

/// Recalls the player's missionary from province `id`.
pub fn recall(Session { m, ctx, .. }: Session, id: u32) -> GResult<()> {
    let p = province(m, &ctx, id)?;
    if !read(m, &ctx, &p)?.0 {
        return Err(eu4_harmony_checks::envoys::none_there("missionary").into());
    }
    envoys::post(&written(p.owner, ctx.player, id, true), "the missionary command")
}

/// The player's missionaries and how many of them the game sends by itself.
pub fn automatic(Session { m, ctx, .. }: Session) -> GResult<AutomaticMissionaries> {
    let (missionaries, _) = envoys::count(m, ctx.country, Kind::Missionary)?;
    let automatic = m.i32(ctx.country + AUTOMATIC)?.max(0) as u32;
    let choice = |can: bool| if can { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    Ok(AutomaticMissionaries { missionaries, automatic, raise: choice(automatic < missionaries), lower: choice(automatic > 0) })
}

/// Has the game send one more (`raise`) or one fewer of the player's
/// missionaries by itself, as the religion view's buttons do.
pub fn set_automatic(session: Session, raise: bool) -> GResult<()> {
    let now = automatic(session)?;
    if raise && now.automatic >= now.missionaries {
        return Err(eu4_harmony_checks::envoys::all_automatic(now.missionaries).into());
    }
    if !raise && now.automatic == 0 {
        return Err(eu4_harmony_checks::envoys::none_automatic().into());
    }
    let ctx = session.ctx;
    envoys::post(
        &format!("automate_missionary_command={{ country={}{} }}", quoted(ctx.player), if raise { " value=yes" } else { "" }),
        "the automatic missionaries command",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_names_the_owner_the_sender_and_the_province() {
        let (fra, eng) = (Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0")), Tag(u64::from_le_bytes(*b"ENG\0\0\0\0\0")));
        assert_eq!(written(eng, fra, 236, false), r#"send_missionary={ country="ENG" owned_by="FRA" location=236 }"#);
        assert_eq!(written(eng, fra, 236, true), r#"send_missionary={ country="ENG" owned_by="FRA" location=236 cancel=yes }"#);
    }
}
