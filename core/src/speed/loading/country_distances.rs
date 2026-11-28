//! Speed up country distance table rebuilds (`0x2d0b30`) by precomputing
//! per-province capital separation (`0x9fabe0`), detecting adjacent/same-province
//! zero distances in O(|T| * deg) instead of O(|S| * |T|), and pruning planar
//! Euclidean searches with x-sorted wrapped squared distance
//! (docs/game/program/saves.md, "Country distance scratch space" and
//! "Distance inputs, arithmetic and shared writes").
//!
//! Vanilla `0x2d0b30(country, arg2)` re-evaluates `0x9fabe0(src_prov, src_cap, 1)`
//! inside the target-country loop (repeating it for every target country),
//! re-evaluates `0x9fabe0(dst_prov, dst_cap, 1)` inside the source-province loop
//! (repeating it for every source province), probes the target's adjacency hash
//! table for every `(src, dst)` pair, and calls `0x88a1a0` (square root + float
//! scaling) for every non-adjacent pair.
//!
//! For planar maps (`map+0x31 == 0`) with valid province IDs, this replacement:
//! 1. Assembles and deduplicates the source province list once, classifies each
//!    source province against the source capital once, marks source IDs in a
//!    generation-stamped scratch table, and sorts `src_near` / `src_far` by x.
//! 2. For each qualifying target country, deduplicates target provinces,
//!    classifies each target province against the target capital once, checks
//!    same-province / direct-adjacency zero hits by scanning each target
//!    province's own adjacency table once, and then finds the minimum wrapped
//!    squared distance via x-sorted two-sided pruning before running the exact
//!    `0x88a230` square-root rounding formula once per table.
//! 3. Writes both `(country, target)` and `(target, country)` through native
//!    `0x2d2120` in the original order.
//!
//! Spherical maps (`map+0x31 != 0`) and out-of-range province IDs fall back to
//! the game's original code so diagnostic logs and spherical math stay intact.
//! In `checked` mode the fast result for every target is compared against the
//! game's original rebuild table slot by slot.
use crate::base::called_by_game;
use crate::native::objects::campaign::{
    countries,
    functions::SUBJECT_TYPE,
    globals::{COUNTRY_MANAGER, MAP, WORLD},
};
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;

const REBUILD: usize = 0x2d0b30;
const PRE_REBUILD: usize = 0x2d0a00;
const STORE_BOTH: usize = 0x2d2120;
const IS_SEPARATED: usize = 0x9fabe0;
const RESIZE_U16_VEC: usize = 0x4167d0;

const STATIC_PROV_STRIDE: usize = 0x230;
const RUNTIME_PROV_STRIDE: usize = 0x2e10;
const DEFAULT_DISTANCE: u16 = 64000;

type Rebuild = unsafe extern "C" fn(usize, u8);
type PreRebuild = unsafe extern "C" fn(usize);
type StoreBoth = unsafe extern "C" fn(usize, u64, u16, u16);
type IsSeparated = unsafe extern "C" fn(usize, usize, u8) -> u8;
type ResizeU16Vec = unsafe extern "C" fn(usize, usize);
type SubjectType = unsafe extern "C" fn(usize) -> usize;
type WorldCountryCount = unsafe extern "C" fn(usize) -> i32;

static ORIGINAL_REBUILD: Function<Rebuild> = Function::new();
static PRE_REBUILD_FN: Function<PreRebuild> = Function::new();
static STORE_BOTH_FN: Function<StoreBoth> = Function::new();
static IS_SEPARATED_FN: Function<IsSeparated> = Function::new();
static RESIZE_U16_VEC_FN: Function<ResizeU16Vec> = Function::new();

