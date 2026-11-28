//! Parallel precomputation of each country's daily army strength sum
//! (`0x93c790`, `docs/game/day/country-daily-update.md`).
//!
//! During the countries' daily phase (`0x75b0d0`), every non-rebel country's
//! daily update (`0x2ee280`) calls `0x93c790(country + 0x28c0, &out, _)` while
//! AI is active (`world + 0x7484 != 0`) to sum the combat strength of all of
//! its armies (`0xd46800` -> `0xae0f00` -> `0xae01d0` / `0xae1330`). That sum
//! is purely read-only: its callees (`0x93d750`, `0xd46800`, `0xae0f00`,
//! `0xae01d0`, `0xae1330`, `0xd40790`, `0x341820`, `0x3c77c0`) are the same
//! functions already invoked concurrently across the game's AI worker threads
//! during `0x77d8a0`.
//!
//! Right before the country loop (`0x75b1f3`), this speed-up computes
//! `0x93c790` in parallel across a worker pool for every active non-rebel
//! country that has at least one unit, together with a fingerprint of all
//! per-country inputs read by `0x93c790`. When `0x2ee280` subsequently calls
//! `0x93c790` on the main thread, a matching fingerprint returns the
//! precomputed sum immediately; any country mutated by an earlier country's
//! daily update (for example by a battle, modifier rebuild, or ruler death)
//! misses the fingerprint check and falls back to the game's `0x93c790`.
//!
//! Checked mode (`speed::checked`) verifies every cache hit against the game's
//! own `0x93c790` on the main thread.
use crate::base::called_by_game;
use crate::native::hooks;
use crate::native::objects::campaign::{
    classes::ARMY as ARMY_VTABLE,
    country,
    modifier_set::{BY_MODIFIER, COUNT, PAIRS},
    modifiers::OWN as MODIFIERS_OWN,
    regiment, tag, unit,
};
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use crate::speed::pool::{Work, WorkerPool};
use std::cell::{RefCell, UnsafeCell};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

#[derive(Default)]
struct FastHash(u64);

