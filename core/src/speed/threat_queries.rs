//! Read-only AI-stage snapshot for threat-map queries (`0x1998f0` and
//! `0x199500`, `docs/game/ai/ai.md`).
//!
//! Right before `0x77d8a0` dispatches parallel AI workers in `0x75b4b0`,
//! `0x1950e0` finishes updating the global threat map at `0x242ba00`. Throughout
//! `0x77d8a0`, the threat map's land/naval province buckets (`+0x00`, `+0x18`),
//! its unit-stamp hash map (`+0x30`), all live units, their strengths
//! (`0x177df0` -> `0xd46800` / `0xd462b0`), country enemy lists (`+0x1548`), and
//! diplomatic relations (`+0x1510`) remain read-only.
//!
//! Vanilla `0x1998f0` (army threat ratio, called from `0x198c70`) and
//! `0x199500` (recruitment threat sum, called from `0x198b00`) repeatedly hash
//! into `0x242ba00+0x30`, resolve every entry through `0x14cf730`, call
//! `vtable+0x138` twice per entry, run an unrolled float power loop for
//! `0.75^dist` / `0.5^dist`, and look up army strength in each AI country's
//! separate `0x177df0` hash map.
//!
//! Building one immutable [`ThreatSnapshot`] on the main thread at `0x77d8a0`
//! entry strips stale/dead entries from every bucket once (preserving exact
//! bucket order so IEEE-754 summation matches bit for bit), resolves each live
//! unit's pointer and owner tag once, shares lazy `OnceLock<i64>` strength
//! across all AI workers, replaces the power loop with a bit-exact lookup
//! table, and caches pure `0x197540` neutral-threat checks per worker thread.
//!
//! Checked mode (`speed::checked`) compares every query result against the
//! game's original `0x1998f0` / `0x199500`.
use super::support_checks::{ARMY_STRENGTH, ZERO_STRENGTH_CONST};
use crate::base::called_by_game;
use crate::native::objects::{
    army_rva::FIND_ID,
    campaign::{
        countries::LIST as COUNTRY_LIST,
        functions::COUNTRY_MODIFIERS,
        globals::{COUNTRY_MANAGER, SPECIAL_COUNTRIES},
        unit::{ATTACHED_TO, LOCATION, OWNER},
    },
};
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    LazyLock, OnceLock,
};

#[derive(Default)]
struct FastHash(u64);