pub const SPEED_UP: SpeedUp = SpeedUp::new("country-distances", "loading", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            REBUILD,
            "88 54 24 10 48 89 4c 24 08 55 53 56 57 41 54 41 55 41 56 41 57",
            "rebuild country distance tables",
            rebuild_distances as Rebuild,
            &ORIGINAL_REBUILD,
        ),
        Point::keep(PRE_REBUILD, "40 53 56 57 41 56 41 57 48 83 ec 20 48 8b 81 38 23 00 00", "country distance pre-rebuild").recorded(PRE_REBUILD_FN.slot()),
        Point::keep(STORE_BOTH, "48 89 5c 24 08 48 89 6c 24 18 48 89 74 24 20 48 89 54 24 10", "store both country distance minima")
            .recorded(STORE_BOTH_FN.slot()),
        Point::keep(IS_SEPARATED, "48 83 ec 28 48 8b 82 60 05 00 00 48 39 81 60 05 00 00", "province capital separation predicate")
            .recorded(IS_SEPARATED_FN.slot()),
        Point::keep(RESIZE_U16_VEC, "40 53 48 83 ec 20 4c 8b 51 08 48 8b d9 4c 8b 01", "resize u16 distance table").recorded(RESIZE_U16_VEC_FN.slot()),
        Point::keep(0x2d0d22usize, "48 8b 41 08 48 85 c0 0f 84 19 04 00 00 48 8b 40 40", "country distance subject type check"),
    ]
}

#[derive(Clone, Copy, Debug)]
struct ProvCoord {
    x: i16,
    y: i16,
}

#[derive(Default)]
struct Scratch {
    gen: u32,
    /// Per-province generation marker for source membership:
    /// `src_mark[id] == gen` -> source province, `src_near_mark[id] == gen` -> near source province.
    src_mark: Vec<u32>,
    src_near_mark: Vec<u32>,
    /// Per-province generation marker for target deduplication.
    dst_gen: u32,
    dst_mark: Vec<u32>,
    src_ids: Vec<i32>,
    src_near: Vec<ProvCoord>,
    src_far: Vec<ProvCoord>,
    dst_near: Vec<ProvCoord>,
    dst_far: Vec<ProvCoord>,
    checked_results: Vec<(usize, u16, u16)>,
}

impl Scratch {
    fn ensure_provinces(&mut self, count: usize) {
        if self.src_mark.len() < count {
            self.src_mark.resize(count, 0);
            self.src_near_mark.resize(count, 0);
            self.dst_mark.resize(count, 0);
        }
    }

    fn next_src_gen(&mut self) -> u32 {
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            self.src_mark.fill(0);
            self.src_near_mark.fill(0);
            self.gen = 1;
        }
        self.gen
    }

    fn next_dst_gen(&mut self) -> u32 {
        self.dst_gen = self.dst_gen.wrapping_add(1);
        if self.dst_gen == 0 {
            self.dst_mark.fill(0);
            self.dst_gen = 1;
        }
        self.dst_gen
    }
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

#[inline]
unsafe fn read_usize(addr: usize) -> usize {
    (addr as *const usize).read()
}

#[inline]
unsafe fn read_u64(addr: usize) -> u64 {
    (addr as *const u64).read()
}

#[inline]
unsafe fn read_i32(addr: usize) -> i32 {
    (addr as *const i32).read()
}

#[inline]
unsafe fn read_i16(addr: usize) -> i16 {
    (addr as *const i16).read()
}

#[inline]
unsafe fn read_u8(addr: usize) -> u8 {
    (addr as *const u8).read()
}

#[inline]
fn wrapped_dx(x1: i16, x2: i16, map_w: i32, half_w: i32) -> i32 {
    let raw = x1.wrapping_sub(x2);
    let abs = raw.wrapping_abs();
    if (abs as i32) > half_w {
        (map_w as i16).wrapping_sub(abs) as i32
    } else {
        raw as i32
    }
}

#[cfg(test)]
#[inline]
fn wrapped_sq_dist(a: ProvCoord, b: ProvCoord, map_w: i32, half_w: i32) -> i32 {
    let dx = wrapped_dx(a.x, b.x, map_w, half_w);
    let dy = a.y.wrapping_sub(b.y) as i32;
    dx * dx + dy * dy
}

/// Exact match of `0x88a230` (planar mode 0) followed by `/ 1000` and narrowing
/// to `u16` in `0x2d0b30`.
#[inline]
fn sq_dist_to_u16(sq: i32) -> u16 {
    let d = (sq as f64).sqrt();
    let scaled = ((d as f32) * 1000.0_f32 + 0.5_f32) as i32;
    (scaled / 1000) as u16
}

