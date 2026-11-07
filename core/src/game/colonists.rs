//! Colonists: what the province view offers the player for colonizing a
//! province, expelling a minority into a new colony and promoting settlement
//! growth, and carrying each out through the game's own commands
//! (docs/research/envoys.md, "Colonists").
use super::{
    action,
    envoys::{self, quoted},
    with_state, Ctx, GResult, Session, Tag,
};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{construction, country, province as field},
    points::Point,
};
use eu4_harmony_remote::colonists::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    colonists::{ColonistOptions, Expulsion},
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
/// The province's construction of a kind, or null: `(province, kind)`.
const CONSTRUCTION_OF: usize = 0xa1ba90;
/// The kind of a colonist promoting settlement growth.
const GROWTH: usize = 0x38a2;
/// The colonial upkeep a month of the country's colonies and of the provinces
/// a vector names, in thousandths of ducats before the country's factor:
/// `(country, i32* out, text or null, i32* factor in thousandths, vector of
/// provinces or null) -> out`.
const UPKEEP: usize = 0x34b7a0;
/// The country's factor of the colonial upkeep, thousandths.
const UPKEEP_FACTOR: usize = 0xe14;
/// Whether the country may have a colonist promote settlement growth in the
/// province, and why not: `(country, province, reasons or null) -> bool`,
/// appending to `reasons`.
const MAY_GROW: usize = 0x262060;
/// Whether the country may expel minorities at all: `(tag by value) ->
/// bool`; whether a province may take expelled minorities: `(province) ->
/// bool`; whether the colonize button lists a province of the player's to
/// expel from: `(province) -> bool`; and whether its minority may be
/// expelled now, and why not: `(province, reasons or null) -> bool`,
/// appending to `reasons`.
const MAY_EXPEL_ANY: usize = 0x264ec0;
const TAKES_EXPELLED: usize = 0x264fb0;
const LISTED: usize = 0x264f50;
const MAY_EXPEL: usize = 0x264ff0;
/// The most provinces believed.
const MOST: i32 = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_SEND, "40 53 48 83 ec 20 48 8b 81 80 14 00 00 48 8b da", "may send a colonist"),
        Point::keep(WHY_NOT, "44 88 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53 56 57", "why no colonist"),
        Point::keep(CONSTRUCTION, "48 89 5c 24 08 57 48 83 ec 20 48 8b 99 d8 04 00 00 48 85 db 74 23 66 66 0f 1f 84 00 00 00 00 00 48 8b 3b 48 8b 5b 10 48 8b cf 48 8b 07 ff 50 70 3c 0a", "colonist construction"),
        Point::keep(CONSTRUCTION_OF, "48 8b 89 d8 04 00 00 48 85 c9 74 15", "construction of a kind"),
        // The province view's settlement growth button asks for this kind.
        Point::keep(0x137a031usize, "ba a2 38 00 00", "settlement growth construction").uses(&[GROWTH]),
        // The cancel colonist command's check: the location, then the
        // construction's country.
        Point::keep(0x446850usize, "48 63 41 58 85 c0 0f 88 83 00 00 00 80 79 5c 00 75 74 4c 8b 41 50", "cancel colonist check"),
        Point::keep(0x4468d9usize, "48 8b 42 40", "colonist construction's country").uses(&[construction::COUNTRY]),
        Point::keep(UPKEEP, "4c 89 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53", "colonial upkeep"),
        // The budget's colonial upkeep: the sum times the country's factor.
        Point::keep(0xeb78bfusize, "48 63 9f 14 0e 00 00", "colonial upkeep factor").uses(&[UPKEEP_FACTOR]),
        Point::keep(MAY_GROW, "48 89 5c 24 08 48 89 74 24 10 55 57 41 56 48 8d 6c 24 b9 48 81 ec", "may promote settlement growth"),
        Point::keep(MAY_EXPEL_ANY, "40 53 48 83 ec 20 80 3d 67 b0 0d 02 00 48 8b d9 74 09", "may expel minorities"),
        Point::keep(TAKES_EXPELLED, "48 83 ec 28 48 8b 81 68 04 00 00 48 c1 e8 38 84 c0 75 18", "takes expelled minorities"),
        Point::keep(LISTED, "40 53 48 83 ec 20 f6 81 58 09 00 00 10 48 8b d9 74 3f", "lists a province to expel from"),
        Point::keep(MAY_EXPEL, "48 89 5c 24 10 48 89 7c 24 18 55 48 8b ec 48 83 ec 60 48 8b fa 48", "may expel the minority"),
        // The colonize button's list goes through the player's provinces and
        // keeps those of another culture or religion.
        Point::keep(0xe2e868usize, "49 8b 9c 24 38 13 00 00 48 89 5d 7f 49 63 84 24 44 13 00 00", "country provinces").uses(&[country::OWNED, country::OWNED_COUNT]),
        Point::keep(0xe2e95dusize, "4d 8b ae 40 04 00 00 4d 3b ac 24 28 12 00 00", "minority culture").uses(&[field::CULTURE, country::PRIMARY_CULTURE]),
        Point::keep(0xe2eab6usize, "4d 8b ae 48 04 00 00 4d 3b ac 24 30 0f 00 00", "minority religion").uses(&[field::RELIGION, country::STATE_RELIGION]),
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
pub fn options(Session { m, ctx, .. }: Session, args::Options { province: ProvinceId(id) }: args::Options) -> GResult<Box<ColonistOptions>> {
    read(m, &ctx, &province(m, &ctx, id)?, id).map(Box::new)
}

