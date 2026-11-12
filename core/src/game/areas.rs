//! A country's share of an area and its governing costs. State projections
//! preserve territorial cores, use the loaded static modifiers, and cross-check
//! the arithmetic against the game's getters (docs/research/area-governing-cost.md).
use super::{countries, modifiers, provinces, with_state, GResult, Session, Status, Tag};
use crate::{
    containers, game_string,
    memory::Memory,
    objects::campaign::province,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::{
    provinces::{args, AreaSnapshot, StateKind},
    CountryTag, ProvinceId,
};

const MAP_AREA: usize = 0x40;
const AREA_KEY: usize = 0x48;
const AREA_PROVINCES: usize = 0x88;
const TERRITORIAL_CORES: usize = 0x4a8;
const AREAS_CURRENT: usize = 0x98b8;
const STATICS: GameFunction<0> = GameFunction::at(0x8ecec0);
const COST_FACTOR: GameFunction<4> = GameFunction::at(0xa2e0f0);
const COST: GameFunction<4> = GameFunction::at(0xa2e6f0);
const USED: GameFunction<2> = GameFunction::at(0x3f31d0);

const GOVERNING: i32 = 0x19b;
const TERRITORIES: i32 = 0x19d;
const STATES: i32 = 0x19e;
const LOCAL: i32 = 0x19f;
const FLAT: i32 = 0x1a0;
const AREA_FLAT: i32 = 0x1a1;
const STATEWIDE: i32 = 0x1a2;
const MIN_FACTOR: i32 = 10;
const IN_STATE: usize = 0x310;
const CORE: usize = 0x460;
const NON_CORE: usize = 0x470;
const COLONIAL_CORE: usize = 0x480;
const TERRITORY_CORE: usize = 0x490;
const TERRITORY_NON_CORE: usize = 0x4a0;

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::keep(STATICS, "40 53 48 83 ec 20 48 8b 05 3b ee b3 01 48 85 c0", "static modifiers database"),
        Point::keep(COST_FACTOR, "40 55 53 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 b8 48 81 ec 48 01 00 00", "province governing cost factor"),
        Point::keep(COST, "48 8b c4 48 89 58 08 48 89 68 10 48 89 70 20 4c 89 40 18 57 41 56 41 57 48 83 ec 50", "province governing cost"),
        Point::keep(USED, "41 54 41 57 48 83 ec 68 c7 02 00 00 00 00 4c 8b e2 80 b9 b8 98 00 00 00", "current governing capacity used").uses(&[AREAS_CURRENT]),
        Point::keep(0x133ece8usize, "49 8b 46 28 4c 8b 60 40", "province area definition").uses(&[province::GEOMETRY, MAP_AREA]),
        Point::keep(0xac2c36usize, "48 83 c3 48", "area content key").uses(&[AREA_KEY]),
        Point::keep(0x3f130eusize, "8b f2 4c 8b 95 88 00 00 00 48 8b 85 90 00 00 00", "area province numbers").uses(&[AREA_PROVINCES, AREA_PROVINCES + 8]),
        Point::keep(0x95d31dusize, "4d 8b 82 b0 04 00 00 49 8b 8a a8 04 00 00", "territorial core countries").uses(&[TERRITORIAL_CORES, TERRITORIAL_CORES + 8]),
        Point::keep(0x8edff2usize, "4d 8d 86 10 03 00 00", "in state static modifier").uses(&[IN_STATE]),
        Point::keep(0x8ee0b0usize, "4d 8d 86 60 04 00 00", "core static modifier").uses(&[CORE]),
        Point::keep(0x8ee0c3usize, "4d 8d 86 70 04 00 00", "non core static modifier").uses(&[NON_CORE]),
        Point::keep(0x8ee0d6usize, "4d 8d 86 80 04 00 00", "colonial core static modifier").uses(&[COLONIAL_CORE]),
        Point::keep(0x8ee0e9usize, "4d 8d 86 90 04 00 00", "territory core static modifier").uses(&[TERRITORY_CORE]),
        Point::keep(0x8ee0fcusize, "4d 8d 86 a0 04 00 00", "territory non core static modifier").uses(&[TERRITORY_NON_CORE]),
        Point::keep(0xa2e170usize, "81 38 9b 01 00 00", "governing cost modifier").uses(&[GOVERNING as usize]),
        Point::keep(0xa2e1c0usize, "bb 9d 01 00 00", "territory governing cost modifier").uses(&[TERRITORIES as usize]),
        Point::keep(0xa2e1d6usize, "b8 9e 01 00 00", "state governing cost modifier").uses(&[STATES as usize]),
        Point::keep(0xa2e264usize, "81 38 9f 01 00 00", "local governing cost modifier").uses(&[LOCAL as usize]),
        Point::keep(0xa2e7d3usize, "81 38 a0 01 00 00", "local governing cost increase modifier").uses(&[FLAT as usize]),
        Point::keep(0x3f32f4usize, "81 38 a1 01 00 00", "state governing cost increase modifier").uses(&[AREA_FLAT as usize]),
        Point::keep(0xa2e4f8usize, "81 38 a2 01 00 00", "statewide governing cost modifier").uses(&[STATEWIDE as usize]),
        Point::keep(0xa2e5c9usize, "bb 0a 00 00 00", "minimum governing cost factor").uses(&[MIN_FACTOR as usize]),
    ]
}