/// Updates `best_sq` against all coordinates in `sorted_src` (sorted ascending
/// by `x`) using wrapped two-sided x-pruning around `target.x`.
#[inline]
fn min_sq_against_sorted(target: ProvCoord, sorted_src: &[ProvCoord], map_w: i32, half_w: i32, best_sq: &mut i32) {
    let n = sorted_src.len();
    if n == 0 || *best_sq == 0 {
        return;
    }
    let split = sorted_src.partition_point(|p| p.x < target.x);
    let mut visited = 0usize;

    // Walk right from `split`, wrapping around `n -> 0`.
    let mut r = split;
    while visited < n {
        if r == n {
            r = 0;
        }
        let p = sorted_src[r];
        let dx = wrapped_dx(p.x, target.x, map_w, half_w);
        let dx2 = dx * dx;
        if dx2 >= *best_sq {
            break;
        }
        let dy = p.y.wrapping_sub(target.y) as i32;
        let sq = dx2 + dy * dy;
        if sq < *best_sq {
            *best_sq = sq;
            if sq == 0 {
                return;
            }
        }
        r += 1;
        visited += 1;
    }

    // Walk left from `split - 1`, wrapping around `0 -> n - 1`.
    let mut l = split;
    while visited < n {
        l = if l == 0 { n - 1 } else { l - 1 };
        let p = sorted_src[l];
        let dx = wrapped_dx(p.x, target.x, map_w, half_w);
        let dx2 = dx * dx;
        if dx2 >= *best_sq {
            break;
        }
        let dy = p.y.wrapping_sub(target.y) as i32;
        let sq = dx2 + dy * dy;
        if sq < *best_sq {
            *best_sq = sq;
            if sq == 0 {
                return;
            }
        }
        visited += 1;
    }
}

/// Pushes valid province IDs from a `std::vector<i32>` at `vec_addr`.
/// Returns `false` if any ID is out of range `[0, num_provs)`.
#[inline]
unsafe fn collect_std_vec_i32(vec_addr: usize, num_provs: usize, mark: &mut [u32], gen: u32, out: &mut Vec<i32>) -> bool {
    let begin = read_usize(vec_addr);
    let end = read_usize(vec_addr + 8);
    if begin == end {
        return true;
    }
    let slice = std::slice::from_raw_parts(begin as *const i32, (end - begin) / 4);
    for &id in slice {
        let idx = id as u32 as usize;
        if idx >= num_provs {
            return false;
        }
        if mark[idx] != gen {
            mark[idx] = gen;
            out.push(id);
        }
    }
    true
}

/// Pushes valid province IDs from a Paradox array `(ptr at +0x00, count at +0x0c)`.
#[inline]
unsafe fn collect_pdx_array_i32(ptr: usize, count: i32, num_provs: usize, mark: &mut [u32], gen: u32, out: &mut Vec<i32>) -> bool {
    if count <= 0 || ptr == 0 {
        return true;
    }
    let slice = std::slice::from_raw_parts(ptr as *const i32, count as usize);
    for &id in slice {
        let idx = id as u32 as usize;
        if idx >= num_provs {
            return false;
        }
        if mark[idx] != gen {
            mark[idx] = gen;
            out.push(id);
        }
    }
    true
}

/// Checks if province `q` (runtime object `runtime_q`) is the same as or
/// directly adjacent to any marked source province (`src_mark` / `src_near_mark`).
/// Returns `(adj_near, adj_any)`.
#[inline]
unsafe fn check_target_adjacency(q_id: i32, runtime_q: usize, src_mark: &[u32], src_near_mark: &[u32], src_gen: u32) -> (bool, bool) {
    let q_idx = q_id as u32 as usize;
    let mut adj_near = src_near_mark[q_idx] == src_gen;
    let mut adj_any = src_mark[q_idx] == src_gen;
    if adj_near {
        return (true, true);
    }
    let static_q = read_usize(runtime_q + 0x28);
    if static_q == 0 {
        return (adj_near, adj_any);
    }
    let adj_begin = read_usize(static_q + 0x180);
    let adj_end = read_usize(static_q + 0x188);
    if adj_begin != 0 && adj_end > adj_begin {
        let table = std::slice::from_raw_parts(adj_begin as *const i32, (adj_end - adj_begin) / 4);
        for &nbr in table {
            if nbr == 0 {
                continue;
            }
            let n_idx = nbr as u32 as usize;
            if n_idx < src_mark.len() && src_mark[n_idx] == src_gen {
                adj_any = true;
                if src_near_mark[n_idx] == src_gen {
                    adj_near = true;
                    break;
                }
            }
        }
    }
    (adj_near, adj_any)
}