impl Hasher for FastHash {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0 ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    #[inline]
    fn write_i16(&mut self, i: i16) {
        self.0 = (self.0 ^ (i as u16 as u64)).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHash>>;

/// Parallel AI stage dispatcher (`0x77d8a0`, called solely from `0x75ba6e` in `0x75b4b0`).
const AI_STAGE: usize = 0x77d8a0;
/// Threat-map query functions: army threat ratio (`0x1998f0`) and recruitment threat sum (`0x199500`).
const THREAT_RATIO_QUERY: usize = 0x1998f0;
const THREAT_SUM_QUERY: usize = 0x199500;
/// Global threat map updated by `0x1950e0` immediately before `0x77d8a0`.
const THREAT_MAP: usize = 0x242ba00;
const THREAT_MAP_LAND: usize = 0x00;
const THREAT_MAP_NAVAL: usize = 0x18;
const THREAT_MAP_STAMPS: usize = 0x30;
const THREAT_MAP_INITIALIZED: usize = 0x74;
/// Virtual strength evaluator (`0x177df0`, vtable slot `+8` of `param_5[2]`).
const STRENGTH_EVAL_FN: usize = 0x177df0;
/// Underlying pure naval strength evaluator called by `0x177df0`.
const FLEET_STRENGTH: usize = 0xd462b0;
const COUNTRY_CACHED_DATA: usize = 0x28c0;
/// Pure neutral-country friendliness check called when `param_5[1] != 0`.
const POTENTIAL_ALLY_FN: usize = 0x197540;

const UNIT_IS_FLEET_SLOT: usize = 0x88;
const UNIT_OWNER_TAG_SLOT: usize = 0x138;
const PROVINCE_GEOM: usize = 0x28;
const GEOM_TERRAIN_TYPE: usize = 0x30;
const TERRAIN_INLAND_SEA: usize = 0x109;
const GEOM_FLAGS: usize = 0x228;

const COUNTRY_TAG: usize = 0x20;
const COUNTRY_TAG_INDEX: usize = 0x24;
const COUNTRY_RELATIONS: usize = 0x1510;
const COUNTRY_RELATION_STRIDE: usize = 0x48;
const COUNTRY_RELATION_FRIENDLY: usize = 0x1c;
const COUNTRY_ENEMIES_PTR: usize = 0x1548;
const COUNTRY_ENEMIES_COUNT: usize = 0x1554;

const BUCKET_STRIDE: usize = 0x18;
const RAW_ENTRY_STRIDE: usize = 0x10;

/// Exact IEEE-754 constants loaded by `0x1998f0` and `0x199500`.
const SCALE_Q15: f32 = f32::from_bits(0x3800_0000); // 1.0 / 32768.0
const SCALE_001: f32 = f32::from_bits(0x3c23_d70a); // 0.01f

type AiStageFn = unsafe extern "C" fn(usize, usize);
type ThreatQueryFn = unsafe extern "C" fn(*mut i32, usize, i32, usize, usize) -> *mut i32;

static ORIGINAL_AI_STAGE: Function<AiStageFn> = Function::new();
static ORIGINAL_THREAT_RATIO: Function<ThreatQueryFn> = Function::new();
static ORIGINAL_THREAT_SUM: Function<ThreatQueryFn> = Function::new();

/// Active immutable snapshot for the current `0x77d8a0` AI stage (0 outside it).
/// Written on the main thread before workers start and cleared after `0x77d8a0`
/// joins all workers, so worker reads require only a single `Acquire` load.
static SNAPSHOT_PTR: AtomicUsize = AtomicUsize::new(0);
static SNAPSHOT_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Precomputed `[factor_idx][shifted][dist]` table (`0 => 0.75`, `1 => 0.5`)
/// generated with the exact `f32` multiplication sequence of `0x1998f0` / `0x199500`.
static POW_TABLE: LazyLock<[[[f32; 256]; 2]; 2]> = LazyLock::new(build_pow_table);

fn game_pow_weight(factor: f32, dist: u8, shifted: bool) -> f32 {
    let mut val = 1.0_f32;
    let mut count = dist as i32;
    if shifted {
        count -= 1;
        if count >= 0 {
            val = factor * 0.5_f32 + 0.5_f32;
        }
    }
    let mut i = 0i32;
    if count > 7 {
        let chunks = (((count as u32) - 8) >> 3) + 1;
        i = (chunks * 8) as i32;
        for _ in 0..chunks {
            val *= factor;
            val *= factor;
            val *= factor;
            val *= factor;
            val *= factor;
            val *= factor;
            val *= factor;
            val *= factor;
        }
    }
    while i < count {
        val *= factor;
        i += 1;
    }
    val
}

fn build_pow_table() -> [[[f32; 256]; 2]; 2] {
    let mut table = [[[0.0_f32; 256]; 2]; 2];
    for (fi, &factor) in [0.75_f32, 0.5_f32].iter().enumerate() {
        for (si, &shifted) in [false, true].iter().enumerate() {
            for (d, slot) in table[fi][si].iter_mut().enumerate() {
                *slot = game_pow_weight(factor, d as u8, shifted);
            }
        }
    }
    table
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CompactEntry {
    days: i16,
    dist: u8,
    quarter_flag: u8,
    unit_idx: u32,
}

struct LiveUnit {
    unit_ptr: usize,
    owner_idx: i16,
    owner_is_special: bool,
    owner_can_shift: bool,
    factor_idx: u8,
    strength: OnceLock<i64>,
}

impl LiveUnit {
    #[inline]
    fn strength(&self, m: &crate::native::memory::Memory) -> i64 {
        *self.strength.get_or_init(|| unsafe { compute_unit_strength(m, self.unit_ptr) })
    }
}

struct ThreatSnapshot {
    generation: u64,
    stamp_map_addr: usize,
    strength_fn_addr: usize,
    countries: usize,
    special: [i32; 3],
    land_begin: usize,
    land_end: usize,
    land_offsets: Vec<u32>,
    land_entries: Vec<CompactEntry>,
    naval_begin: usize,
    naval_end: usize,
    naval_offsets: Vec<u32>,
    naval_entries: Vec<CompactEntry>,
    units: Vec<LiveUnit>,
}

impl ThreatSnapshot {
    #[inline]
    fn is_special(&self, idx: i32) -> bool {
        idx == self.special[0] || idx == self.special[1] || idx == self.special[2]
    }

    #[inline]
    fn bucket_entries(&self, bucket_ptr: usize) -> Option<&[CompactEntry]> {
        if bucket_ptr >= self.land_begin && bucket_ptr < self.land_end {
            let off = bucket_ptr - self.land_begin;
            if off.is_multiple_of(BUCKET_STRIDE) {
                let idx = off / BUCKET_STRIDE;
                let start = self.land_offsets[idx] as usize;
                let end = self.land_offsets[idx + 1] as usize;
                return Some(&self.land_entries[start..end]);
            }
        } else if bucket_ptr >= self.naval_begin && bucket_ptr < self.naval_end {
            let off = bucket_ptr - self.naval_begin;
            if off.is_multiple_of(BUCKET_STRIDE) {
                let idx = off / BUCKET_STRIDE;
                let start = self.naval_offsets[idx] as usize;
                let end = self.naval_offsets[idx + 1] as usize;
                return Some(&self.naval_entries[start..end]);
            }
        }
        None
    }
}

#[derive(Default)]
struct AllyCache {
    generation: u64,
    map: FastMap<(i16, i16), bool>,
}

thread_local! {
    static ALLY_CACHE: RefCell<AllyCache> = RefCell::new(AllyCache::default());
}

#[inline]
unsafe fn potential_ally_cached(
    m: &crate::native::memory::Memory,
    generation: u64,
    q_owner_idx: i16,
    q_country: usize,
    target_idx: i16,
    countries: usize,
) -> bool {
    ALLY_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if cache.generation != generation {
            cache.map.clear();
            cache.generation = generation;
        }
        if let Some(&val) = cache.map.get(&(q_owner_idx, target_idx)) {
            return val;
        }
        let target_country = unsafe { *((countries as isize + (target_idx as isize) * 8) as *const usize) };
        type PotentialAlly = unsafe extern "C" fn(usize, usize) -> u8;
        let f: PotentialAlly = unsafe { std::mem::transmute(m.at(POTENTIAL_ALLY_FN)) };
        let val = unsafe { f(q_country, target_country) } != 0;
        cache.map.insert((q_owner_idx, target_idx), val);
        val
    })
}

#[inline]
unsafe fn unit_owner_tag(unit: usize) -> u64 {
    let mut tag = 0u64;
    let vtable = unsafe { *(unit as *const usize) };
    let get_tag: unsafe extern "C" fn(usize, *mut u64) -> *mut u64 = unsafe { std::mem::transmute(*((vtable + UNIT_OWNER_TAG_SLOT) as *const usize)) };
    unsafe { get_tag(unit, &mut tag) };
    tag
}

#[inline]
unsafe fn is_country_enemy(country: usize, target_idx: i16) -> bool {
    let count = unsafe { *((country + COUNTRY_ENEMIES_COUNT) as *const i32) };
    if count <= 0 {
        return false;
    }
    let ptr = unsafe { *((country + COUNTRY_ENEMIES_PTR) as *const usize) };
    let target = target_idx as i32;
    let mut cur = ptr;
    let end = ptr + (count as usize) * 16;
    while cur != end {
        if unsafe { *(cur as *const i32) } == target {
            return true;
        }
        cur += 16;
    }
    false
}

/// Pure unit-strength computation matching `0x177df0` for both land and naval units.
unsafe fn compute_unit_strength(m: &crate::native::memory::Memory, unit: usize) -> i64 {
    let vtable = unsafe { *(unit as *const usize) };
    let is_fleet: unsafe extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(*((vtable + UNIT_IS_FLEET_SLOT) as *const usize)) };
    if unsafe { is_fleet(unit) } == 0 {
        let attached = unsafe { *((unit + ATTACHED_TO) as *const usize) };
        let loc = unsafe { *((unit + LOCATION) as *const usize) };
        if attached != 0 && unsafe { *((attached + LOCATION) as *const usize) } == loc {
            unsafe { *(m.at(ZERO_STRENGTH_CONST) as *const i64) }
        } else {
            let mut out = 0i64;
            let mut pips = 0i32;
            type ArmyStr = unsafe extern "C" fn(usize, *mut i64, usize, u8, u8, *mut i32) -> *mut i64;
            let f: ArmyStr = unsafe { std::mem::transmute(m.at(ARMY_STRENGTH)) };
            unsafe { f(unit, &mut out, 0, 1, 1, &mut pips) };
            out
        }
    } else {
        let loc = unsafe { *((unit + LOCATION) as *const usize) };
        let geom = unsafe { *((loc + PROVINCE_GEOM) as *const usize) };
        let terrain_type = unsafe { *((geom + GEOM_TERRAIN_TYPE) as *const usize) };
        let flag: u8 = if unsafe { *((terrain_type + TERRAIN_INLAND_SEA) as *const u8) } != 0 {
            2
        } else {
            let b = unsafe { *((geom + GEOM_FLAGS) as *const u8) };
            if (b as i8) < 0 {
                1
            } else if (b & 8) != 0 {
                3
            } else {
                0
            }
        };
        let owner_idx = ((unsafe { *((unit + OWNER) as *const u64) }) >> 32) as i16 as isize;
        let country_mgr = unsafe { *(m.at(COUNTRY_MANAGER) as *const usize) };
        let countries = unsafe { *((country_mgr + COUNTRY_LIST) as *const usize) };
        let owner_country = unsafe { *((countries as isize + owner_idx * 8) as *const usize) };
        type CountryMods = unsafe extern "C" fn(usize) -> usize;
        let get_mods: CountryMods = unsafe { std::mem::transmute(m.at(COUNTRY_MODIFIERS.rva())) };
        let mods = unsafe { get_mods(owner_country + COUNTRY_CACHED_DATA) };
        let mut out = 0i64;
        let mut dummy = 0i32;
        type FleetStr = unsafe extern "C" fn(usize, *mut i64, usize, u8, u8, u8, *mut i32, u8, i32, usize) -> *mut i64;
        let f: FleetStr = unsafe { std::mem::transmute(m.at(FLEET_STRENGTH)) };
        unsafe { f(unit, &mut out, mods + 8, 1, 0, flag, &mut dummy, 0, 7, 0) };
        out
    }
}