impl Hasher for FastHash {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut h = self.0;
        for &b in bytes {
            h = (h.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
        self.0 = h;
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.0 = (self.0.rotate_left(5) ^ (i as u64)).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHash>>;

const COUNTRIES_PHASE: usize = 0x75b0d0;
const COUNTRY_LOOP_ENTRY: usize = 0x75b1f3;
const COUNTRY_LOOP_BODY: usize = 0x75b1fc;
const COUNTRY_LOOP_DONE: usize = 0x75b242;
const COUNTRY_ARMY_STRENGTH: usize = 0x93c790;
const STATIC_LOGGER_INIT: usize = 0x14d4c80;

const WORLD_COUNTRIES_DATA: usize = 0x1d08;
const WORLD_COUNTRIES_COUNT: usize = 0x1d14;
const WORLD_AI_ACTIVE: usize = 0x7484;

const COUNTRY_TAG_KIND: usize = country::TAG + 6;
const COUNTRY_MAX_MORALE: usize = 0x9878;
const COUNTRY_HIRED_MERCS: usize = 0x17a0;
const COUNTRY_HIRED_MERCS_COUNT: usize = 0x17ac;
const COUNTRY_SPECIAL_MODS: usize = 0x210;
const COUNTRY_SPECIAL_MODS_COUNT: usize = 0x21c;
const MODIFIERS_STATE: usize = country::MODIFIERS + 8;
const MODIFIER_STATE_CALCULATING: i32 = 1;

const TECH_FIRE_SHOCK_BEGIN: usize = 0x38;
const TECH_TACTICS: usize = 0x58;
const TECH_WORDS: usize = (TECH_TACTICS + 4 - TECH_FIRE_SHOCK_BEGIN) / 4;

const UNIT_MERC_COMPANY: usize = 0x48;
const UNIT_LEADER: usize = 0xd8;
const UNIT_REBEL_FACTION: usize = 0x188;
const UNIT_MERC_TAG: usize = 0x220;
const UNIT_VTABLE_IS_ARMY: usize = 0x80;
const UNIT_VTABLE_IS_MERC: usize = 0x90;

const REGIMENT_DRILL: usize = 0x30;
const REGIMENT_SPECIAL_CAT: usize = 0x64;

const MOST_THREADS: usize = 8;
const MIN_COUNTRIES_FOR_POOL: usize = 16;
const CHUNK_SIZE: usize = 2;
const WAIT: Duration = Duration::from_secs(30);
const TIMEOUT_NOTE: &str = "daily-strength: a worker thread did not finish in time; falling back to the game";

type CountryArmyStrength = unsafe extern "C" fn(usize, *mut i64, u8) -> *mut i64;
static ORIGINAL_ARMY_STRENGTH: Function<CountryArmyStrength> = Function::new();

#[derive(Clone, Copy)]
struct CachedStrength {
    fingerprint: u64,
    value: i64,
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static LOGGER_READY: AtomicBool = AtomicBool::new(false);
static POOL: OnceLock<WorkerPool> = OnceLock::new();

thread_local! {
    static CACHE: RefCell<FastMap<usize, CachedStrength>> = RefCell::new(FastMap::default());
}

fn pool() -> Option<&'static WorkerPool> {
    let pool = POOL.get_or_init(|| {
        let wanted = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MOST_THREADS).saturating_sub(1);
        WorkerPool::new("eu4-harmony daily strength", "daily strength", wanted, || {})
    });
    (!pool.is_broken() && pool.width() >= 2).then_some(pool)
}

/// The pool's worker thread IDs, which write-watch leaves running alongside
/// the main thread during `0x75b0d0`.
#[cfg(feature = "dev")]
pub(crate) fn threads() -> &'static [u32] {
    POOL.get().map_or(&[], WorkerPool::thread_ids)
}

fn mxcsr() -> u32 {
    let mut value = 0u32;
    unsafe { std::arch::asm!("stmxcsr [{}]", in(reg) &mut value, options(nostack)) };
    value
}

fn set_mxcsr(value: u32) {
    unsafe { std::arch::asm!("ldmxcsr [{}]", in(reg) &value, options(nostack)) };
}

/// Mixes a 64-bit word into an FxHash-style state.
#[inline]
fn mix(hash: u64, word: u64) -> u64 {
    (hash.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95)
}

/// Computes a fast fingerprint over every per-country field read by `0x93c790`
/// and its callees (`0xd46800`, `0xae0f00`, `0xae01d0`, `0xae1330`, `0x341820`,
/// `0x3c77c0`, `0xd40790`).
unsafe fn fingerprint_country(country: usize, army_vtable: usize) -> Option<u64> {
    // Skip countries whose modifiers are mid-recalculation.
    let mod_state = unsafe { *((country + MODIFIERS_STATE) as *const i32) };
    if mod_state == MODIFIER_STATE_CALCULATING {
        return None;
    }

    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let max_morale = unsafe { *((country + COUNTRY_MAX_MORALE) as *const i32) };
    h = mix(h, max_morale as u32 as u64);

    // Technology fire/shock values (+0x38..+0x5c including tactics at +0x58).
    let tech = unsafe { *((country + country::TECHNOLOGY) as *const usize) };
    h = mix(h, tech as u64);
    if tech != 0 {
        let base = (tech + TECH_FIRE_SHOCK_BEGIN) as *const u32;
        for i in 0..TECH_WORDS {
            h = mix(h, unsafe { *base.add(i) } as u64);
        }
    }

    // Country modifier set at `country + 0x28c8 + 8` (`country + 0x28d0`).
    let mod_set = country + MODIFIERS_STATE + MODIFIERS_OWN;
    let by_mod = unsafe { *((mod_set + BY_MODIFIER) as *const u8) };
    let mod_count = unsafe { *((mod_set + COUNT) as *const i32) }.max(0) as usize;
    let mod_pairs = unsafe { *((mod_set + PAIRS) as *const *const u64) };
    h = mix(h, ((by_mod as u64) << 32) | (mod_count as u64));
    if !mod_pairs.is_null() && mod_count > 0 {
        for i in 0..mod_count {
            h = mix(h, unsafe { *mod_pairs.add(i) });
        }
    }

    // Special-unit modifiers array at `country + 0x210` / `+0x21c` (`0xaedf20`).
    let special_count = unsafe { *((country + COUNTRY_SPECIAL_MODS_COUNT) as *const i32) }.max(0) as usize;
    let special_ptr = unsafe { *((country + COUNTRY_SPECIAL_MODS) as *const *const usize) };
    h = mix(h, special_count as u64);
    if !special_ptr.is_null() && special_count > 0 {
        for i in 0..special_count {
            let entry = unsafe { *special_ptr.add(i) };
            h = mix(h, entry as u64);
            if entry != 0 {
                h = mix(h, unsafe { *((entry + 0x30) as *const usize) } as u64);
            }
        }
    }

    // Hired mercenary companies array (`country + 0x17a0` / `+0x17ac`, 16-byte entries).
    let merc_count = unsafe { *((country + COUNTRY_HIRED_MERCS_COUNT) as *const i32) }.max(0) as usize;
    let merc_ptr = unsafe { *((country + COUNTRY_HIRED_MERCS) as *const *const u64) };
    h = mix(h, merc_count as u64);
    if !merc_ptr.is_null() && merc_count > 0 {
        for i in 0..merc_count {
            let id = unsafe { *(merc_ptr.add(i * 2) as *const i32) };
            let comp = unsafe { *merc_ptr.add(i * 2 + 1) } as usize;
            let hire_date = if comp != 0 { unsafe { *((comp + 0x20) as *const i32) } } else { 0 };
            h = mix(h, ((id as u32 as u64) << 32) | (hire_date as u32 as u64));
            h = mix(h, comp as u64);
        }
    }

    // Units linked list (`country + 0xd18`).
    let mut node = unsafe { *((country + country::UNITS) as *const usize) };
    let mut unit_count = 0u64;
    while node != 0 {
        let u = unsafe { *(node as *const usize) };
        node = unsafe { *((node + 0x10) as *const usize) };
        if u == 0 {
            continue;
        }
        let vtable = unsafe { *(u as *const usize) };
        let is_army = if vtable == army_vtable {
            true
        } else {
            let f: unsafe extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(*((vtable + UNIT_VTABLE_IS_ARMY) as *const usize)) };
            unsafe { f(u) != 0 }
        };
        if !is_army {
            continue;
        }
        unit_count += 1;
        h = mix(h, u as u64);
        h = mix(h, unsafe { *((u + unit::OWNER) as *const u64) });
        h = mix(h, unsafe { *((u + UNIT_LEADER) as *const usize) } as u64);
        h = mix(h, unsafe { *((u + UNIT_MERC_COMPANY) as *const usize) } as u64);
        h = mix(h, unsafe { *((u + unit::LOCATION) as *const usize) } as u64);
        let rebel = unsafe { *((u + UNIT_REBEL_FACTION) as *const usize) };
        h = mix(h, rebel as u64);

        let is_merc_fn: unsafe extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(*((vtable + UNIT_VTABLE_IS_MERC) as *const usize)) };
        if unsafe { is_merc_fn(u) } != 0 {
            h = mix(h, unsafe { *((u + UNIT_MERC_TAG) as *const u64) });
        }

