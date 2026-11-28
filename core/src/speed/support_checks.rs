//! Exact interval-bounded lazy route evaluation for AI support checks
//! (`0x106320`, `docs/game/ai/support-checks.md`).
//!
//! `0x106320(agent, province, army, strength_ptr, share_ptr, route_arg,
//! only_enemies, _)` answers whether friendly strength reaching `province` in
//! time exceeds enemy strength by `share`, scaled by the combat-roll factor
//! `0x10cd80(max_friendly_leader - max_enemy_leader, -terrain_penalty)`.
//!
//! Profiling shows over 99% of `0x106320` is spent in `0x96c520` route queries,
//! while `0xd46800` strength evaluation is under 0.2%. Because every term is
//! integer-added and leader pips take a running maximum, the result does not
//! depend on unit order. By evaluating free (same-province / adjacent-region)
//! units first, bounding pending units with their exact scaled strengths and
//! leader pips, and querying routes lazily in descending strength order, both
//! guaranteed-`true` and guaranteed-`false` outcomes terminate as soon as the
//! interval `[lower, upper]` collapses.
//!
//! Checked mode (`speed::checked`) compares every answer against the game's
//! original `0x106320`.
use crate::base::called_by_game;
use crate::native::game_heap::DELETE;
use crate::native::objects::campaign::{
    countries::LIST as COUNTRY_LIST,
    country::{TECHNOLOGY, UNITS},
    functions::ROUTE_QUERY,
    globals::{COUNTRY_MANAGER, MAP, ROUTE_POLICY, SPECIAL_COUNTRIES},
    route_options, tag,
    unit::{ATTACHED_TO, LOCATION, OWNER, PATH, REGIMENTS, RETREATING, SIEGE, TASK},
};
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;

const SUPPORT_CHECK: usize = 0x106320;
pub(super) const ARMY_STRENGTH: usize = 0xd46800;
const ROLL_FACTOR: usize = 0x10cd80;
const MAP_DISTANCE_SQ: usize = 0x88a390;
const PROVINCE_BY_ID: usize = 0x9aa80;

const BASE_FRIENDLY_STRENGTH: usize = 0x1fd3b38;
const MAX_SUPPORT_DISTANCE: usize = 0x233fc7c;
pub(super) const ZERO_STRENGTH_CONST: usize = 0x23582d0;

const AGENT_COUNTRY_TAG: usize = 0x28;
const COUNTRY_WAR_ALLIES: usize = 0x1bc0;
const COUNTRY_WAR_ENEMIES: usize = 0x1b88;
const TECH_COMBAT_WIDTH: usize = 0x8c;

const PROVINCE_ID: usize = 0x20;
const PROVINCE_GEOM: usize = 0x28;
const PROVINCE_REGION: usize = 0x340;
const GEOM_TERRAIN_TYPE: usize = 0x30;
const TERRAIN_PENALTY: usize = 0xd4;
const GEOM_NEIGHBOR_SET_BEGIN: usize = 0x180;
const GEOM_NEIGHBOR_SET_END: usize = 0x188;

const MAP_WIDTH: usize = 0x28;
const MAP_HEIGHT: usize = 0x2c;
const MAP_LINEAR_DISTANCE: usize = 0x31;
const MAP_PROVINCE_CENTERS: usize = 0x6c0;
const MAP_CENTER_STRIDE: usize = 0x230;
const MAP_CENTER_POS: usize = 0x7c;

const UNIT_IS_FLEET_SLOT: usize = 0x88;
const UNIT_SIEGE_ALLOW_FLAG: usize = 0x209;
const TASK_KIND_SLOT: usize = 0x50;

const FIXED_SCALE: i64 = 0x1f40000; // 1000 << 15
const ENEMY_BASE_STRENGTH: i64 = 0x8000;

type SupportCheck = unsafe extern "C" fn(usize, usize, usize, *const i64, *const i32, usize, u8, usize) -> u8;
static ORIGINAL_SUPPORT_CHECK: Function<SupportCheck> = Function::new();

#[derive(Clone, Copy)]
struct PendingUnit {
    unit: usize,
    start_province: usize,
    strength: i64,
    leader: i32,
    regiments: i32,
}