/// Compacts one province-bucket vector (`land` or `naval`) into dense live
/// entries, preserving exact per-bucket order.
unsafe fn compact_buckets(begin: usize, end: usize, live_map: &FastMap<u64, (u8, u32)>) -> (Vec<u32>, Vec<CompactEntry>) {
    let count = end.saturating_sub(begin) / BUCKET_STRIDE;
    let mut offsets = Vec::with_capacity(count + 1);
    let mut entries = Vec::with_capacity(count * 4);
    offsets.push(0);
    for idx in 0..count {
        let bucket = begin + idx * BUCKET_STRIDE;
        let e_begin = unsafe { *(bucket as *const usize) };
        let e_end = unsafe { *((bucket + 8) as *const usize) };
        let n = e_end.saturating_sub(e_begin) / RAW_ENTRY_STRIDE;
        let mut cur = e_begin;
        for _ in 0..n {
            let unit_id = unsafe { std::ptr::read_unaligned((cur + 4) as *const u64) };
            let stamp = unsafe { *((cur + 3) as *const u8) };
            if let Some(&(live_stamp, unit_idx)) = live_map.get(&unit_id) {
                if stamp == live_stamp {
                    let days = unsafe { *(cur as *const i16) };
                    let dist = unsafe { *((cur + 2) as *const u8) };
                    let quarter_flag = unsafe { *((cur + 0xc) as *const u8) };
                    entries.push(CompactEntry { days, dist, quarter_flag, unit_idx });
                }
            }
            cur += RAW_ENTRY_STRIDE;
        }
        offsets.push(entries.len() as u32);
    }
    (offsets, entries)
}

