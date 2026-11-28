//! Faster DLC verification when `dlc_signature` misses (docs/game/program/loading.md).
//!
//! On Steam launches (`[dlc_manager + 0xe0] == 0`), `0x150b8d0` hashes all
//! non-exempt `.dlc` descriptors via `0x15098b0(..., 1)` and compares with
//! `dlc_signature`. Whenever any installed `.dlc` fails verification (for
//! example, unowned depots left on disk after a subscription ends), the final
//! `0x15098b0(..., 0)` writes only the verified subset, so every subsequent
//! launch misses `dlc_signature` and runs `0x150ae80` sequentially over all
//! DLCs — reading gigabytes of `.zip` archives single-threaded through
//! `0x1509d20`.
//!
//! This speed-up is stateless and preserves every ownership and integrity
//! check identically:
//! - Before the miss loop computes any archive MD5, it queries the same store
//!   backend (`[mgr + 0x08]`, `0x15049c0`, vtable `+0xa8` / `+0x20`) on the
//!   main thread. For any DLC that will fail the store lookup or ownership
//!   check, `0x1509d20` returns the descriptor's expected checksum so
//!   `0x150ae80` skips the wasted archive scan and proceeds directly to the
//!   game's own store check, logging, and failure status (`1`).
//! - Every store-owned DLC is hashed live in parallel across worker threads
//!   (sorted largest-archive-first) via the game's own `0x1509d20`, then
//!   checked normally by `0x150ae80` against `[dlc + 0x140]` and the live
//!   store backend.
//! - Checked (`checked = ["dlc-verify"]`), no MD5 is skipped and every
//!   parallel digest is compared against the game's own `0x1509d20`.
use crate::base::called_by_game;
use crate::native::game_string;
use crate::native::hooks::Call;
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::collections::HashMap;
use std::ffi::CString;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

/// `0x150b8d0(mgr, flag)`: top-level DLC verification pass.
const VERIFY_ALL: usize = 0x150b8d0;
/// `0x150ae80(mgr, dlc, platform, flag) -> u32`: verifies one DLC's archive MD5
/// and store ownership (`0` = verified, non-zero = failed).
const VERIFY_ONE: usize = 0x150ae80;
/// `0x1509d20(dlc, out_std_string) -> out_std_string`: computes the DLC's MD5.
const HASH_DLC: usize = 0x1509d20;
/// `0x15049c0(out_std_string, dlc) -> out_std_string`: formats the DLC's store ID.
const STORE_ID: usize = 0x15049c0;

/// Offsets in `CDLCManager` (`0x150b8f9`, `0x150b922`).
const MGR_STORE: usize = 0x08;
const MGR_TREE_HEAD: usize = 0xc0;
/// Offsets in the RB-tree node (`0x150bd13`, `0x150bd58`).
const NODE_PARENT: usize = 0x08;
const NODE_RIGHT: usize = 0x10;
const NODE_IS_NIL: usize = 0x19;
const NODE_DLC: usize = 0x40;
const NODE_EXEMPT: usize = 0x370;
/// Offsets in `CDLC` (`0x150a86e`, `0x150aed5`, `0x150aebf`).
const DLC_ARCHIVE_PATH: usize = 0xd0;
const DLC_CHECKSUM: usize = 0x140;
const DLC_STORE_KIND: usize = 0x2b0;
/// Store backend vtable slots (`0x150b103`, `0x150b11b`).
const STORE_VT_FIND: usize = 0xa8;
const ITEM_VT_OWNED: usize = 0x20;

pub const SPEED_UP: SpeedUp = SpeedUp::new("dlc-verify", "loading", points, true);

type VerifyOne = unsafe extern "C" fn(usize, usize, i32, u8) -> u32;
type HashDlc = unsafe extern "C" fn(usize, usize) -> usize;
type StoreId = unsafe extern "C" fn(usize, usize) -> usize;
type StoreFind = unsafe extern "C" fn(usize, usize) -> usize;
type ItemOwned = unsafe extern "C" fn(usize) -> u8;

static ORIGINAL_VERIFY_ONE: Function<VerifyOne> = Function::new();
static ORIGINAL_HASH: Function<HashDlc> = Function::new();
static ORIGINAL_STORE_ID: Function<StoreId> = Function::new();