#[derive(Default)]
struct Scratch {
    friends: Vec<PendingUnit>,
    enemies: Vec<PendingUnit>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundDecision {
    True,
    False,
    Undecided,
}

#[derive(Clone, Copy)]
struct EvalContext {
    only_enemies: bool,
    width_threshold: i32,
    neg_terrain: i32,
    share_q15: i64,
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    f_min: i64,
    f_max: i64,
    b_min: i64,
    b_max: i64,
    l_min: i32,
    l_max: i32,
    e_min: i32,
    e_max: i32,
    regs_min: i32,
    regs_max: i32,
    monotonic: bool,
}

fn eval_ratio(friends: i64, enemies: i64, factor: i32, share_q15: i64) -> bool {
    let factor_q15 = ((factor as i64) << 30) / FIXED_SCALE;
    let lhs = (factor_q15.wrapping_mul((friends << 15) / enemies)) >> 15;
    lhs > share_q15
}

fn decide_bounds(bounds: &Bounds, ctx: &EvalContext, mut roll_factor: impl FnMut(i32, i32) -> i32) -> BoundDecision {
    if ctx.only_enemies && bounds.regs_max <= ctx.width_threshold {
        return BoundDecision::True;
    }
    if !bounds.monotonic || bounds.f_min < 0 || bounds.b_min <= 0 || bounds.b_max <= 0 {
        return BoundDecision::Undecided;
    }
    let m_lo = roll_factor(bounds.l_min - bounds.e_max, ctx.neg_terrain);
    let m_hi = roll_factor(bounds.l_max - bounds.e_min, ctx.neg_terrain);
    if m_lo < 0 || m_hi < m_lo {
        return BoundDecision::Undecided;
    }
    if eval_ratio(bounds.f_min, bounds.b_max, m_lo, ctx.share_q15) {
        return BoundDecision::True;
    }
    if (!ctx.only_enemies || bounds.regs_min > ctx.width_threshold) && !eval_ratio(bounds.f_max, bounds.b_min, m_hi, ctx.share_q15) {
        return BoundDecision::False;
    }
    BoundDecision::Undecided
}

#[derive(Clone, Copy)]
struct ScanParams {
    self_army: usize,
    target_province: usize,
    special: [i32; 3],
    map: usize,
    max_dist: i32,
    half_max: i32,
}

#[derive(Clone, Copy)]
struct SupportArgs {
    agent: usize,
    target_province: usize,
    self_army: usize,
    strength_ptr: *const i64,
    share_ptr: *const i32,
    route_arg: usize,
    only_enemies: u8,
}

/// Evaluates whether a unit passes the support check's eligibility filters,
/// returning `Ok(Some((effective_distance, free_reachable)))` when in range,
/// `Ok(None)` when ineligible, or `Err(())` when unexpected pointers/IDs
/// require falling back to the game's function.
unsafe fn check_unit_eligible(m: &crate::native::memory::Memory, unit: usize, check_siege: bool, p: &ScanParams) -> Result<Option<(i32, bool)>, ()> {
    if unit == 0 {
        return Err(());
    }
    if unit == p.self_army {
        return Ok(None);
    }
    let owner = unsafe { *((unit + OWNER) as *const u64) };
    let unit_loc = unsafe { *((unit + LOCATION) as *const usize) };
    let valid_owner = ((owner >> tag::VALID) as u8 != 0) && ((owner >> tag::KIND) as u8 != tag::REBELS) && {
        let idx = (owner >> tag::INDEX) as i16 as i32;
        idx != p.special[0] && idx != p.special[1] && idx != p.special[2]
    };
    if !valid_owner && unit_loc != p.target_province {
        return Ok(None);
    }
    let vtable = unsafe { *(unit as *const usize) };
    if vtable == 0 {
        return Err(());
    }
    let is_fleet: unsafe extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(*((vtable + UNIT_IS_FLEET_SLOT) as *const usize)) };
    if unsafe { is_fleet(unit) } != 0 {
        return Ok(None);
    }
    if unsafe { *((unit + RETREATING[0]) as *const u8) } != 0 {
        return Ok(None);
    }
    let task = unsafe { *((unit + TASK) as *const usize) };
    if task == 0 {
        return Err(());
    }
    let task_vtable = unsafe { *(task as *const usize) };
    if task_vtable == 0 {
        return Err(());
    }
    let task_kind_fn: unsafe extern "C" fn(usize) -> i32 = unsafe { std::mem::transmute(*((task_vtable + TASK_KIND_SLOT) as *const usize)) };
    let kind = unsafe { task_kind_fn(task) };
    if matches!(kind, 5 | 11 | 12) {
        return Ok(None);
    }
    if check_siege && unsafe { *((unit + SIEGE) as *const usize) } != 0 && unsafe { *((unit + UNIT_SIEGE_ALLOW_FLAG) as *const u8) } == 0 {
        return Ok(None);
    }