#[inline]
unsafe fn static_prov_coord(static_provs: usize, id: i32) -> ProvCoord {
    let entry = static_provs + (id as i16 as i32 as u32 as usize) * STATIC_PROV_STRIDE;
    ProvCoord { x: read_i16(entry + 0x7c), y: read_i16(entry + 0x7e) }
}

/// Computes `(min_near, min_any)` for every target country in `1..country_count`
/// (skipping `country`). Returns `false` if we must fall back to native code
/// (spherical map or out-of-range province ID).
///
/// When `record_only` is `true`, results are pushed into `scratch.checked_results`
/// without mutating game tables; when `false`, results are written directly via
/// native `0x2d2120`.
unsafe fn compute_distances(country: usize, record_only: bool, scratch: &mut Scratch) -> bool {
    let m = crate::native::hooked_memory();
    let world = read_usize(m.at(WORLD));
    let map = read_usize(m.at(MAP));
    let country_db = read_usize(m.at(COUNTRY_MANAGER));
    let subject_type_fn: SubjectType = std::mem::transmute(m.at(SUBJECT_TYPE.rva()));
    if world == 0 || map == 0 || country_db == 0 {
        return false;
    }
    if read_u8(map + 0x31) != 0 {
        return false;
    }

    let map_w = read_i32(map + 0x28);
    let half_w = map_w / 2;
    let static_provs = read_usize(map + 0x6c0);
    let runtime_begin = read_usize(map + 0x6f0);
    let runtime_end = read_usize(map + 0x6f8);
    let num_provs = (runtime_end.saturating_sub(runtime_begin)) / RUNTIME_PROV_STRIDE;
    if static_provs == 0 || runtime_begin == 0 || num_provs == 0 {
        return false;
    }

    let Some((countries_by_tag, countries_by_tag_len)) = countries::list_and_count(country_db) else {
        static REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        crate::base::broken_invariant(&REPORTED, "country-distances: unexpected country table; falling back to game");
        return false;
    };
    let country_by_idx = |idx: i16| -> Option<usize> {
        if idx < 0 {
            return None;
        }
        let u = idx as usize;
        if u >= countries_by_tag_len {
            return None;
        }
        let entry = read_usize(countries_by_tag + u * 8);
        (entry != 0).then_some(entry)
    };
    let self_tag = read_u64(country + 0x20);
    let self_idx = (self_tag >> 32) as i16;
    let Some(self_entry) = country_by_idx(self_idx) else {
        return false;
    };
    let self_capital = read_usize(self_entry + 0x14b0);

    scratch.ensure_provinces(num_provs);
    scratch.src_ids.clear();
    scratch.src_near.clear();
    scratch.src_far.clear();
    if record_only {
        scratch.checked_results.clear();
    }

    // 1. Assemble and deduplicate source province IDs.
    let src_gen = scratch.next_src_gen();
    if !collect_std_vec_i32(country + 0x2338, num_provs, &mut scratch.src_mark, src_gen, &mut scratch.src_ids) {
        return false;
    }
    let self_extra = read_usize(country + 0x9a78);
    if self_extra != 0
        && !collect_pdx_array_i32(read_usize(self_extra + 0x30), read_i32(self_extra + 0x3c), num_provs, &mut scratch.src_mark, src_gen, &mut scratch.src_ids)
    {
        return false;
    }
    let subj_begin = read_usize(country + 0x1a18);
    let subj_end = read_usize(country + 0x1a20);
    if subj_begin != 0 && subj_end > subj_begin {
        let subjects = std::slice::from_raw_parts(subj_begin as *const u64, (subj_end - subj_begin) / 8);
        for &tag in subjects {
            let sub_idx = (tag >> 32) as i16;
            let Some(sub) = country_by_idx(sub_idx) else {
                return false;
            };
            let st = subject_type_fn(sub);
            if st != 0 && read_u8(st + 0x151) != 0 {
                if !collect_pdx_array_i32(read_usize(sub + 0x1338), read_i32(sub + 0x1344), num_provs, &mut scratch.src_mark, src_gen, &mut scratch.src_ids) {
                    return false;
                }
                let sub_extra = read_usize(sub + 0x9a78);
                if sub_extra != 0
                    && !collect_pdx_array_i32(
                        read_usize(sub_extra + 0x30),
                        read_i32(sub_extra + 0x3c),
                        num_provs,
                        &mut scratch.src_mark,
                        src_gen,
                        &mut scratch.src_ids,
                    )
                {
                    return false;
                }
            }
        }
    }

    // 2. Classify each source province once and build x-sorted coordinate arrays.
    for &p_id in &scratch.src_ids {
        let p_idx = p_id as u32 as usize;
        let runtime_p = runtime_begin + p_idx * RUNTIME_PROV_STRIDE;
        let separated = self_capital != 0 && IS_SEPARATED_FN.get()(runtime_p, self_capital, 1) != 0;
        let coord = static_prov_coord(static_provs, p_id);
        if !separated {
            scratch.src_near_mark[p_idx] = src_gen;
            scratch.src_near.push(coord);
        } else {
            scratch.src_far.push(coord);
        }
    }
    scratch.src_near.sort_unstable_by_key(|p| p.x);
    scratch.src_far.sort_unstable_by_key(|p| p.x);

    // 3. Walk world countries from index 1.
    let world_countries_begin = read_usize(world + 0x1ce8);
    let world_countries_end = read_usize(world + 0x1cf0);
    let total_countries = (world_countries_end.saturating_sub(world_countries_begin)) / 8;

    let mut dst_ids_buf: Vec<i32> = Vec::new();
    for ci in 1..total_countries {
        let target = read_usize(world_countries_begin + ci * 8);
        if target == 0 {
            return false;
        }
        let target_tag = read_u64(target + 0x20);
        let target_idx = (target_tag >> 32) as i16;
        if target_idx == self_idx {
            continue;
        }

        let (mut min_near, mut min_any) = (DEFAULT_DISTANCE, DEFAULT_DISTANCE);
        let Some(target_entry) = country_by_idx(target_idx) else {
            return false;
        };
        let qualifies = !scratch.src_ids.is_empty() && (read_i32(target_entry + 0x1344) != 0 || read_u8(target_entry + 0x26) == 7);

        if qualifies {
            dst_ids_buf.clear();
            let dst_gen = scratch.next_dst_gen();
            if !collect_std_vec_i32(target_entry + 0x2338, num_provs, &mut scratch.dst_mark, dst_gen, &mut dst_ids_buf) {
                return false;
            }
            let target_extra = read_usize(target_entry + 0x9a78);
            if target_extra != 0
                && !collect_pdx_array_i32(
                    read_usize(target_extra + 0x30),
                    read_i32(target_extra + 0x3c),
                    num_provs,
                    &mut scratch.dst_mark,
                    dst_gen,
                    &mut dst_ids_buf,
                )
            {
                return false;
            }
            let t_subj_begin = read_usize(target_entry + 0x1a18);
            let t_subj_end = read_usize(target_entry + 0x1a20);
            if t_subj_begin != 0 && t_subj_begin < t_subj_end {
                let t_subjects = std::slice::from_raw_parts(t_subj_begin as *const u64, (t_subj_end - t_subj_begin) / 8);
                for &tag in t_subjects {
                    let sub_idx = (tag >> 32) as i16;
                    let Some(sub) = country_by_idx(sub_idx) else {
                        return false;
                    };
                    let st = subject_type_fn(sub);
                    if st != 0
                        && read_u8(st + 0x151) != 0
                        && !collect_pdx_array_i32(read_usize(sub + 0x1338), read_i32(sub + 0x1344), num_provs, &mut scratch.dst_mark, dst_gen, &mut dst_ids_buf)
                    {
                        return false;
                    }
                }
            }

            if !dst_ids_buf.is_empty() {
                let target_capital = read_usize(target_entry + 0x14b0);
                scratch.dst_near.clear();
                scratch.dst_far.clear();

                let mut best_near_sq = i32::MAX;
                let mut best_any_sq = i32::MAX;

                for &q_id in &dst_ids_buf {
                    let q_idx = q_id as u32 as usize;
                    let runtime_q = runtime_begin + q_idx * RUNTIME_PROV_STRIDE;
                    let sep_q = target_capital != 0 && IS_SEPARATED_FN.get()(runtime_q, target_capital, 1) != 0;

                    let (adj_near, adj_any) = check_target_adjacency(q_id, runtime_q, &scratch.src_mark, &scratch.src_near_mark, src_gen);
                    if adj_any {
                        best_any_sq = 0;
                    }
                    if !sep_q && adj_near {
                        best_near_sq = 0;
                        best_any_sq = 0;
                        break;
                    }

                    let coord = static_prov_coord(static_provs, q_id);
                    if !sep_q {
                        scratch.dst_near.push(coord);
                    } else {
                        scratch.dst_far.push(coord);
                    }
                }

                if best_near_sq != 0 && !scratch.src_near.is_empty() && !scratch.dst_near.is_empty() {
                    for &q in &scratch.dst_near {
                        min_sq_against_sorted(q, &scratch.src_near, map_w, half_w, &mut best_near_sq);
                        if best_near_sq == 0 {
                            break;
                        }
                    }
                    if best_near_sq < best_any_sq {
                        best_any_sq = best_near_sq;
                    }
                }

                if best_any_sq != 0 {
                    if !scratch.src_far.is_empty() {
                        for &q in &scratch.dst_near {
                            min_sq_against_sorted(q, &scratch.src_far, map_w, half_w, &mut best_any_sq);
                            if best_any_sq == 0 {
                                break;
                            }
                        }
                    }
                    if best_any_sq != 0 && !scratch.dst_far.is_empty() {
                        if !scratch.src_near.is_empty() {
                            for &q in &scratch.dst_far {
                                min_sq_against_sorted(q, &scratch.src_near, map_w, half_w, &mut best_any_sq);
                                if best_any_sq == 0 {
                                    break;
                                }
                            }
                        }
                        if best_any_sq != 0 && !scratch.src_far.is_empty() {
                            for &q in &scratch.dst_far {
                                min_sq_against_sorted(q, &scratch.src_far, map_w, half_w, &mut best_any_sq);
                                if best_any_sq == 0 {
                                    break;
                                }
                            }
                        }
                    }
                }

                if best_near_sq != i32::MAX {
                    min_near = sq_dist_to_u16(best_near_sq).min(DEFAULT_DISTANCE);
                }
                if best_any_sq != i32::MAX {
                    min_any = sq_dist_to_u16(best_any_sq).min(DEFAULT_DISTANCE);
                }
                if min_near < min_any {
                    min_any = min_near;
                }
            }
        }

        if record_only {
            scratch.checked_results.push((target_idx as u16 as usize, min_near, min_any));
        } else {
            STORE_BOTH_FN.get()(country, target_tag, min_near, min_any);
            STORE_BOTH_FN.get()(target, self_tag, min_near, min_any);
        }
    }

    true
}