fn points() -> Vec<Point> {
    vec![
        Point::observe(
            VERIFY_ALL,
            "88 54 24 10 48 89 4c 24 08 53 56 57 41 54 41 55",
            "DLC verification pass",
            2,
            Some(before_verify_all),
            Some(after_verify_all),
        ),
        Point::replace(VERIFY_ONE, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 44", "single DLC verification", verify_one as VerifyOne, &ORIGINAL_VERIFY_ONE),
        Point::replace(HASH_DLC, "48 89 5c 24 18 48 89 54 24 10 55 56 57 41 54 41", "DLC archive MD5 computation", hash_dlc as HashDlc, &ORIGINAL_HASH),
        Point::keep(STORE_ID, "48 89 4c 24 08 53 48 83 ec 40 48 8b d9 8b 82 b0 02 00 00", "DLC store ID formatter")
            .recorded(ORIGINAL_STORE_ID.slot())
            .uses(&[DLC_STORE_KIND]),
        Point::keep(0x150b8f9usize, "48 8b 49 08 48 85 c9 74", "DLC manager store backend pointer").uses(&[MGR_STORE]),
        Point::keep(0x150b922usize, "49 8b 8e c0 00 00 00", "DLC manager tree head").uses(&[MGR_TREE_HEAD]),
        Point::keep(0x150bd13usize, "80 bb 70 03 00 00 00 75 32 44 8b 84 bc 78 02 00 00 48 8d 53 40", "DLC tree node exempt flag and payload offset")
            .uses(&[NODE_EXEMPT, NODE_DLC]),
        Point::keep(0x150bd58usize, "48 8b 43 10 80 78 19 00 74 26 48 8b 43 08 80 78 19 00", "DLC tree node links").uses(&[
            NODE_RIGHT,
            NODE_PARENT,
            NODE_IS_NIL,
        ]),
        Point::keep(0x150aebfusize, "83 ba b0 02 00 00 0b", "DLC store kind check").uses(&[DLC_STORE_KIND]),
        Point::keep(0x150aed5usize, "49 8d 8e 40 01 00 00", "DLC expected checksum offset").uses(&[DLC_CHECKSUM]),
        Point::keep(0x150b103usize, "ff 90 a8 00 00 00", "store backend FindItemById slot").uses(&[STORE_VT_FIND]),
        Point::keep(0x150b11busize, "ff 52 20", "store item IsOwned slot").uses(&[ITEM_VT_OWNED]),
        Point::keep(0x150a86eusize, "49 8d 9d d0 00 00 00", "DLC archive path offset").uses(&[DLC_ARCHIVE_PATH]),
    ]
}

enum Prepared {
    /// Store check will fail, so `0x1509d20` yields the expected checksum and
    /// `0x150ae80` proceeds directly to its native store check and logging.
    SkipForStoreFail,
    /// Precomputed MD5 digest to return from `0x1509d20`.
    Digest(String),
}

struct BatchState {
    in_verify_all: bool,
    prepared_for: Option<(usize, i32)>,
    entries: HashMap<usize, Prepared>,
}

static BATCH: LazyLock<Mutex<BatchState>> = LazyLock::new(|| Mutex::new(BatchState { in_verify_all: false, prepared_for: None, entries: HashMap::new() }));

fn before_verify_all(_: &mut Call) {
    let mut st = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    st.in_verify_all = true;
    st.prepared_for = None;
    st.entries.clear();
}

fn after_verify_all(_: &mut Call) {
    let mut st = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    st.in_verify_all = false;
    st.prepared_for = None;
    st.entries.clear();
}

/// Walks the MSVC `std::map` RB-tree at `[mgr + 0xc0]` in order and returns
/// `node + 0x40` (`CDLC*`) for every non-exempt entry (`[node + 0x370] == 0`).
unsafe fn collect_dlcs(mgr: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let head = unsafe { *((mgr + MGR_TREE_HEAD) as *const usize) };
    if head == 0 {
        return out;
    }
    let mut cur = unsafe { *(head as *const usize) };
    let mut steps = 0usize;
    while cur != 0 && cur != head {
        steps += 1;
        if steps > 10_000 {
            out.clear();
            return out;
        }
        let exempt = unsafe { *((cur + NODE_EXEMPT) as *const u8) };
        if exempt == 0 {
            out.push(cur + NODE_DLC);
        }
        let right = unsafe { *((cur + NODE_RIGHT) as *const usize) };
        if right != 0 && unsafe { *((right + NODE_IS_NIL) as *const u8) } == 0 {
            cur = right;
            let mut left = unsafe { *(cur as *const usize) };
            while left != 0 && unsafe { *((left + NODE_IS_NIL) as *const u8) } == 0 {
                cur = left;
                left = unsafe { *(cur as *const usize) };
            }
        } else {
            let mut parent = unsafe { *((cur + NODE_PARENT) as *const usize) };
            while parent != 0 && unsafe { *((parent + NODE_IS_NIL) as *const u8) } == 0 && cur == unsafe { *((parent + NODE_RIGHT) as *const usize) } {
                cur = parent;
                parent = unsafe { *((cur + NODE_PARENT) as *const usize) };
            }
            cur = parent;
        }
    }
    out
}

/// Returns `true` when `0x150ae80` is guaranteed to fail the store backend
/// lookup or ownership check regardless of the archive checksum.
unsafe fn store_will_fail(mgr: usize, dlc: usize, platform: i32) -> bool {
    if !matches!(platform, 1 | 3 | 5 | 7 | 9 | 10) {
        return false;
    }
    let store = unsafe { *((mgr + MGR_STORE) as *const usize) };
    if store == 0 {
        // Without a store backend (offline / non-standard launcher), let the
        // game run its own archive hash rather than fabricating a checksum.
        return false;
    }
    let store_vt = unsafe { *(store as *const usize) };
    if store_vt == 0 {
        return true;
    }
    let find_fn: StoreFind = unsafe { std::mem::transmute(*((store_vt + STORE_VT_FIND) as *const usize)) };
    let mut id_str = game_string::empty();
    let id_ptr = id_str.as_mut_ptr() as usize;
    unsafe { ORIGINAL_STORE_ID.get()(id_ptr, dlc) };
    let item = unsafe { find_fn(store, id_ptr) };
    let _ = game_string::destroy_checked(&crate::native::hooked_memory(), id_ptr);
    if item == 0 {
        return true;
    }
    let item_vt = unsafe { *(item as *const usize) };
    if item_vt == 0 {
        return true;
    }
    let owned_fn: ItemOwned = unsafe { std::mem::transmute(*((item_vt + ITEM_VT_OWNED) as *const usize)) };
    unsafe { owned_fn(item) == 0 }
}

fn read_game_str(ptr: usize) -> Option<String> {
    let bytes = game_string::read(&crate::native::hooked_memory(), ptr).ok()?;
    String::from_utf8(bytes).ok()
}

fn archive_size(dlc: usize) -> u64 {
    let Some(rel) = read_game_str(dlc + DLC_ARCHIVE_PATH) else { return 0 };
    if rel.is_empty() {
        return 0;
    }
    let Ok(cstr) = CString::new(rel.as_str()) else { return 0 };
    let Some(dir) = crate::native::physfs::real_dir(cstr.as_ptr()) else { return 0 };
    std::fs::metadata(Path::new(&dir).join(rel)).map_or(0, |m| m.len())
}

unsafe fn compute_native_digest(dlc: usize) -> Option<String> {
    let mut raw = game_string::empty();
    let ptr = raw.as_mut_ptr() as usize;
    unsafe { ORIGINAL_HASH.get()(dlc, ptr) };
    let mem = crate::native::hooked_memory();
    let digest = game_string::read(&mem, ptr).ok().and_then(|b| String::from_utf8(b).ok());
    let _ = game_string::destroy_checked(&mem, ptr);
    digest
}

struct WorkItem {
    dlc: usize,
    zip_size: u64,
}

fn prepare_batch(mgr: usize, platform: i32) {
    let dlcs = unsafe { collect_dlcs(mgr) };
    if dlcs.is_empty() {
        return;
    }
    let checked = speed::checked(&SPEED_UP);
    let mut prepared: HashMap<usize, Prepared> = HashMap::with_capacity(dlcs.len());
    let mut to_compute: Vec<WorkItem> = Vec::new();

    for dlc in dlcs {
        let store_kind = unsafe { *((dlc + DLC_STORE_KIND) as *const i32) };
        if store_kind == 0xb {
            continue;
        }
        if !checked && unsafe { store_will_fail(mgr, dlc, platform) } {
            prepared.insert(dlc, Prepared::SkipForStoreFail);
            continue;
        }
        to_compute.push(WorkItem { dlc, zip_size: archive_size(dlc) });
    }

    if !to_compute.is_empty() {
        // Longest-processing-time-first keeps large archives from creating a tail.
        to_compute.sort_by_key(|item| std::cmp::Reverse(item.zip_size));
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8).min(to_compute.len());
        let results: Mutex<Vec<(usize, String)>> = Mutex::new(Vec::with_capacity(to_compute.len()));

        if workers <= 1 {
            for item in &to_compute {
                if let Some(digest) = unsafe { compute_native_digest(item.dlc) } {
                    results.lock().unwrap_or_else(|e| e.into_inner()).push((item.dlc, digest));
                }
            }
        } else {
            let next = AtomicUsize::new(0);
            let items = &to_compute;
            std::thread::scope(|scope| {
                for _ in 0..workers {
                    let next = &next;
                    let results = &results;
                    scope.spawn(move || {
                        crate::base::guarded("DLC archive hash worker", || {
                            let mut local = Vec::new();
                            loop {
                                let idx = next.fetch_add(1, Ordering::Relaxed);
                                let Some(item) = items.get(idx) else { break };
                                if let Some(digest) = unsafe { compute_native_digest(item.dlc) } {
                                    local.push((item.dlc, digest));
                                }
                            }
                            if !local.is_empty() {
                                results.lock().unwrap_or_else(|e| e.into_inner()).extend(local);
                            }
                        });
                    });
                }
            });
        }

        for (dlc, digest) in results.into_inner().unwrap_or_else(|e| e.into_inner()) {
            prepared.insert(dlc, Prepared::Digest(digest));
        }
    }

    let mut st = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    st.prepared_for = Some((mgr, platform));
    st.entries = prepared;
}