    let path_count = unsafe { *((unit + PATH + 0x10) as *const i32) };
    let eff_prov = if path_count == 0 {
        unit_loc
    } else if path_count > 0 {
        let tail = unsafe { *((unit + PATH + 8) as *const usize) };
        if tail == 0 {
            return Err(());
        }
        let dest_id = unsafe { *(tail as *const i32) } as i64;
        type ProvById = unsafe extern "C" fn(usize, i64) -> usize;
        let by_id: ProvById = unsafe { std::mem::transmute(m.at(PROVINCE_BY_ID)) };
        unsafe { by_id(p.map, dest_id) }
    } else {
        0
    };
    if eff_prov == 0 {
        return Err(());
    }

    let mut dist = 0i32;
    if p.target_province != eff_prov {
        let target_id = unsafe { *((p.target_province + PROVINCE_ID) as *const i32) };
        if target_id < 0 || target_id > i16::MAX as i32 {
            return Err(());
        }
        let geom = unsafe { *((eff_prov + PROVINCE_GEOM) as *const usize) };
        if geom == 0 {
            return Err(());
        }
        let set_begin = unsafe { *((geom + GEOM_NEIGHBOR_SET_BEGIN) as *const usize) };
        let set_end = unsafe { *((geom + GEOM_NEIGHBOR_SET_END) as *const usize) };
        let len = ((set_end.wrapping_sub(set_begin)) as isize >> 2) as i32;
        let mut adjacent = false;
        if len > 0 {
            let mut slot = ((target_id.wrapping_mul(2)) as i64).rem_euclid(len as i64) as usize;
            loop {
                let entry = unsafe { *((set_begin + slot * 4) as *const i32) };
                if entry == target_id {
                    adjacent = true;
                    break;
                }
                if entry == 0 {
                    break;
                }
                let next = (slot as i32) + 1;
                slot = if next < len { next as usize } else { 0 };
            }
        }
        if !adjacent {
            let eff_id = unsafe { *((eff_prov + PROVINCE_ID) as *const i32) };
            if eff_id < 0 || eff_id > i16::MAX as i32 {
                return Err(());
            }
            if eff_id != target_id {
                let linear = unsafe { *((p.map + MAP_LINEAR_DISTANCE) as *const u8) };
                let centers = unsafe { *((p.map + MAP_PROVINCE_CENTERS) as *const usize) };
                if centers == 0 {
                    return Err(());
                }
                let p_unit = unsafe { *((centers + (eff_id as usize) * MAP_CENTER_STRIDE + MAP_CENTER_POS) as *const u32) };
                let p_target = unsafe { *((centers + (target_id as usize) * MAP_CENTER_STRIDE + MAP_CENTER_POS) as *const u32) };
                let map_w = unsafe { *((p.map + MAP_WIDTH) as *const u32) };
                let map_h = unsafe { *((p.map + MAP_HEIGHT) as *const u32) };
                type DistSq = unsafe extern "C" fn(*const u32, *const u32, u32, u32, u8) -> i32;
                let dist_fn: DistSq = unsafe { std::mem::transmute(m.at(MAP_DISTANCE_SQ)) };
                let raw = unsafe { dist_fn(&p_unit, &p_target, map_w, map_h, linear) };
                if linear == 0 {
                    let s = (raw as f64).sqrt() as f32;
                    let f = s * 1000.0 + 0.5;
                    dist = (if s < 0.0 { f.floor() } else { f }) as i32;
                } else {
                    dist = raw.wrapping_mul(1000);
                }
            }
        }
    }

    if dist > p.max_dist {
        return Ok(None);
    }

    let free = if unit_loc != 0 && p.target_province != 0 {
        let r_unit = unsafe { *((unit_loc + PROVINCE_REGION) as *const usize) };
        let r_target = unsafe { *((p.target_province + PROVINCE_REGION) as *const usize) };
        if r_unit != 0 && r_target != 0 {
            if r_target == r_unit {
                true
            } else {
                let mut cur = unsafe { *((r_target + 0x20) as *const usize) };
                let mut found = false;
                while cur != 0 {
                    if unsafe { *(cur as *const usize) } == r_unit {
                        found = true;
                        break;
                    }
                    cur = unsafe { *((cur + 0x18) as *const usize) };
                }
                found
            }
        } else {
            false
        }
    } else {
        false
    };