fn out(m: &Memory, function: GameFunction<4>, p: usize, owner: &u64, factor: Option<i32>) -> GResult<i32> {
    let mut result = 0i32;
    let ptr = &mut result as *mut i32 as usize;
    let mut factor = factor;
    let fourth = factor.as_mut().map_or(0, |v| v as *mut i32 as usize);
    if function.call(m, [p, ptr, owner as *const u64 as usize, fourth])? != ptr {
        return Err("governing cost getter returned an unexpected pointer".to_string().into());
    }
    Ok(result)
}

fn total_used(m: &Memory, c: usize) -> GResult<i32> {
    if m.u8(c + AREAS_CURRENT)? == 0 {
        return super::fail(Status::NotReady, "country areas are not current");
    }
    let mut used = 0i32;
    let ptr = &mut used as *mut i32 as usize;
    if USED.call(m, [c, ptr])? != ptr {
        return Err("governing capacity getter returned an unexpected pointer".to_string().into());
    }
    Ok(used)
}

fn has_tag(m: &Memory, p: usize, offset: usize, owner: Tag) -> GResult<bool> {
    for entry in containers::entries(m, p + offset, 8, 0x8000)? {
        if Tag(m.u64(entry)?).index() == owner.index() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn value(m: &Memory, set: usize, which: i32) -> GResult<i32> {
    modifiers::value(m, set, which)
}

fn static_value(m: &Memory, statics: usize, slot: usize, which: i32) -> GResult<i32> {
    value(m, m.ptr(statics + slot)?, which)
}

#[derive(Clone)]
struct Member {
    address: usize,
    development: i32,
    local: i32,
    flat: i32,
    area_flat: i32,
    statewide: i32,
    transition: [i32; 4],
}

/// Fixed-point arithmetic of the native cost getter, with checked overflow.
fn cost(development: i32, factor: i32, flat: i32) -> GResult<i32> {
    let n = i64::from(development) * i64::from(factor.max(0)) / 1000 + i64::from(flat);
    i32::try_from(n.max(0)).map_err(|_| "governing cost overflow".to_string().into())
}

fn sum(values: impl IntoIterator<Item = i32>) -> GResult<i32> {
    values.into_iter().try_fold(0i32, |a, b| a.checked_add(b).ok_or_else(|| "governing cost overflow".to_string().into()))
}

/// The country's total getter keeps the larger of an area's sum and 1% of it.
fn area_total(total: i32) -> i32 {
    total.max(total / 100)
}

/// A projection built from the same modifiers, with no writes to the world.
fn projected(m: &Memory, owner: Tag, country_values: usize, members: &[Member]) -> GResult<i32> {
    let global = value(m, country_values, GOVERNING)?;
    let territory = value(m, country_values, TERRITORIES)?;
    let state = value(m, country_values, STATES)?;
    let statewide = sum(members.iter().map(|p| p.statewide))?;
    let future_statewide = sum(members.iter().map(|p| sum([p.statewide, p.transition[3]])).collect::<GResult<Vec<_>>>()?)?;
    let mut before = Vec::new();
    let mut after = Vec::new();
    for p in members {
        let factor = sum([1000, global, territory, p.local, statewide])?.max(MIN_FACTOR);
        if out(m, COST_FACTOR, p.address, &owner.0, None)? != factor {
            return Err("province governing factor differs from its modifiers".to_string().into());
        }
        let current = cost(p.development, factor, p.flat)?;
        if out(m, COST, p.address, &owner.0, Some(factor))? != current {
            return Err("province governing cost differs from its development and modifiers".to_string().into());
        }
        let future_factor = sum([1000, global, state, p.local, p.transition[0], future_statewide])?.max(MIN_FACTOR);
        let future_flat = sum([p.flat, p.transition[1]])?;
        before.push(sum([current, p.area_flat])?);
        after.push(sum([cost(p.development, future_factor, future_flat)?, p.area_flat, p.transition[2]])?);
    }
    area_total(sum(after)?).checked_sub(area_total(sum(before)?)).ok_or_else(|| "governing cost overflow".to_string().into())
}

pub fn area(Session { m, ctx, .. }: Session, args::Area { province: ProvinceId(id) }: args::Area) -> GResult<AreaSnapshot> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
    if Tag(m.u64(address + province::OWNER)?).index() != ctx.player.index() {
        return super::fail(Status::Rejected, "the province is not owned by the country acted as");
    }
    let geometry = m.ptr(address + province::GEOMETRY)?;
    let definition = m.ptr(geometry + MAP_AREA)?;
    if definition == 0 {
        return super::fail(Status::NotFound, "the province has no area");
    }
    let key = game_string::key(m, definition + AREA_KEY)?;
    let start = m.ptr(definition + AREA_PROVINCES)?;
    let count = containers::span(start, m.ptr(definition + AREA_PROVINCES + 8)?, 4, 0x10000)?;
    let ids = m.bytes(start, count * 4)?;
    let country_values = modifiers::values(m, ctx.country)?;
    let statics = STATICS.call(m, [])?;
    let in_state = m.ptr(address + province::STATE)? != 0;
    let mut members = Vec::new();
    let mut provinces = Vec::new();
    let mut trade_company = false;
    let mut development = 0i32;
    let mut governing_cost = 0i32;
    for number in ids.chunks_exact(4) {
        let id = u32::from_le_bytes(number.try_into().unwrap());
        let p = with_state(|s| s.geography.province_address(m, &ctx, id))?;
        if Tag(m.u64(p + province::OWNER)?).index() != ctx.player.index() {
            continue;
        }
        if (m.ptr(p + province::STATE)? != 0) != in_state {
            return Err("owned provinces of an area disagree on statehood".to_string().into());
        }
        provinces.push(ProvinceId(id));
        let dev = provinces::development(m, p)?;
        development = sum([development, dev])?;
        trade_company |= m.u8(p + province::HIDDEN)? & province::TRADE_COMPANY_BIT != 0;
        let set = p + province::MODIFIERS;
        let local = value(m, set, LOCAL)?;
        let flat = value(m, set, FLAT)?;
        let area_flat = value(m, set, AREA_FLAT)?;
        let statewide = value(m, set, STATEWIDE)?;
        let current_factor = out(m, COST_FACTOR, p, &ctx.player.0, None)?;
        governing_cost = sum([governing_cost, out(m, COST, p, &ctx.player.0, Some(current_factor))?, area_flat])?;
        let mut transition = [0i32; 4];
        if !in_state {
            let core = has_tag(m, p, province::CORES, ctx.player)?;
            let old = if core { TERRITORY_CORE } else { TERRITORY_NON_CORE };
            let new = if !core {
                NON_CORE
            } else if has_tag(m, p, TERRITORIAL_CORES, ctx.player)? {
                COLONIAL_CORE
            } else {
                CORE
            };
            for (i, which) in [LOCAL, FLAT, AREA_FLAT, STATEWIDE].into_iter().enumerate() {
                let old_value = static_value(m, statics, old, which)?.checked_neg().ok_or_else(|| "governing modifier overflow".to_string())?;
                transition[i] = sum([static_value(m, statics, new, which)?, old_value, static_value(m, statics, IN_STATE, which)?])?;
            }
        }
        members.push(Member { address: p, development: dev, local, flat, area_flat, statewide, transition });
    }
    let used = total_used(m, ctx.country)?;
    let max = (countries::governing_capacity(m, ctx.country)?.max * 1000.0).round() as i32;
    let (make_state_cost, unavailable_reason) = if in_state {
        (Some(0), None)
    } else if trade_company {
        (None, Some("the area contains a trade company province".to_string()))
    } else {
        match projected(m, ctx.player, country_values, &members) {
            Ok(cost) => (Some(cost), None),
            Err(e) => (None, Some(e.message)),
        }
    };
    Ok(AreaSnapshot {
        area: key,
        owner: CountryTag(ctx.player.letters()),
        provinces,
        development: super::thousandths(development),
        trade_company,
        state: if in_state { StateKind::State } else { StateKind::Territory },
        governing_cost: area_total(governing_cost),
        make_state_cost,
        governing_capacity_used: used,
        governing_capacity_max: max,
        unavailable_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn costs_keep_thousandths_and_apply_the_floor_before_flat_cost() {
        assert_eq!(cost(30_001, 500, 0).unwrap(), 15_000);
        assert_eq!(cost(30_001, 10, -301).unwrap(), 0);
        assert_eq!(cost(30_001, 10, 15_000).unwrap(), 15_300);
        assert!(cost(i32::MAX, i32::MAX, 0).is_err());
        assert!(sum([i32::MAX, 1]).is_err());
        assert_eq!(area_total(10_001), 10_001);
        assert_eq!(area_total(-10_001), -100);
    }
}