        let reg_begin = unsafe { *((u + unit::REGIMENTS) as *const usize) };
        let reg_end = unsafe { *((u + unit::REGIMENTS + 8) as *const usize) };
        let reg_count = reg_end.saturating_sub(reg_begin) / 8;
        h = mix(h, reg_count as u64);
        let regs = reg_begin as *const usize;
        for i in 0..reg_count {
            let r = unsafe { *regs.add(i) };
            if r == 0 {
                continue;
            }
            let unit_type = unsafe { *((r + regiment::TYPE) as *const usize) };
            let strength = unsafe { *((r + regiment::STRENGTH) as *const i32) } as u32 as u64;
            let morale = unsafe { *((r + regiment::MORALE) as *const i32) } as u32 as u64;
            let drill = unsafe { *((r + REGIMENT_DRILL) as *const i32) } as u32 as u64;
            let special_cat = unsafe { *((r + REGIMENT_SPECIAL_CAT) as *const i32) } as u32 as u64;
            h = mix(h, unit_type as u64);
            h = mix(h, (strength << 32) | morale);
            h = mix(h, (drill << 32) | special_cat);
        }
    }
    h = mix(h, unit_count);
    Some(h)
}

struct Batch {
    countries: Vec<usize>,
    army_vtable: usize,
    mxcsr: u32,
    next: AtomicUsize,
    abandoned: AtomicBool,
    /// Disjoint per-index output slots written once by whichever thread claims
    /// index `i`.
    results: Vec<UnsafeCell<Option<CachedStrength>>>,
}

// SAFETY: Each index `i` in `results` is claimed atomically via `next` by at
// most one thread and read only after `WorkerPool::run` joins all workers.
unsafe impl Sync for Batch {}