    Ok(Some((dist, free)))
}

/// Computes a unit's distance-scaled strength and leader fire+shock pips,
/// matching `0x106797..0x1068aa`.
unsafe fn scaled_unit_strength(m: &crate::native::memory::Memory, unit: usize, dist: i32, p: &ScanParams) -> (i64, i32) {
    let ratio_1000 = if p.max_dist == 0 { -1i32 } else { ((dist as i64 * 1000) / (p.max_dist as i64)) as i32 };
    let attached = unsafe { *((unit + ATTACHED_TO) as *const usize) };
    let mut leader_pips = 0i32;
    let mut strength = if attached != 0 && unsafe { *((attached + LOCATION) as *const usize) } == unsafe { *((unit + LOCATION) as *const usize) } {
        unsafe { *(m.at(ZERO_STRENGTH_CONST) as *const i64) }
    } else {
        let mut out = 0i64;
        type ArmyStr = unsafe extern "C" fn(usize, *mut i64, usize, u8, u8, *mut i32) -> *mut i64;
        let f: ArmyStr = unsafe { std::mem::transmute(m.at(ARMY_STRENGTH)) };
        unsafe { f(unit, &mut out, p.target_province, 0, 1, &mut leader_pips) };
        out
    };
    if dist > p.half_max {
        let denom = ((ratio_1000 as i64 + 1000) << 30) / FIXED_SCALE;
        strength = (strength << 15) / denom;
    }
    (strength, leader_pips)
}

/// Runs `0x96c520` with the exact policy and options layout built by `0x106320`
/// and frees any returned path nodes.
unsafe fn unit_can_reach(m: &crate::native::memory::Memory, unit: usize, start_province: usize, target_province: usize, route_arg: usize) -> bool {
    #[repr(C)]
    struct RouteList {
        head: usize,
        tail: usize,
        count: i32,
        flag: u8,
        pad: [u8; 3],
    }

    let policy: [usize; 1] = [m.at(ROUTE_POLICY)];
    let mut list = RouteList { head: 0, tail: 0, count: 0, flag: 0, pad: [0; 3] };
    let mut opts = [0u8; route_options::SIZE];
    opts[route_options::ONE_HOP..route_options::ONE_HOP + 2].copy_from_slice(&0x0101u16.to_le_bytes());
    opts[route_options::LIMIT..route_options::LIMIT + 4].copy_from_slice(&(-1i32).to_le_bytes());
    opts[route_options::TAG..route_options::TAG + 8].copy_from_slice(&route_options::NO_TAG.to_le_bytes());
    opts[0x38..0x40].copy_from_slice(&route_arg.to_le_bytes());

    type RouteQueryFn = unsafe extern "C" fn(usize, usize, usize, usize, *mut RouteList, *mut u8, usize, usize) -> u8;
    let query: RouteQueryFn = unsafe { std::mem::transmute(m.at(ROUTE_QUERY.rva())) };
    let reachable = unsafe { query(policy.as_ptr() as usize, unit, start_province, target_province, &mut list, opts.as_mut_ptr(), 0, 0) } != 0;

    let delete_fn: unsafe extern "C" fn(usize, usize) = unsafe { std::mem::transmute(m.at(DELETE.rva())) };
    let mut node = list.head;
    while node != 0 {
        let next = unsafe { *((node + 0x10) as *const usize) };
        unsafe { delete_fn(node, 0x20) };
        node = next;
    }
    reachable
}

fn max_leader_of(base: i32, slice: &[PendingUnit]) -> i32 {
    slice.iter().fold(base, |acc, u| acc.max(u.leader))
}