unsafe fn build_snapshot(m: &crate::native::memory::Memory) -> Option<Box<ThreatSnapshot>> {
    let threat_map = m.at(THREAT_MAP);
    if unsafe { *((threat_map + THREAT_MAP_INITIALIZED) as *const u8) } == 0 {
        return None;
    }
    let country_mgr = unsafe { *(m.at(COUNTRY_MANAGER) as *const usize) };
    if country_mgr == 0 {
        return None;
    }
    let countries = unsafe { *((country_mgr + COUNTRY_LIST) as *const usize) };
    if countries == 0 {
        return None;
    }

    let special =
        unsafe { [*(m.at(SPECIAL_COUNTRIES[0]) as *const i32), *(m.at(SPECIAL_COUNTRIES[1]) as *const i32), *(m.at(SPECIAL_COUNTRIES[2]) as *const i32)] };
    let is_special = |idx: i32| idx == special[0] || idx == special[1] || idx == special[2];

    let stamp_map = threat_map + THREAT_MAP_STAMPS;
    let sentinel = unsafe { *((stamp_map + 8) as *const usize) };
    let map_size = unsafe { *((stamp_map + 0x10) as *const usize) };
    if sentinel == 0 {
        return None;
    }

    type FindId = unsafe extern "C" fn(*const u64) -> usize;
    let find_id: FindId = unsafe { std::mem::transmute(m.at(FIND_ID.rva())) };

    let mut units = Vec::with_capacity(map_size);
    let mut live_map: FastMap<u64, (u8, u32)> = FastMap::with_capacity_and_hasher(map_size, Default::default());

    let mut node = unsafe { *(sentinel as *const usize) };
    while node != 0 && node != sentinel {
        let unit_id = unsafe { *((node + 0x10) as *const u64) };
        let stamp = unsafe { *((node + 0x18) as *const u8) };
        let found = unsafe { find_id(&unit_id) };
        if found != 0 {
            let unit_ptr = found - 0x10;
            let raw_owner = ((unsafe { *((unit_ptr + OWNER) as *const u64) }) >> 32) as i16 as i32;
            let factor_idx = u8::from(is_special(raw_owner));
            let tag = unsafe { unit_owner_tag(unit_ptr) };
            let owner_idx = (tag >> 32) as i16;
            let owner_b6 = (tag >> 48) as u8;
            let owner_b7 = (tag >> 56) as u8;
            let owner_is_special = is_special(owner_idx as i32);
            let owner_can_shift = owner_b7 != 0 && owner_b6 != 7 && !owner_is_special;

            let unit_idx = units.len() as u32;
            units.push(LiveUnit { unit_ptr, owner_idx, owner_is_special, owner_can_shift, factor_idx, strength: OnceLock::new() });
            live_map.insert(unit_id, (stamp, unit_idx));
        }
        node = unsafe { *(node as *const usize) };
    }

    let land_begin = unsafe { *((threat_map + THREAT_MAP_LAND) as *const usize) };
    let land_end = unsafe { *((threat_map + THREAT_MAP_LAND + 8) as *const usize) };
    let naval_begin = unsafe { *((threat_map + THREAT_MAP_NAVAL) as *const usize) };
    let naval_end = unsafe { *((threat_map + THREAT_MAP_NAVAL + 8) as *const usize) };

    let (land_offsets, land_entries) = unsafe { compact_buckets(land_begin, land_end, &live_map) };
    let (naval_offsets, naval_entries) = unsafe { compact_buckets(naval_begin, naval_end, &live_map) };

    let generation = SNAPSHOT_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    // Ensure POW_TABLE is initialized before parallel workers start reading it.
    let _ = &*POW_TABLE;

    Some(Box::new(ThreatSnapshot {
        generation,
        stamp_map_addr: stamp_map,
        strength_fn_addr: m.at(STRENGTH_EVAL_FN),
        countries,
        special,
        land_begin,
        land_end,
        land_offsets,
        land_entries,
        naval_begin,
        naval_end,
        naval_offsets,
        naval_entries,
        units,
    }))
}