impl Work for Batch {
    fn work(&self, on_worker: bool) {
        if on_worker {
            set_mxcsr(self.mxcsr);
        }
        let orig = ORIGINAL_ARMY_STRENGTH.get();
        let len = self.countries.len();
        loop {
            if self.abandoned.load(Ordering::Relaxed) {
                return;
            }
            let start = self.next.fetch_add(CHUNK_SIZE, Ordering::Relaxed);
            if start >= len {
                return;
            }
            let end = (start + CHUNK_SIZE).min(len);
            for i in start..end {
                let country = self.countries[i];
                let Some(fp) = (unsafe { fingerprint_country(country, self.army_vtable) }) else {
                    continue;
                };
                let mut sum = 0i64;
                unsafe { orig(country + country::MODIFIERS, &mut sum, 0) };
                unsafe {
                    *self.results[i].get() = Some(CachedStrength { fingerprint: fp, value: sum });
                }
            }
        }
    }

    fn abandon(&self) {
        self.abandoned.store(true, Ordering::SeqCst);
    }
}

#[called_by_game]
extern "C" fn precompute_daily_strengths(world_ptr: usize) {
    ACTIVE.store(false, Ordering::Relaxed);
    CACHE.with(|c| c.borrow_mut().clear());

    if world_ptr == 0 || speed::game_path(&SPEED_UP) {
        return;
    }
    // `0x2ee280` only calls `0x93c790` when AI is active (`world + 0x7484 != 0`).
    let ai_active = unsafe { *((world_ptr + WORLD_AI_ACTIVE) as *const u8) };
    if ai_active == 0 {
        return;
    }
    let count = unsafe { *((world_ptr + WORLD_COUNTRIES_COUNT) as *const i32) };
    if count <= 0 {
        return;
    }
    let data = unsafe { *((world_ptr + WORLD_COUNTRIES_DATA) as *const *const usize) };
    if data.is_null() {
        return;
    }

    let Some(pool) = pool() else { return };

    let mut countries = Vec::with_capacity(count as usize);
    for i in 0..(count as usize) {
        let c = unsafe { *data.add(i) };
        if c == 0 {
            continue;
        }
        // `0x2ee280` returns early for rebel/observer tags (`tag_kind == 7`).
        if unsafe { *((c + COUNTRY_TAG_KIND) as *const u8) } == tag::REBELS {
            continue;
        }
        // Countries without any units finish `0x93c790` immediately.
        if unsafe { *((c + country::UNITS) as *const usize) } == 0 {
            continue;
        }
        countries.push(c);
    }

    if countries.len() < MIN_COUNTRIES_FOR_POOL {
        return;
    }

    // Ensure the static logger singleton inside `0x93c790` (`0x14d4c80`) is
    // initialized once on the main thread before workers call `0x93c790`.
    let m = crate::native::hooked_memory();
    if !LOGGER_READY.swap(true, Ordering::Relaxed) {
        let init_logger: unsafe extern "C" fn() -> usize = unsafe { std::mem::transmute(m.at(STATIC_LOGGER_INIT)) };
        unsafe { init_logger() };
    }

    MAIN_THREAD_ID.store(crate::base::thread_id(), Ordering::Relaxed);

    let army_vtable = m.at(ARMY_VTABLE);
    let results = (0..countries.len()).map(|_| UnsafeCell::new(None)).collect();
    let batch = Arc::new(Batch { countries, army_vtable, mxcsr: mxcsr(), next: AtomicUsize::new(0), abandoned: AtomicBool::new(false), results });

    if pool.run(batch.clone(), WAIT, TIMEOUT_NOTE).is_none() || pool.is_broken() || batch.abandoned.load(Ordering::SeqCst) {
        return;
    }

    CACHE.with(|cell| {
        let mut map = cell.borrow_mut();
        map.reserve(batch.countries.len());
        for (i, &country) in batch.countries.iter().enumerate() {
            if let Some(entry) = unsafe { *batch.results[i].get() } {
                map.insert(country, entry);
            }
        }
    });
    ACTIVE.store(true, Ordering::Relaxed);
}

fn on_countries_phase_after(_: &mut hooks::Call<'_>) {
    if ACTIVE.swap(false, Ordering::Relaxed) {
        CACHE.with(|c| c.borrow_mut().clear());
    }
}