unsafe fn evaluate_support(m: &crate::native::memory::Memory, args: &SupportArgs) -> Option<bool> {
    if args.agent == 0 || args.target_province == 0 || args.self_army == 0 || args.strength_ptr.is_null() || args.share_ptr.is_null() {
        return None;
    }
    let mut l_min = 0i32;
    let attached = unsafe { *((args.self_army + ATTACHED_TO) as *const usize) };
    if attached == 0 || unsafe { *((attached + LOCATION) as *const usize) } != unsafe { *((args.self_army + LOCATION) as *const usize) } {
        let mut dummy = 0i64;
        type ArmyStr = unsafe extern "C" fn(usize, *mut i64, usize, u8, u8, *mut i32) -> *mut i64;
        let f: ArmyStr = unsafe { std::mem::transmute(m.at(ARMY_STRENGTH)) };
        unsafe { f(args.self_army, &mut dummy, args.target_province, 0, 1, &mut l_min) };
    }

    let country_mgr = unsafe { *(m.at(COUNTRY_MANAGER) as *const usize) };
    if country_mgr == 0 {
        return None;
    }
    let countries = unsafe { *((country_mgr + COUNTRY_LIST) as *const usize) };
    if countries == 0 {
        return None;
    }
    let agent_tag = unsafe { *((args.agent + AGENT_COUNTRY_TAG) as *const u64) };
    let agent_idx = (agent_tag >> tag::INDEX) as i16 as isize;
    if agent_idx < 0 {
        return None;
    }
    let agent_country = unsafe { *((countries as isize + agent_idx * 8) as *const usize) };
    if agent_country == 0 {
        return None;
    }

    let tech = unsafe { *((agent_country + TECHNOLOGY) as *const usize) };
    if tech == 0 {
        return None;
    }
    let width_scaled = unsafe { *((tech + TECH_COMBAT_WIDTH) as *const i32) }.wrapping_add(1000).wrapping_mul(20);
    let width_threshold = width_scaled / 1000;

    let prov_geom = unsafe { *((args.target_province + PROVINCE_GEOM) as *const usize) };
    if prov_geom == 0 {
        return None;
    }
    let terrain_type = unsafe { *((prov_geom + GEOM_TERRAIN_TYPE) as *const usize) };
    if terrain_type == 0 {
        return None;
    }
    let neg_terrain = -unsafe { *((terrain_type + TERRAIN_PENALTY) as *const i32) };

    let share = unsafe { *args.share_ptr };
    let share_q15 = ((share as i64) << 30) / FIXED_SCALE;

    let ctx = EvalContext { only_enemies: args.only_enemies != 0, width_threshold, neg_terrain, share_q15 };

    let map = unsafe { *(m.at(MAP) as *const usize) };
    if map == 0 {
        return None;
    }
    let max_dist = unsafe { *(m.at(MAX_SUPPORT_DISTANCE) as *const i32) };
    let scan = ScanParams {
        self_army: args.self_army,
        target_province: args.target_province,
        special: unsafe {
            [*(m.at(SPECIAL_COUNTRIES[0]) as *const i32), *(m.at(SPECIAL_COUNTRIES[1]) as *const i32), *(m.at(SPECIAL_COUNTRIES[2]) as *const i32)]
        },
        map,
        max_dist,
        half_max: ((max_dist as i64 * 1000) / 2000) as i32,
    };

    let base_f = unsafe { *(m.at(BASE_FRIENDLY_STRENGTH) as *const i64) + *args.strength_ptr };
    let mut bounds = Bounds {
        f_min: base_f,
        f_max: base_f,
        b_min: ENEMY_BASE_STRENGTH,
        b_max: ENEMY_BASE_STRENGTH,
        l_min,
        l_max: l_min,
        e_min: 0,
        e_max: 0,
        regs_min: 0,
        regs_max: 0,
        monotonic: true,
    };

    type RollFactorFn = unsafe extern "C" fn(*mut i32, i32, i32) -> *mut i32;
    let roll_fn: RollFactorFn = unsafe { std::mem::transmute(m.at(ROLL_FACTOR)) };
    let roll_factor = |diff: i32, terrain: i32| -> i32 {
        let mut out = 0i32;
        unsafe { roll_fn(&mut out, diff, terrain) };
        out
    };

    SCRATCH.with(|cell| {
        let mut scratch = cell.borrow_mut();
        scratch.friends.clear();
        scratch.enemies.clear();

        // Phase 1a: scan war enemies' units without running route queries yet.
        let mut enemy_country_node = unsafe { *((agent_country + COUNTRY_WAR_ENEMIES) as *const usize) };
        while enemy_country_node != 0 {
            let tag_val = unsafe { *(enemy_country_node as *const u64) };
            enemy_country_node = unsafe { *((enemy_country_node + 0x10) as *const usize) };
            let c_idx = (tag_val >> tag::INDEX) as i16 as isize;
            if c_idx < 0 {
                return None;
            }
            let country = unsafe { *((countries as isize + c_idx * 8) as *const usize) };
            if country == 0 {
                return None;
            }
            let mut unit_node = unsafe { *((country + UNITS) as *const usize) };
            while unit_node != 0 {
                let unit = unsafe { *(unit_node as *const usize) };
                unit_node = unsafe { *((unit_node + 0x10) as *const usize) };
                let Some((dist, free)) = (unsafe { check_unit_eligible(m, unit, false, &scan) }).ok()? else {
                    continue;
                };
                let (strength, leader) = unsafe { scaled_unit_strength(m, unit, dist, &scan) };
                if strength < 0 {
                    bounds.monotonic = false;
                }
                let reg_begin = unsafe { *((unit + REGIMENTS) as *const usize) };
                let reg_end = unsafe { *((unit + REGIMENTS + 8) as *const usize) };
                let regiments = ((reg_end.wrapping_sub(reg_begin)) as isize >> 3) as i32;
                if free {
                    bounds.b_min += strength;
                    bounds.b_max += strength;
                    bounds.e_min = bounds.e_min.max(leader);
                    bounds.e_max = bounds.e_max.max(leader);
                    bounds.regs_min += regiments;
                    bounds.regs_max += regiments;
                } else {
                    bounds.b_max += strength;
                    bounds.e_max = bounds.e_max.max(leader);
                    bounds.regs_max += regiments;
                    let start_province = unsafe { *((unit + LOCATION) as *const usize) };
                    scratch.enemies.push(PendingUnit { unit, start_province, strength, leader, regiments });
                }
            }
        }

        // Fast check before even scanning friends when only_enemies is set.
        if ctx.only_enemies {
            match decide_bounds(&bounds, &ctx, roll_factor) {
                BoundDecision::True => return Some(true),
                BoundDecision::False => return Some(false),
                BoundDecision::Undecided => {}
            }
        }

        // Phase 1b: scan own and allied units without running route queries yet.
        if !ctx.only_enemies {
            let mut scan_friendly_country = |country: usize| -> Result<(), ()> {
                let mut unit_node = unsafe { *((country + UNITS) as *const usize) };
                while unit_node != 0 {
                    let unit = unsafe { *(unit_node as *const usize) };
                    unit_node = unsafe { *((unit_node + 0x10) as *const usize) };
                    let Some((dist, free)) = (unsafe { check_unit_eligible(m, unit, true, &scan) })? else {
                        continue;
                    };
                    let (strength, leader) = unsafe { scaled_unit_strength(m, unit, dist, &scan) };
                    if strength < 0 {
                        bounds.monotonic = false;
                    }
                    if free {
                        bounds.f_min += strength;
                        bounds.f_max += strength;
                        bounds.l_min = bounds.l_min.max(leader);
                        bounds.l_max = bounds.l_max.max(leader);
                    } else {
                        bounds.f_max += strength;
                        bounds.l_max = bounds.l_max.max(leader);
                        let start_province = unsafe { *((unit + LOCATION) as *const usize) };
                        scratch.friends.push(PendingUnit { unit, start_province, strength, leader, regiments: 0 });
                    }
                }
                Ok(())
            };
            scan_friendly_country(agent_country).ok()?;
            let mut ally_node = unsafe { *((agent_country + COUNTRY_WAR_ALLIES) as *const usize) };
            while ally_node != 0 {
                let tag_val = unsafe { *(ally_node as *const u64) };
                ally_node = unsafe { *((ally_node + 0x10) as *const usize) };
                let c_idx = (tag_val >> tag::INDEX) as i16 as isize;
                if c_idx < 0 {
                    return None;
                }
                let ally_country = unsafe { *((countries as isize + c_idx * 8) as *const usize) };
                if ally_country == 0 {
                    return None;
                }
                scan_friendly_country(ally_country).ok()?;
            }
        }

        match decide_bounds(&bounds, &ctx, roll_factor) {
            BoundDecision::True => return Some(true),
            BoundDecision::False => return Some(false),
            BoundDecision::Undecided => {}
        }

        // Phase 2: resolve remaining route queries lazily, largest strength first.
        scratch.friends.sort_unstable_by_key(|u| std::cmp::Reverse(u.strength));
        scratch.enemies.sort_unstable_by_key(|u| std::cmp::Reverse(u.strength));

        let mut fi = 0usize;
        let mut ei = 0usize;
        while fi < scratch.friends.len() || ei < scratch.enemies.len() {
            let pick_friend = if fi >= scratch.friends.len() {
                false
            } else if ei >= scratch.enemies.len() {
                true
            } else {
                (bounds.f_max - bounds.f_min) >= (bounds.b_max - bounds.b_min)
            };

            if pick_friend {
                let item = scratch.friends[fi];
                fi += 1;
                let ok = unsafe { unit_can_reach(m, item.unit, item.start_province, args.target_province, args.route_arg) };
                if ok {
                    bounds.f_min += item.strength;
                    bounds.l_min = bounds.l_min.max(item.leader);
                } else {
                    bounds.f_max -= item.strength;
                    if item.leader == bounds.l_max && item.leader > bounds.l_min {
                        bounds.l_max = max_leader_of(bounds.l_min, &scratch.friends[fi..]);
                    }
                }
            } else {
                let item = scratch.enemies[ei];
                ei += 1;
                let ok = unsafe { unit_can_reach(m, item.unit, item.start_province, args.target_province, args.route_arg) };
                if ok {
                    bounds.b_min += item.strength;
                    bounds.e_min = bounds.e_min.max(item.leader);
                    bounds.regs_min += item.regiments;
                } else {
                    bounds.b_max -= item.strength;
                    bounds.regs_max -= item.regiments;
                    if item.leader == bounds.e_max && item.leader > bounds.e_min {
                        bounds.e_max = max_leader_of(bounds.e_min, &scratch.enemies[ei..]);
                    }
                }
            }

            match decide_bounds(&bounds, &ctx, roll_factor) {
                BoundDecision::True => return Some(true),
                BoundDecision::False => return Some(false),
                BoundDecision::Undecided => {}
            }
        }

        // All units resolved: exact final evaluation.
        if ctx.only_enemies && bounds.regs_min <= ctx.width_threshold {
            return Some(true);
        }
        // Guard against division by zero if negative modded strengths cancel out
        // `ENEMY_BASE_STRENGTH`; fall back to the game's original routine.
        if bounds.b_min <= 0 {
            return None;
        }
        let factor = roll_factor(bounds.l_min - bounds.e_min, ctx.neg_terrain);
        Some(eval_ratio(bounds.f_min, bounds.b_min, factor, ctx.share_q15))
    })
}