struct SnapshotGuard {
    raw: usize,
}

impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        if self.raw != 0 {
            SNAPSHOT_PTR.store(0, Ordering::Release);
            unsafe { drop(Box::from_raw(self.raw as *mut ThreatSnapshot)) };
        }
    }
}

#[called_by_game]
unsafe extern "C" fn ai_stage_hook(a0: usize, a1: usize) {
    let _guard = if SNAPSHOT_PTR.load(Ordering::Acquire) == 0 {
        let m = crate::native::hooked_memory();
        if let Some(snap) = unsafe { build_snapshot(&m) } {
            let raw = Box::into_raw(snap) as usize;
            SNAPSHOT_PTR.store(raw, Ordering::Release);
            SnapshotGuard { raw }
        } else {
            SnapshotGuard { raw: 0 }
        }
    } else {
        SnapshotGuard { raw: 0 }
    };
    unsafe { ORIGINAL_AI_STAGE.get()(a0, a1) };
}

#[inline]
unsafe fn eval_strength_fn_matches(snap: &ThreatSnapshot, eval_obj: usize) -> bool {
    if eval_obj == 0 {
        return false;
    }
    let vtable = unsafe { *(eval_obj as *const usize) };
    vtable != 0 && unsafe { *((vtable + 8) as *const usize) } == snap.strength_fn_addr
}

#[inline]
fn round_threat_ratio(enemy_sum: f32, friendly_sum: f32, self_strength: f32) -> i32 {
    if self_strength <= 0.0_f32 {
        1_000_000_000
    } else {
        let ratio = enemy_sum / (friendly_sum + self_strength);
        let scaled = ratio * 1000.0_f32 + 0.5_f32;
        let rounded = if ratio < 0.0_f32 { scaled.floor() } else { scaled };
        rounded as i32
    }
}