#[called_by_game]
unsafe extern "C" fn country_army_strength_hook(cached_data: usize, out: *mut i64, param_3: u8) -> *mut i64 {
    if ACTIVE.load(Ordering::Relaxed) && crate::base::thread_id() == MAIN_THREAD_ID.load(Ordering::Relaxed) && cached_data != 0 {
        let country = unsafe { *(cached_data as *const usize) };
        let entry = CACHE.with(|c| c.borrow().get(&country).copied());
        if let Some(cached) = entry {
            let army_vtable = crate::native::hooked_memory().at(ARMY_VTABLE);
            if unsafe { fingerprint_country(country, army_vtable) } == Some(cached.fingerprint) {
                if speed::checked(&SPEED_UP) {
                    let mut game_out = 0i64;
                    unsafe { ORIGINAL_ARMY_STRENGTH.get()(cached_data, &mut game_out, param_3) };
                    speed::compared(&SPEED_UP, cached.value == game_out, || {
                        format!("daily army strength for country {country:#x}: cached {}, game {game_out}", cached.value)
                    });
                }
                unsafe { *out = cached.value };
                return out;
            }
        }
    }
    unsafe { ORIGINAL_ARMY_STRENGTH.get()(cached_data, out, param_3) }
}

resume!(COUNTRY_LOOP_RESUME: LOOP_BODY_AT = COUNTRY_LOOP_BODY, LOOP_DONE_AT = COUNTRY_LOOP_DONE);

/// Replaces `cmp dword ptr [rdi+0x1d14], 0; jle 0x75b242` at the start of the
/// daily countries loop in `0x75b0d0`: precomputes daily army strengths in
/// parallel across active countries before entering the loop body.
#[unsafe(naked)]
extern "C" fn country_loop_stub() {
    std::arch::naked_asm!(
        "cmp dword ptr [rdi + 0x1d14], 0",
        "jle 2f",
        call_rust!("mov rcx, rdi", call precompute_daily_strengths),
        "jmp qword ptr [rip + {body}]",
        "2:",
        "jmp qword ptr [rip + {done}]",
        precompute_daily_strengths = sym precompute_daily_strengths,
        body = sym LOOP_BODY_AT,
        done = sym LOOP_DONE_AT,
    );
}