#[called_by_game]
unsafe extern "C" fn support_check(
    agent: usize,
    province: usize,
    army: usize,
    strength_ptr: *const i64,
    share_ptr: *const i32,
    route_arg: usize,
    only_enemies: u8,
    a7: usize,
) -> u8 {
    let m = crate::native::hooked_memory();
    let args = SupportArgs { agent, target_province: province, self_army: army, strength_ptr, share_ptr, route_arg, only_enemies };
    let Some(ours) = (unsafe { evaluate_support(&m, &args) }) else {
        return unsafe { ORIGINAL_SUPPORT_CHECK.get()(agent, province, army, strength_ptr, share_ptr, route_arg, only_enemies, a7) };
    };
    if speed::checked(&SPEED_UP) {
        let game = unsafe { ORIGINAL_SUPPORT_CHECK.get()(agent, province, army, strength_ptr, share_ptr, route_arg, only_enemies, a7) != 0 };
        speed::compared(&SPEED_UP, ours == game, || {
            format!("support check for army {army:#x} at province {province:#x} (only_enemies={only_enemies}): ours={ours}, game={game}")
        });
    }
    u8::from(ours)
}

/// Checked, every answer is compared against the game's original `0x106320`.
pub const SPEED_UP: SpeedUp = SpeedUp::new("support-checks", "ai", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            SUPPORT_CHECK,
            "48 8b c4 4c 89 40 18 48 89 50 10 48 89 48 08 55",
            "AI support check",
            support_check as SupportCheck,
            &ORIGINAL_SUPPORT_CHECK,
        ),
        Point::keep(ARMY_STRENGTH, "44 88 4c 24 20 4c 89 44 24 18 48 89 4c 24 08 55", "army strength evaluation"),
        Point::keep(ROLL_FACTOR, "48 89 5c 24 08 48 89 7c 24 10 33 ff 47 8d 1c 00", "support check combat roll factor"),
        Point::keep(MAP_DISTANCE_SQ, "40 53 48 81 ec 80 00 00 00 41 8b d8 4c 8b da 44", "squared map distance between two points"),
        Point::keep(PROVINCE_BY_ID, "48 89 5c 24 10 48 89 74 24 18 55 57 41 56 48 8d", "province lookup by ID"),
        Point::keep(0x1063c7usize, "4c 8b 3d 6a d7 ec 01", "support check base friendly strength").uses(&[BASE_FRIENDLY_STRENGTH]),
        Point::keep(0x106638usize, "8b 0d 3e 96 23 02", "support check max distance define").uses(&[MAX_SUPPORT_DISTANCE]),
        Point::keep(0x1067e9usize, "4c 8b 05 e0 1a 25 02", "support check zero strength constant").uses(&[ZERO_STRENGTH_CONST]),
        Point::keep(0x1068f9usize, "4d 8b b6 c0 1b 00 00", "country war allies list").uses(&[COUNTRY_WAR_ALLIES]),
        Point::keep(0x106e42usize, "4d 8b b6 88 1b 00 00", "country war enemies list").uses(&[COUNTRY_WAR_ENEMIES]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_roll(diff: i32, _terrain: i32) -> i32 {
        let num = diff.max(0) + 15;
        let den = (-diff).max(0) + 15;
        (num * 1000) / den
    }

    #[test]
    fn bounds_decide_true_and_false_without_waiting_for_remaining_units() {
        let ctx = EvalContext { only_enemies: false, width_threshold: 20, neg_terrain: 0, share_q15: (1000i64 << 30) / FIXED_SCALE };

        // Guaranteed true: even minimal friends beat maximal enemies and worst leader diff.
        let b_true = Bounds {
            f_min: 100_000,
            f_max: 150_000,
            b_min: 30_000,
            b_max: 50_000,
            l_min: 4,
            l_max: 6,
            e_min: 0,
            e_max: 4,
            regs_min: 5,
            regs_max: 10,
            monotonic: true,
        };
        assert_eq!(decide_bounds(&b_true, &ctx, fake_roll), BoundDecision::True);

        // Guaranteed false: even maximal friends and best leader cannot beat minimal enemies.
        let b_false = Bounds {
            f_min: 10_000,
            f_max: 20_000,
            b_min: 100_000,
            b_max: 140_000,
            l_min: 0,
            l_max: 2,
            e_min: 4,
            e_max: 6,
            regs_min: 30,
            regs_max: 40,
            monotonic: true,
        };
        assert_eq!(decide_bounds(&b_false, &ctx, fake_roll), BoundDecision::False);

        // Undecided when interval straddles the threshold.
        let b_mid = Bounds {
            f_min: 30_000,
            f_max: 90_000,
            b_min: 50_000,
            b_max: 70_000,
            l_min: 2,
            l_max: 4,
            e_min: 2,
            e_max: 4,
            regs_min: 10,
            regs_max: 20,
            monotonic: true,
        };
        assert_eq!(decide_bounds(&b_mid, &ctx, fake_roll), BoundDecision::Undecided);
    }

    #[test]
    fn only_enemies_respects_combat_width_auto_true_threshold() {
        let ctx = EvalContext { only_enemies: true, width_threshold: 25, neg_terrain: 0, share_q15: (1000i64 << 30) / FIXED_SCALE };

        // Even if enemies are stronger, regs_max <= width_threshold forces true.
        let auto_true = Bounds {
            f_min: 1_000,
            f_max: 1_000,
            b_min: 500_000,
            b_max: 500_000,
            l_min: 0,
            l_max: 0,
            e_min: 6,
            e_max: 6,
            regs_min: 10,
            regs_max: 25,
            monotonic: true,
        };
        assert_eq!(decide_bounds(&auto_true, &ctx, fake_roll), BoundDecision::True);

        // Cannot declare false until regs_min strictly exceeds width_threshold.
        let still_undecided = Bounds { regs_min: 20, regs_max: 35, ..auto_true };
        assert_eq!(decide_bounds(&still_undecided, &ctx, fake_roll), BoundDecision::Undecided);

        // Once regs_min > width_threshold and ratio fails, false is certain.
        let certain_false = Bounds { regs_min: 26, regs_max: 35, ..auto_true };
        assert_eq!(decide_bounds(&certain_false, &ctx, fake_roll), BoundDecision::False);
    }
}