#[inline]
fn round_threat_sum(threat_sum: f32) -> i32 {
    let scaled = threat_sum * 1000.0_f32 + 0.5_f32;
    let rounded = if threat_sum < 0.0_f32 { scaled.floor() } else { scaled };
    rounded as i32
}

unsafe fn eval_threat_ratio(m: &crate::native::memory::Memory, snap: &ThreatSnapshot, entries: &[CompactEntry], max_days: i32, ctx: usize) -> i32 {
    let self_strength = unsafe { *((ctx + 0x18) as *const f32) };
    if entries.is_empty() {
        return round_threat_ratio(0.0, 0.0, self_strength);
    }
    let query_unit = unsafe { *(ctx as *const usize) };
    let include_neutral_quarter = unsafe { *((ctx + 8) as *const u8) } != 0;

    let q_tag = unsafe { unit_owner_tag(query_unit) };
    let q_owner_idx = (q_tag >> 32) as i16;
    let q_country = unsafe { *((snap.countries as isize + (q_owner_idx as isize) * 8) as *const usize) };
    let q_country_tag_idx = (unsafe { *((q_country + COUNTRY_TAG) as *const u64) } >> 32) as i16;
    let q_country_24 = unsafe { *((q_country + COUNTRY_TAG_INDEX) as *const i16) };
    let q_is_special = snap.is_special(q_country_24 as i32);
    let relations_ptr = unsafe { *((q_country + COUNTRY_RELATIONS) as *const usize) };

    let pow = &*POW_TABLE;
    let mut enemy_sum = 0.0_f32;
    let mut friendly_sum = 0.0_f32;

    for entry in entries {
        if (entry.days as i32) > max_days {
            continue;
        }
        let u = &snap.units[entry.unit_idx as usize];
        if u.unit_ptr == query_unit {
            continue;
        }
        let enemy_or_special = u.owner_idx != q_country_tag_idx && (u.owner_is_special || q_is_special || unsafe { is_country_enemy(q_country, u.owner_idx) });
        let shifted = u.owner_can_shift && enemy_or_special;
        let mut weight = pow[u.factor_idx as usize][usize::from(shifted)][entry.dist as usize];
        if entry.quarter_flag != 0 {
            weight *= 0.25_f32;
        }
        let strength = u.strength(m);
        let contrib = ((strength as f32) * SCALE_Q15) * weight;

        if enemy_or_special {
            enemy_sum += contrib;
        } else {
            let is_friendly = q_owner_idx == u.owner_idx
                || unsafe { *((relations_ptr + COUNTRY_RELATION_FRIENDLY + (u.owner_idx as i32 as u32 as usize) * COUNTRY_RELATION_STRIDE) as *const i16) }
                    != 0;
            if is_friendly {
                friendly_sum += contrib;
            } else if include_neutral_quarter && !unsafe { potential_ally_cached(m, snap.generation, q_owner_idx, q_country, u.owner_idx, snap.countries) } {
                enemy_sum += contrib * 0.25_f32;
            }
        }
    }

    round_threat_ratio(enemy_sum, friendly_sum, self_strength)
}

unsafe fn eval_threat_sum(m: &crate::native::memory::Memory, snap: &ThreatSnapshot, entries: &[CompactEntry], max_days: i32, ctx: usize) -> i32 {
    if entries.is_empty() {
        return 0;
    }
    let q_owner_idx = (unsafe { *(ctx as *const u64) } >> 32) as i16;
    let include_neutral_quarter = unsafe { *((ctx + 8) as *const u8) } != 0;
    let q_country = unsafe { *((snap.countries as isize + (q_owner_idx as isize) * 8) as *const usize) };
    let q_country_tag_idx = (unsafe { *((q_country + COUNTRY_TAG) as *const u64) } >> 32) as i16;
    let q_country_24 = unsafe { *((q_country + COUNTRY_TAG_INDEX) as *const i16) };
    let q_is_special = snap.is_special(q_country_24 as i32);

    let pow = &*POW_TABLE;
    let mut threat_sum = 0.0_f32;

    for entry in entries {
        if (entry.days as i32) > max_days {
            continue;
        }
        let u = &snap.units[entry.unit_idx as usize];
        let mut weight = pow[u.factor_idx as usize][0][entry.dist as usize];
        if entry.quarter_flag != 0 {
            weight *= 0.25_f32;
        }
        let strength = u.strength(m);
        let contrib = (((strength as f32) * SCALE_Q15) * SCALE_001) * weight;

        let is_hostile = u.owner_idx != q_country_tag_idx && (u.owner_is_special || q_is_special || unsafe { is_country_enemy(q_country, u.owner_idx) });
        if is_hostile {
            threat_sum += contrib;
        } else if include_neutral_quarter && !unsafe { potential_ally_cached(m, snap.generation, q_owner_idx, q_country, u.owner_idx, snap.countries) } {
            threat_sum += contrib * 0.25_f32;
        }
    }

    round_threat_sum(threat_sum)
}