/// The colonial upkeep a month the player's colonies cost, in thousandths of
/// ducats, with the province at `extra` counted too when it is not 0.
fn upkeep(m: &Memory, ctx: &Ctx, extra: usize) -> GResult<i64> {
    let (mut out, factor) = (0i32, 1000i32);
    let provinces = [extra];
    let range = provinces.as_ptr_range();
    let vector = [range.start as usize, range.end as usize, range.end as usize];
    let list = if extra == 0 { 0 } else { vector.as_ptr() as usize };
    m.call(m.at(UPKEEP), &[ctx.country, &mut out as *mut i32 as usize, 0, &factor as *const i32 as usize, list])?;
    Ok(i64::from(out) * i64::from(m.i32(ctx.country + UPKEEP_FACTOR)?) / 1000)
}

/// What a colonist the player sends to the province costs: the colonist, and
/// the colonial upkeep a month its work there adds.
fn cost(m: &Memory, ctx: &Ctx, p: &Province) -> GResult<Vec<Amount>> {
    let added = upkeep(m, ctx, p.address)? - upkeep(m, ctx, 0)?;
    Ok(vec![Amount::new(Resource::Colonist, 1.0), Amount::new(Resource::MonthlyDucats, added as f64 / 1000.0)])
}

fn read(m: &Memory, ctx: &Ctx, p: &Province, id: u32) -> GResult<ColonistOptions> {
    let construction = m.call(m.at(CONSTRUCTION), &[p.address])?;
    let working = construction != 0 && Tag(m.u64(construction + construction::COUNTRY)?) == ctx.player;
    let size = m.i32(p.address + field::COLONY_SIZE)?;
    let colony = size > 0 && m.u8(p.address + field::HIDDEN)? & field::CITY_BIT == 0;
    let cost = cost(m, ctx, p)?;
    let send = if m.call(m.at(MAY_SEND), &[ctx.country, p.address])? as u8 != 0 {
        Choice::can(cost.clone())
    } else {
        let (_, text) = game_string::written(m, |out| m.call(m.at(WHY_NOT), &[ctx.country, out, p.address, 0]))?;
        Choice::cannot(envoys::lines(&text), cost.clone())
    };
    let recall = if working { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    let growth = m.call(m.at(CONSTRUCTION_OF), &[p.address, GROWTH])?;
    let growing = growth != 0 && Tag(m.u64(growth + construction::COUNTRY)?) == ctx.player;
    let (may, text) = game_string::written(m, |out| m.call(m.at(MAY_GROW), &[ctx.country, p.address, out]))?;
    let start_growth = if may as u8 != 0 { Choice::can(cost.clone()) } else { Choice::cannot(envoys::lines(&text), cost.clone()) };
    let stop_growth = if growing { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    Ok(ColonistOptions {
        province: ProvinceId(id),
        working,
        size: colony.then_some(size as f64 / 1000.0),
        expel_from: expulsions(m, ctx, p, &send)?,
        send,
        recall,
        growing,
        start_growth,
        stop_growth,
    })
}

/// The player's provinces the colonize button lists for expelling their
/// minority into the province, each with whether the player can now: as
/// sending a colonist there, and the province's own rules.
fn expulsions(m: &Memory, ctx: &Ctx, p: &Province, send: &Choice) -> GResult<Vec<Expulsion>> {
    if m.call(m.at(MAY_EXPEL_ANY), &[ctx.player.0 as usize])? as u8 == 0 || m.call(m.at(TAKES_EXPELLED), &[p.address])? as u8 == 0 {
        return Ok(Vec::new());
    }
    let count = m.i32(ctx.country + country::OWNED_COUNT)?;
    if !(0..MOST).contains(&count) {
        return Err(format!("{count} provinces").into());
    }
    let (ids, mut out) = (m.ptr(ctx.country + country::OWNED)?, Vec::new());
    for i in 0..count as usize {
        let from = m.u32(ids + i * 4)?;
        let address = with_state(|s| s.geography.province_address(m, ctx, from))?;
        let minority = m.ptr(address + field::CULTURE)? != m.ptr(ctx.country + country::PRIMARY_CULTURE)?
            || m.ptr(address + field::RELIGION)? != m.ptr(ctx.country + country::STATE_RELIGION)?;
        if !minority || m.call(m.at(LISTED), &[address])? as u8 == 0 {
            continue;
        }
        let choice = if !send.can {
            send.clone()
        } else {
            let (may, text) = game_string::written(m, |out| m.call(m.at(MAY_EXPEL), &[address, out]))?;
            if may as u8 != 0 {
                send.clone()
            } else {
                Choice::cannot(envoys::lines(&text), send.cost.clone())
            }
        };
        out.push(Expulsion { from: ProvinceId(from), choice });
    }
    Ok(out)
}

/// What a colonist the player sends to the province does there.
enum Work {
    Colonize,
    Recall,
    Expel { from: u32 },
    StartGrowth,
    StopGrowth,
}

/// The command each of the province view's colonist buttons posts.
fn written(player: Tag, province: u32, work: Work) -> String {
    let country = quoted(player);
    match work {
        Work::Colonize => format!("send_colonist={{ country={country} location={province} }}"),
        Work::Recall => format!("cancel_colonist={{ country={country} location={province} }}"),
        Work::Expel { from } => format!("send_colonist={{ country={country} location={province} expulsion_province={from} }}"),
        Work::StartGrowth => format!("send_colonist={{ country={country} location={province} settlement_growth_construction=yes }}"),
        Work::StopGrowth => format!("cancel_colonist={{ country={country} location={province} settlement_growth_construction=yes }}"),
    }
}

/// Sends one of the player's colonists to province `id`.
pub fn send(Session { m, ctx, commands, .. }: Session, args::Send { province: ProvinceId(id) }: args::Send) -> GResult<()> {
    let options = read(m, &ctx, &province(m, &ctx, id)?, id)?;
    if !options.send.can {
        return Err(action::not_allowed("send a colonist there", &options.send.reasons));
    }
    action::post(commands, &written(ctx.player, id, Work::Colonize), "send a colonist there")
}

/// Recalls the player's colonist from province `id`.
pub fn recall(Session { m, ctx, commands, .. }: Session, args::Recall { province: ProvinceId(id) }: args::Recall) -> GResult<()> {
    if !read(m, &ctx, &province(m, &ctx, id)?, id)?.working {
        return Err(envoys::checks::none_there("colonist"));
    }
    action::post(commands, &written(ctx.player, id, Work::Recall), "recall the colonist")
}

/// Sends one of the player's colonists to found a colony in province
/// `province` with the minority expelled from the player's province `from`.
pub fn expel(
    Session { m, ctx, commands, .. }: Session,
    args::ExpelMinorities { province: ProvinceId(province), from: ProvinceId(from) }: args::ExpelMinorities,
) -> GResult<()> {
    let options = read(m, &ctx, &self::province(m, &ctx, province)?, province)?;
    let Some(offered) = options.expel_from.iter().find(|e| e.from.0 == from) else {
        return Err(envoys::checks::no_minority(from));
    };
    if !offered.choice.can {
        return Err(action::not_allowed("expel that minority there", &offered.choice.reasons));
    }
    action::post(commands, &written(ctx.player, province, Work::Expel { from }), "expel that minority there")
}

/// Has one of the player's colonists promote settlement growth in province
/// `id`, or, `start` false, stops the one there.
pub fn change_growth(Session { m, ctx, commands, .. }: Session, id: u32, start: bool) -> GResult<()> {
    let options = read(m, &ctx, &province(m, &ctx, id)?, id)?;
    if !start {
        // The command's own check takes any province.
        if !options.growing {
            return Err(envoys::checks::none_there("colonist promoting settlement growth"));
        }
        return action::post(commands, &written(ctx.player, id, Work::StopGrowth), "stop promoting settlement growth there");
    }
    if !options.start_growth.can {
        return Err(action::not_allowed("promote settlement growth there", &options.start_growth.reasons));
    }
    action::post(commands, &written(ctx.player, id, Work::StartGrowth), "promote settlement growth there")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_name_the_country_the_province_and_the_work() {
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(written(fra, 1000, Work::Colonize), r#"send_colonist={ country="FRA" location=1000 }"#);
        assert_eq!(written(fra, 1000, Work::Recall), r#"cancel_colonist={ country="FRA" location=1000 }"#);
        assert_eq!(written(fra, 1000, Work::Expel { from: 183 }), r#"send_colonist={ country="FRA" location=1000 expulsion_province=183 }"#);
        assert_eq!(written(fra, 183, Work::StartGrowth), r#"send_colonist={ country="FRA" location=183 settlement_growth_construction=yes }"#);
        assert_eq!(written(fra, 183, Work::StopGrowth), r#"cancel_colonist={ country="FRA" location=183 settlement_growth_construction=yes }"#);
    }
}