#[called_by_game]
unsafe extern "C" fn verify_one(mgr: usize, dlc: usize, platform: i32, flag: u8) -> u32 {
    crate::base::guarded("DLC verification", || {
        let needs_prepare = {
            let st = BATCH.lock().unwrap_or_else(|e| e.into_inner());
            st.in_verify_all && st.prepared_for != Some((mgr, platform))
        };
        if needs_prepare {
            prepare_batch(mgr, platform);
        }
        unsafe { ORIGINAL_VERIFY_ONE.get()(mgr, dlc, platform, flag) }
    })
}

#[called_by_game]
unsafe extern "C" fn hash_dlc(dlc: usize, out_str: usize) -> usize {
    crate::base::guarded("DLC archive MD5", || {
        let entry = {
            let mut st = BATCH.lock().unwrap_or_else(|e| e.into_inner());
            st.entries.remove(&dlc)
        };
        match entry {
            Some(Prepared::SkipForStoreFail) => {
                if let Ok(expected) = game_string::read(&crate::native::hooked_memory(), dlc + DLC_CHECKSUM) {
                    unsafe { game_string::construct(out_str as *mut u8, &expected) };
                    return out_str;
                }
                unsafe { ORIGINAL_HASH.get()(dlc, out_str) }
            }
            Some(Prepared::Digest(digest)) => {
                if speed::checked(&SPEED_UP) {
                    let ret = unsafe { ORIGINAL_HASH.get()(dlc, out_str) };
                    let game = read_game_str(out_str).unwrap_or_default();
                    speed::compared(&SPEED_UP, game == digest, || format!("dlc {dlc:#x}: prepared {digest:?}, game {game:?}"));
                    return ret;
                }
                unsafe { game_string::construct(out_str as *mut u8, digest.as_bytes()) };
                out_str
            }
            None => unsafe { ORIGINAL_HASH.get()(dlc, out_str) },
        }
    })
}