#[called_by_game]
unsafe extern "C" fn threat_ratio_query(out: *mut i32, bucket_ptr: usize, max_days: i32, stamp_map: usize, ctx: usize) -> *mut i32 {
    let ptr = SNAPSHOT_PTR.load(Ordering::Acquire);
    if ptr != 0 {
        let snap = unsafe { &*(ptr as *const ThreatSnapshot) };
        if stamp_map == snap.stamp_map_addr && unsafe { eval_strength_fn_matches(snap, *((ctx + 0x10) as *const usize)) } {
            if let Some(entries) = snap.bucket_entries(bucket_ptr) {
                let m = crate::native::hooked_memory();
                let ours = unsafe { eval_threat_ratio(&m, snap, entries, max_days, ctx) };
                if speed::checked(&SPEED_UP) {
                    let mut game = 0i32;
                    unsafe { ORIGINAL_THREAT_RATIO.get()(&mut game, bucket_ptr, max_days, stamp_map, ctx) };
                    speed::compared(&SPEED_UP, ours == game, || {
                        format!("threat ratio bucket {bucket_ptr:#x} (max_days={max_days}): snapshot={ours}, game={game}")
                    });
                }
                unsafe { *out = ours };
                return out;
            }
        }
    }
    unsafe { ORIGINAL_THREAT_RATIO.get()(out, bucket_ptr, max_days, stamp_map, ctx) }
}

#[called_by_game]
unsafe extern "C" fn threat_sum_query(out: *mut i32, bucket_ptr: usize, max_days: i32, stamp_map: usize, ctx: usize) -> *mut i32 {
    let ptr = SNAPSHOT_PTR.load(Ordering::Acquire);
    if ptr != 0 {
        let snap = unsafe { &*(ptr as *const ThreatSnapshot) };
        if stamp_map == snap.stamp_map_addr && unsafe { eval_strength_fn_matches(snap, *((ctx + 0x10) as *const usize)) } {
            if let Some(entries) = snap.bucket_entries(bucket_ptr) {
                let m = crate::native::hooked_memory();
                let ours = unsafe { eval_threat_sum(&m, snap, entries, max_days, ctx) };
                if speed::checked(&SPEED_UP) {
                    let mut game = 0i32;
                    unsafe { ORIGINAL_THREAT_SUM.get()(&mut game, bucket_ptr, max_days, stamp_map, ctx) };
                    speed::compared(&SPEED_UP, ours == game, || {
                        format!("threat sum bucket {bucket_ptr:#x} (max_days={max_days}): snapshot={ours}, game={game}")
                    });
                }
                unsafe { *out = ours };
                return out;
            }
        }
    }
    unsafe { ORIGINAL_THREAT_SUM.get()(out, bucket_ptr, max_days, stamp_map, ctx) }
}