#[called_by_game]
unsafe extern "C" fn rebuild_distances(country: usize, arg2: u8) {
    let m = crate::native::hooked_memory();
    let world = read_usize(m.at(WORLD));
    if world == 0 || read_u8(world + 0x7485) != 0 {
        return;
    }

    if speed::checked(&SPEED_UP) {
        // Run the game's original rebuild first so `PRE_REBUILD_FN` (`0x2d0a00`)
        // refreshes `country + 0x2338` before our fast path reads it, then
        // compare every target country's two distance slots against the result.
        ORIGINAL_REBUILD.get()(country, arg2);
        SCRATCH.with(|cell| {
            let Ok(mut s) = cell.try_borrow_mut() else {
                return;
            };
            if !compute_distances(country, true, &mut s) {
                return;
            }
            let near_buf = read_usize(country + 0x2398) as *const u16;
            let any_buf = read_usize(country + 0x23b0) as *const u16;
            let mut mismatch: Option<(usize, u16, u16, u16, u16)> = None;
            for &(t_idx, exp_near, exp_any) in &s.checked_results {
                let got_near = *near_buf.add(t_idx);
                let got_any = *any_buf.add(t_idx);
                if got_near != exp_near || got_any != exp_any {
                    mismatch = Some((t_idx, exp_near, got_near, exp_any, got_any));
                    break;
                }
            }
            speed::compared(&SPEED_UP, mismatch.is_none(), || {
                let (t_idx, en, gn, ea, ga) = mismatch.unwrap();
                format!("country {country:#x} vs target {t_idx}: fast=({en},{ea}) game=({gn},{ga})")
            });
        });
        return;
    }

    if country == 0 || world == 0 {
        ORIGINAL_REBUILD.get()(country, arg2);
        return;
    }
    // Native pre-checks (`0x2d0b98..0x2d0c35`).
    if read_usize(world + 0x74c8) == 0 && read_u8(world + 0x2521) == 0 {
        PRE_REBUILD_FN.get()(country);
    }
    *(country as *mut u8).add(0x2888) = 0;

    let world_vtable = read_usize(world);
    let get_country_count: WorldCountryCount = std::mem::transmute(read_usize(world_vtable + 0x40));
    if read_usize(country + 0x23a0).saturating_sub(read_usize(country + 0x2398)) < 2 {
        let count = get_country_count(world) as usize;
        RESIZE_U16_VEC_FN.get()(country + 0x2398, count);
    }
    if read_usize(country + 0x23b8).saturating_sub(read_usize(country + 0x23b0)) < 2 {
        let count = get_country_count(world) as usize;
        RESIZE_U16_VEC_FN.get()(country + 0x23b0, count);
    }

    let ok = SCRATCH.with(|cell| {
        let Ok(mut s) = cell.try_borrow_mut() else {
            return false;
        };
        compute_distances(country, false, &mut s)
    });
    if !ok {
        ORIGINAL_REBUILD.get()(country, arg2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_x_pruning_matches_brute_force_across_seam() {
        let map_w = 5632;
        let half_w = map_w / 2;
        let mut src = vec![
            ProvCoord { x: 10, y: 100 },
            ProvCoord { x: 5620, y: 105 },
            ProvCoord { x: 2800, y: 500 },
            ProvCoord { x: 1200, y: 300 },
            ProvCoord { x: 4000, y: 900 },
            ProvCoord { x: 5630, y: 102 },
        ];
        src.sort_unstable_by_key(|p| p.x);

        let targets = [
            ProvCoord { x: 2, y: 101 },
            ProvCoord { x: 5629, y: 100 },
            ProvCoord { x: 2816, y: 499 },
            ProvCoord { x: 1200, y: 300 },
            ProvCoord { x: 3000, y: 1000 },
        ];

        for &t in &targets {
            let brute = src.iter().map(|&p| wrapped_sq_dist(p, t, map_w, half_w)).min().unwrap();
            let mut fast = i32::MAX;
            min_sq_against_sorted(t, &src, map_w, half_w, &mut fast);
            assert_eq!(fast, brute, "mismatch for target {t:?}");
        }
    }

    #[test]
    fn sq_dist_to_u16_matches_native_rounding() {
        assert_eq!(sq_dist_to_u16(0), 0);
        assert_eq!(sq_dist_to_u16(100), 10);
        // sqrt(2) = 1.4142135 -> *1000 + 0.5 = 1414.7135 -> /1000 = 1
        assert_eq!(sq_dist_to_u16(2), 1);
        // sqrt(999999) = 999.9995 -> *1000 + 0.5 = 1000000.0 -> /1000 = 1000
        assert_eq!(sq_dist_to_u16(999_999), 1000);
    }
}