/// Checked, every precomputed daily army strength hit is compared against
/// calling `0x93c790` on the main thread.
pub const SPEED_UP: SpeedUp = SpeedUp::new("daily-strength", "frames", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::observe(COUNTRIES_PHASE, "40 53 55 56 57 48 83 ec 68 0f 29 74 24 50 48 8b", "countries daily phase", 1, None, Some(on_countries_phase_after)),
        Point::jump(
            COUNTRY_LOOP_ENTRY,
            "83 bf 14 1d 00 00 00 7e 46",
            "countries daily loop entry",
            country_loop_stub as *const () as usize,
            &COUNTRY_LOOP_RESUME,
        )
        .uses(&[WORLD_COUNTRIES_COUNT]),
        Point::keep(COUNTRY_LOOP_BODY, "48 8b f3 90 48 8b 8f 08 1d 00 00", "countries daily loop body").uses(&[WORLD_COUNTRIES_DATA]),
        Point::keep(COUNTRY_LOOP_DONE, "48 8b 1d 0f 0c cd 01 80 7b 68 00", "countries daily loop end"),
        Point::replace(
            COUNTRY_ARMY_STRENGTH,
            "48 8b c4 44 88 40 18 53 41 54 41 55 41 57 48 83",
            "country daily army strength sum",
            country_army_strength_hook as CountryArmyStrength,
            &ORIGINAL_ARMY_STRENGTH,
        ),
        Point::keep(0x2ee4cc, "44 38 a0 84 74 00 00 74 5f", "country daily update checks AI active flag").uses(&[WORLD_AI_ACTIVE]),
        Point::keep(0x93c7b5, "48 8b 9b 18 0d 00 00", "country army strength walks units").uses(&[country::UNITS]),
        Point::keep(0x93c8ac, "49 8b 86 20 02 00 00 48 8b 8d a0 17 00 00 48 63 95 ac 17 00 00", "country army strength reads hired mercenary list").uses(&[
            UNIT_MERC_TAG,
            COUNTRY_HIRED_MERCS,
            COUNTRY_HIRED_MERCS_COUNT,
        ]),
        Point::keep(0xae0f24, "49 63 90 78 98 00 00", "regiment strength reads country max morale").uses(&[COUNTRY_MAX_MORALE]),
        Point::keep(0xae106e, "48 63 4f 30", "regiment strength reads drill").uses(&[REGIMENT_DRILL]),
        Point::keep(0xae1148, "48 63 47 64", "regiment strength reads special unit category").uses(&[REGIMENT_SPECIAL_CAT]),
        Point::keep(0x3c74c5, "48 8b 81 40 24 00 00 44 8b 40 58", "military tactics reads country technology").uses(&[country::TECHNOLOGY, TECH_TACTICS]),
        Point::keep(0xd46852, "48 8b 91 88 01 00 00", "army strength reads rebel faction").uses(&[UNIT_REBEL_FACTION]),
        Point::keep(0xd46bf1, "49 8b 8f d8 00 00 00", "army strength reads unit leader").uses(&[UNIT_LEADER]),
        Point::keep(0xaedf36, "48 8b ba 10 02 00 00 48 63 82 1c 02 00 00", "special unit modifiers array")
            .uses(&[COUNTRY_SPECIAL_MODS, COUNTRY_SPECIAL_MODS_COUNT]),
        Point::keep(STATIC_LOGGER_INIT, "48 83 ec 28 65 48 8b 04 25 58 00 00 00 ba 08 00", "country army strength static logger init"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_detects_country_and_regiment_mutations() {
        // Build a synthetic country buffer with tech, modifier set, and one army + regiment.
        let mut country_buf = vec![0u8; 0xa000];
        let mut tech_buf = [0u32; 32];
        let mut mod_pairs = [0x0001_0000_0000_0001u64; 8];
        let mut army_buf = vec![0u8; 0x240];
        let mut reg_buf = vec![0u8; 0x80];
        let mut vtable = [0usize; 32];

        extern "C" fn return_false(_: usize) -> u8 {
            0
        }
        vtable[UNIT_VTABLE_IS_MERC / 8] = return_false as *const () as usize;
        let army_vtable = vtable.as_ptr() as usize;

        let reg_ptr = reg_buf.as_mut_ptr() as usize;
        let reg_vec = [reg_ptr];
        let army_ptr = army_buf.as_mut_ptr() as usize;
        let unit_node = [army_ptr, 0usize, 0usize];
        let country_ptr = country_buf.as_mut_ptr() as usize;

        unsafe {
            *(army_ptr as *mut usize) = army_vtable;
            *((army_ptr + unit::REGIMENTS) as *mut usize) = reg_vec.as_ptr() as usize;
            *((army_ptr + unit::REGIMENTS + 8) as *mut usize) = reg_vec.as_ptr().add(1) as usize;
            *((reg_ptr + regiment::STRENGTH) as *mut i32) = 1000;
            *((reg_ptr + REGIMENT_DRILL) as *mut i32) = 500;

            *((country_ptr + country::TECHNOLOGY) as *mut usize) = tech_buf.as_ptr() as usize;
            *((country_ptr + COUNTRY_MAX_MORALE) as *mut i32) = 4000;
            *((country_ptr + country::UNITS) as *mut usize) = unit_node.as_ptr() as usize;
            let mod_set = country_ptr + MODIFIERS_STATE + MODIFIERS_OWN;
            *((mod_set + COUNT) as *mut i32) = mod_pairs.len() as i32;
            *((mod_set + PAIRS) as *mut usize) = mod_pairs.as_ptr() as usize;
        }

        let base = unsafe { fingerprint_country(country_ptr, army_vtable) }.unwrap();
        assert_eq!(unsafe { fingerprint_country(country_ptr, army_vtable) }, Some(base));

        // Mutating regiment strength invalidates the fingerprint.
        unsafe { *((reg_ptr + regiment::STRENGTH) as *mut i32) = 950 };
        let after_strength = unsafe { fingerprint_country(country_ptr, army_vtable) }.unwrap();
        assert_ne!(after_strength, base);

        // Mutating a modifier invalidates the fingerprint.
        unsafe { *mod_pairs.as_mut_ptr().add(3) ^= 0x10 };
        let after_mod = unsafe { fingerprint_country(country_ptr, army_vtable) }.unwrap();
        assert_ne!(after_mod, after_strength);

        // Mutating tech tactics invalidates the fingerprint.
        unsafe { *tech_buf.as_mut_ptr().add(TECH_TACTICS / 4) = 2500 };
        let after_tech = unsafe { fingerprint_country(country_ptr, army_vtable) }.unwrap();
        assert_ne!(after_tech, after_mod);

        // Modifier state == CALCULATING returns None.
        unsafe { *((country_ptr + MODIFIERS_STATE) as *mut i32) = MODIFIER_STATE_CALCULATING };
        assert_eq!(unsafe { fingerprint_country(country_ptr, army_vtable) }, None);
    }
}