/// Checked, every snapshot threat query is compared against the game's original
/// `0x1998f0` / `0x199500`.
pub const SPEED_UP: SpeedUp = SpeedUp::new("threat-queries", "ai", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(AI_STAGE, "48 89 5c 24 10 48 89 74 24 20 55 57 41 54 41 56 41 57", "parallel AI stage", ai_stage_hook as AiStageFn, &ORIGINAL_AI_STAGE),
        Point::replace(
            THREAT_RATIO_QUERY,
            "48 8b c4 4c 89 48 20 44 89 40 18 48 89 50 10 48 89 48 08",
            "threat map army threat ratio query",
            threat_ratio_query as ThreatQueryFn,
            &ORIGINAL_THREAT_RATIO,
        ),
        Point::replace(
            THREAT_SUM_QUERY,
            "4c 8b dc 49 89 4b 08 53 48 81 ec 00 01 00 00",
            "threat map recruitment threat sum query",
            threat_sum_query as ThreatQueryFn,
            &ORIGINAL_THREAT_SUM,
        ),
        Point::keep(0x75b543, "48 8d 0d b6 04 cd 01 e8 91 9b a3 ff", "AI phase updates global threat map").uses(&[THREAT_MAP]),
        Point::keep(STRENGTH_EVAL_FN, "48 89 5c 24 10 48 89 6c 24 20 4c 89 44 24 18 56 57 41 56", "AI cached unit strength evaluator"),
        Point::keep(FLEET_STRENGTH, "48 89 5c 24 18 44 88 4c 24 20 48 89 54 24 10 48 89 4c 24 08", "fleet strength evaluation"),
        Point::keep(POTENTIAL_ALLY_FN, "48 89 6c 24 18 48 89 74 24 20 57 48 83 ec 20 48 8b 41 20", "threat query potential ally check"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow_table_matches_game_unrolled_loop_bit_for_bit() {
        let table = build_pow_table();
        for (fi, &factor) in [0.75_f32, 0.5_f32].iter().enumerate() {
            for (si, &shifted) in [false, true].iter().enumerate() {
                for (d, &actual) in table[fi][si].iter().enumerate() {
                    let expected = game_pow_weight(factor, d as u8, shifted);
                    assert_eq!(actual.to_bits(), expected.to_bits(), "factor={factor} shifted={shifted} d={d}");
                }
            }
        }
        // Shifted at distance 0 keeps 1.0 (because `dist - 1 == -1 < 0`).
        assert_eq!(table[0][1][0], 1.0_f32);
        assert_eq!(table[1][1][0], 1.0_f32);
        // Shifted at distance 1 uses `factor * 0.5 + 0.5`.
        assert_eq!(table[0][1][1], 0.875_f32);
        assert_eq!(table[1][1][1], 0.75_f32);
    }

    #[test]
    fn rounding_matches_game_positive_negative_and_zero_self_strength_cases() {
        assert_eq!(round_threat_ratio(10.0, 5.0, 0.0), 1_000_000_000);
        assert_eq!(round_threat_ratio(10.0, 5.0, -1.0), 1_000_000_000);
        assert_eq!(round_threat_ratio(0.0, 0.0, 10.0), 0);
        assert_eq!(round_threat_ratio(5.0, 5.0, 5.0), 500);
        assert_eq!(round_threat_ratio(-1.0, 0.0, 2.0), -500);
        assert_eq!(round_threat_sum(0.0), 0);
        assert_eq!(round_threat_sum(1.2346), 1235);
        assert_eq!(round_threat_sum(-1.2346), -1235);
    }

    #[test]
    fn compact_buckets_strips_stale_entries_and_preserves_order() {
        #[repr(C)]
        struct RawEntry {
            days: i16,
            dist: u8,
            stamp: u8,
            id_lo: u32,
            id_hi: u32,
            quarter: u8,
            pad: [u8; 3],
        }
        #[repr(C)]
        struct RawBucket {
            begin: usize,
            end: usize,
            cap: usize,
        }
        let b0_entries = [
            RawEntry { days: 5, dist: 2, stamp: 7, id_lo: 1, id_hi: 0, quarter: 0, pad: [0; 3] },
            RawEntry { days: 3, dist: 1, stamp: 6, id_lo: 1, id_hi: 0, quarter: 1, pad: [0; 3] }, // stale stamp
            RawEntry { days: 9, dist: 4, stamp: 2, id_lo: 2, id_hi: 0, quarter: 1, pad: [0; 3] },
            RawEntry { days: 1, dist: 0, stamp: 1, id_lo: 99, id_hi: 0, quarter: 0, pad: [0; 3] }, // missing unit
        ];
        let buckets = [
            RawBucket { begin: b0_entries.as_ptr() as usize, end: unsafe { b0_entries.as_ptr().add(b0_entries.len()) } as usize, cap: 0 },
            RawBucket { begin: 0, end: 0, cap: 0 },
        ];
        let mut live = FastMap::default();
        live.insert(1u64, (7u8, 10u32));
        live.insert(2u64, (2u8, 20u32));

        let begin = buckets.as_ptr() as usize;
        let end = unsafe { buckets.as_ptr().add(buckets.len()) } as usize;
        let (offsets, compact) = unsafe { compact_buckets(begin, end, &live) };
        assert_eq!(offsets, vec![0, 2, 2]);
        assert_eq!(compact.len(), 2);
        assert_eq!((compact[0].days, compact[0].dist, compact[0].quarter_flag, compact[0].unit_idx), (5, 2, 0, 10));
        assert_eq!((compact[1].days, compact[1].dist, compact[1].quarter_flag, compact[1].unit_idx), (9, 4, 1, 20));
    }
}
