//! Trigger optimization profile lookup, parallel `InitGame` batch pass, and
//! background registry teardown (docs/game/program/localisation-loading.md,
//! docs/game/program/loading.md, docs/plans/README.md, M2).
//!
//! At the start of `0x208620`, `0xb9b5a0` loads `trigger_profile.txt` (~6,593
//! trigger trees with historical costs) into `[0x242be60] + 0x48`. At the end
//! of `InitGame` (`0x205bed..0x205c94`), while holding `[0x23582b0]`, the game
//! walks both lists of `[0x2340010]` (`+8` top-level triggers and `+0x48`
//! nested triggers):
//!
//! 1. Calls `0xb9b4e0(manager, trigger)` and stores the returned cost at
//!    `[trigger + 0x58]`.
//! 2. Calls `0xb980a0(trigger)`, which stably sorts `[trigger + 8]` (length
//!    `[trigger + 0x14]`) in ascending order of `[child + 0x58]` whenever
//!    `manager + 0x58` is non-zero and every direct child has a non-zero cost.
//! 3. Releases `[0x23582b0]` and calls `0xb89560()` to free `[0x2340010]`.
//!
//! A `CTriggerProfileNode` has only a 32-bit trigger type (`+0`) and an
//! unordered set of distinct child `CTriggerProfileNode`s (`+8`), so every
//! profile tree has a unique canonical structural ID when interned bottom-up
//! by `(type_id, sorted_deduped_child_ids)`. After `0xb9b5a0` loads the
//! profile table, the framework interns the profile trees into a read-only
//! table (`ProfileIndex`) keyed with a fast non-cryptographic hasher and
//! allocation-free slice lookups:
//!
//! - In normal mode, `0x205bed` runs a two-phase scoped worker pool across all
//!   deduplicated trigger pointers from `[0x2340010]`: phase 1 computes and
//!   writes `[trigger + 0x58]` with per-thread memoization, then a barrier
//!   precedes phase 2, which stably sorts each trigger's own child array
//!   `[trigger + 8]` identically to `0xbbd220`/`0xbbd310`.
//! - `0xb89560` detaches `[0x2340010]` (writing `0`) and frees the detached
//!   containers on a background thread so `InitGame` proceeds immediately to
//!   effect validation.
//! - Checked (`checked = ["trigger-profile"]`), `0x205bed` falls back to the
//!   native loop so every `0xb9b4e0` call is compared against the game's own
//!   implementation.
use crate::base::called_by_game;
use crate::native::hooks::Call;
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::JoinHandle;

/// `0xb9b4e0(manager, trigger) -> u64`: looks `trigger` up in `manager + 0x48`.
const LOOKUP: usize = 0xb9b4e0;
/// `0xb9b380() -> manager`: singleton getter for `[0x242be60]`.
const GET_MANAGER: usize = 0xb9b380;
/// `0xb9b5a0(manager)`: loads `trigger_profile.txt` into `manager + 0x48`.
const LOAD_PROFILE: usize = 0xb9b5a0;
/// `0xb89560()`: frees `[0x2340010]` after `InitGame`'s trigger pass.
const FREE_REGISTERED: usize = 0xb89560;
/// `0xb9dae0(_, manager)`: destroys a profile manager.
const DESTROY_MANAGER: usize = 0xb9dae0;
/// `InitGame` trigger-profile and child-sort loops (`0x205bed..0x205c94`).
const BATCH_LOOP: usize = 0x205bed;
const BATCH_LIST1_BODY: usize = 0x205c00;
const BATCH_LIST2: usize = 0x205c23;
const BATCH_DONE: usize = 0x205c94;
/// `[0x2340010]`: registered trigger set container freed by `0xb89560`.
const REGISTERED_TRIGGERS: usize = 0x2340010;
/// Native helpers called by `0xb89560` to free `[0x2340010]`.
const FREE_BUCKETS: usize = 0xc03b0;
const FREE_LIST: usize = 0xc04d0;
/// Offsets in `[0x2340010]`: two `std::unordered_set`s whose list sentinels
/// sit at `+0x08` and `+0x48`, and bucket vectors at `+0x18` and `+0x58`.
const REGISTRY_LIST1: usize = 0x08;
const REGISTRY_BUCKETS1: usize = 0x18;
const REGISTRY_LIST2: usize = 0x48;
const REGISTRY_BUCKETS2: usize = 0x58;
const REGISTRY_SIZE: usize = 0x80;
/// Offsets in a trigger (`0xb9c160`, `0x205c14`, `0xb980f0`).
const TRIGGER_CHILDREN: usize = 0x08;
const TRIGGER_CHILD_COUNT: usize = 0x14;
const TRIGGER_TYPE: usize = 0x50;
const TRIGGER_COST: usize = 0x58;
/// Minimum trigger count to fan out across worker threads.
const PARALLEL_THRESHOLD: usize = 2048;

/// Offsets in `manager + 0x48` (`0xb9b4e0`, `0xbbc0e0`): an MSVC
/// `std::unordered_map` / `unordered_set` whose list sentinel is at `+0x08`
/// (`manager + 0x50`) and element count at `+0x10` (`manager + 0x58`). Each
/// list node has `next` at `+0x00`, embedded `CTriggerProfileNode` at `+0x10`,
/// and `u64` cost at `+0x58`.
const MANAGER_HEAD: usize = 0x50;
const MANAGER_COUNT: usize = 0x58;
const ENTRY_NODE: usize = 0x10;
const ENTRY_COST: usize = 0x58;
/// Offsets in `CTriggerProfileNode` (`0xb9c210`, `0xb9ca00`): `+0x00` `u32`
/// type ID, `+0x10` child-set list sentinel (`next` at `+0x00`, child
/// `CTriggerProfileNode*` at `+0x10`), `+0x18` child count.
const NODE_CHILDREN_HEAD: usize = 0x10;

pub const SPEED_UP: SpeedUp = SpeedUp { name: "trigger-profile", group: "loading", points, checkable: true };

type Lookup = unsafe extern "C" fn(usize, usize) -> u64;
type GetManager = unsafe extern "C" fn() -> usize;
type FreeRegistered = unsafe extern "C" fn();
type FreeSub = unsafe extern "C" fn(usize);

static ORIGINAL_LOOKUP: Function<Lookup> = Function::new();
static ORIGINAL_GET_MANAGER: Function<GetManager> = Function::new();
static ORIGINAL_FREE_REGISTERED: Function<FreeRegistered> = Function::new();
static ORIGINAL_FREE_BUCKETS: Function<FreeSub> = Function::new();
static ORIGINAL_FREE_LIST: Function<FreeSub> = Function::new();

resume!(BATCH_RESUME: BATCH_LIST1_BODY_AT = BATCH_LIST1_BODY, BATCH_LIST2_AT = BATCH_LIST2, BATCH_DONE_AT = BATCH_DONE);

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            LOOKUP,
            "48 89 5c 24 08 48 89 74 24 10 57 48 81 ec 80 00 00 00 48 8b f1",
            "trigger optimization profile lookup",
            lookup as Lookup,
            &ORIGINAL_LOOKUP,
        ),
        Point::keep(
            GET_MANAGER,
            "48 89 5c 24 18 48 89 74 24 20 57 48 83 ec 20 48 8b 05 ca 0a 89 01",
            "trigger optimization profile manager getter",
        )
        .recorded(ORIGINAL_GET_MANAGER.slot()),
        Point::jump(
            BATCH_LOOP,
            "49 8b 75 08 48 8b 1e 48 3b de 74 2a",
            "InitGame trigger profile and sort loops",
            batch_stub as *const () as usize,
            &BATCH_RESUME,
        )
        .uses(&[REGISTRY_LIST1]),
        Point::keep(
            BATCH_LIST1_BODY,
            "48 8b 7b 10 e8 77 57 99 00 48 8b d7 48 8b c8 e8 cc 58 99 00 48 89 47 58",
            "trigger profile first loop body and cost write",
        )
        .uses(&[TRIGGER_COST]),
        Point::keep(BATCH_LIST2, "49 8b 75 48 48 8b 1e 48 3b de 74 24", "trigger profile second loop head").uses(&[REGISTRY_LIST2]),
        Point::keep(0xb980f0usize, "49 8b 00 48 83 78 58 00 0f 84 d7 00 00 00", "trigger sort checks child cost").uses(&[TRIGGER_COST]),
        Point::keep(0x205c94usize, "48 8b 0d 15 26 15 02 e8 50 dd 80 01 e8 bb 38 98 00", "trigger loops unlock and free registry"),
        // `0xb9c160` reads `[trigger + 0x50]`, `[trigger + 0x08]` and `[trigger + 0x14]`.
        Point::keep(0xb9c175usize, "8b 42 50 89 01 48 83 c1 08 e8 9d 1d 02 00 90 48 8b 7b 08 48 63 43 14", "trigger fields read for profile")
            .uses(&[TRIGGER_TYPE, TRIGGER_CHILDREN, TRIGGER_CHILD_COUNT]),
        // `0xb9b4e0` reads `[manager + 0x50]` (sentinel) and `[entry + 0x58]` (cost).
        Point::keep(
            0xb9b54busize,
            "48 8b 5e 50 48 8d 4c 24 50 e8 57 4e 52 ff 48 8b 54 24 40 e8 7d 2a 02 00 ba 18 00 00 00 48 8b 4c 24 40 e8 7e df b9 00 48 3b 5e 50 74 06 48 8b 43 58",
            "trigger profile sentinel and cost",
        )
        .uses(&[MANAGER_HEAD, ENTRY_COST]),
        // `0xb9c210` walks `[node + 0x10]` and reads `[item + 0x10]`.
        Point::keep(0xb9c233usize, "48 8b 73 10 48 8b 1e 48 3b de 74 63 90 48 8b 7b 10", "profile node child list").uses(&[NODE_CHILDREN_HEAD, ENTRY_NODE]),
        Point::observe(
            LOAD_PROFILE,
            "48 89 5c 24 08 48 89 74 24 18 48 89 7c 24 20 55",
            "trigger optimization profile loaded",
            1,
            None,
            Some(invalidate_all),
        ),
        Point::replace(
            FREE_REGISTERED,
            "40 53 48 83 ec 20 48 8b 1d a3 6a 7b 01 48 85 db",
            "registered triggers freed",
            free_registered as FreeRegistered,
            &ORIGINAL_FREE_REGISTERED,
        )
        .uses(&[REGISTERED_TRIGGERS]),
        Point::keep(FREE_BUCKETS, "40 53 48 83 ec 20 48 8b d9 48 8b 09 48 8b 53 08", "trigger registry bucket free")
            .recorded(ORIGINAL_FREE_BUCKETS.slot()),
        Point::keep(FREE_LIST, "40 57 48 83 ec 20 48 8b 11 48 8b f9 48 8b 42 08", "trigger registry list free").recorded(ORIGINAL_FREE_LIST.slot()),
        Point::observe(DESTROY_MANAGER, "48 85 d2 0f 84 88 00 00 00 53 48 83 ec 20 48 8d", "trigger profile manager destroyed", 6, Some(invalidate_all), None),
    ]
}

/// Fast non-cryptographic hasher (FxHash) for internal pointer and trigger-ID maps.
#[derive(Default)]
struct FxHasher(u64);

impl FxHasher {
    const K: u64 = 0x517c_c1b7_2722_0a95;

    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            let word = u64::from_ne_bytes(bytes[..8].try_into().unwrap());
            self.add(word);
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            let word = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as u64;
            self.add(word);
            bytes = &bytes[4..];
        }
        for &b in bytes {
            self.add(b as u64);
        }
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// Canonical interned representation of a loaded `trigger_profile.txt` table.
struct ProfileIndex {
    manager: usize,
    head: usize,
    count: usize,
    /// 4096-bit bloom filter of every `u32` trigger type ID that appears
    /// anywhere in the profile (roots or descendants) for $O(1)$ rejection.
    type_bits: [u64; 64],
    leaf_ids: FastMap<u32, u32>,
    /// Keyed by parent `type_id`, then sorted deduplicated child canonical IDs.
    /// Splitting `type_id` out lets lookups borrow `&[u32]` directly via
    /// `Box<[u32]>: Borrow<[u32]>` with zero heap allocation.
    compound_ids: FastMap<u32, FastMap<Box<[u32]>, u32>>,
    /// Cost for each root canonical ID (`0` when only a proper subtree).
    costs: Vec<u64>,
}

impl ProfileIndex {
    fn new(manager: usize, head: usize, count: usize) -> Self {
        Self { manager, head, count, type_bits: [0; 64], leaf_ids: FastMap::default(), compound_ids: FastMap::default(), costs: Vec::new() }
    }

    #[inline]
    fn mark_type(&mut self, ty: u32) {
        let h = ty.wrapping_mul(0x9e37_79b9) >> 20;
        self.type_bits[(h >> 6) as usize] |= 1u64 << (h & 63);
    }

    #[inline]
    fn may_have_type(&self, ty: u32) -> bool {
        let h = ty.wrapping_mul(0x9e37_79b9) >> 20;
        (self.type_bits[(h >> 6) as usize] & (1u64 << (h & 63))) != 0
    }

    fn intern(&mut self, ty: u32, mut children: Vec<u32>) -> u32 {
        self.mark_type(ty);
        if children.is_empty() {
            if let Some(&id) = self.leaf_ids.get(&ty) {
                return id;
            }
            let id = self.costs.len() as u32;
            self.costs.push(0);
            self.leaf_ids.insert(ty, id);
            return id;
        }
        children.sort_unstable();
        children.dedup();
        let by_ty = self.compound_ids.entry(ty).or_default();
        if let Some(&id) = by_ty.get(children.as_slice()) {
            return id;
        }
        let id = self.costs.len() as u32;
        self.costs.push(0);
        by_ty.insert(children.into_boxed_slice(), id);
        id
    }

    /// Reads one `CTriggerProfileNode` from the game's loaded profile table
    /// and interns it recursively.
    unsafe fn intern_game_node(&mut self, node: usize, depth: usize) -> Option<u32> {
        if node == 0 || depth > 128 {
            return None;
        }
        let ty = unsafe { *(node as *const u32) };
        let child_head = unsafe { *((node + NODE_CHILDREN_HEAD) as *const usize) };
        if child_head == 0 {
            return None;
        }
        let mut children = Vec::new();
        let mut cur = unsafe { *(child_head as *const usize) };
        let mut steps = 0usize;
        while cur != child_head {
            steps += 1;
            if cur == 0 || steps > 100_000 {
                return None;
            }
            let child_node = unsafe { *((cur + ENTRY_NODE) as *const usize) };
            children.push(unsafe { self.intern_game_node(child_node, depth + 1) }?);
            cur = unsafe { *(cur as *const usize) };
        }
        Some(self.intern(ty, children))
    }

    /// Builds the index from `manager + 0x48`.
    unsafe fn from_manager(manager: usize) -> Option<Self> {
        if manager == 0 {
            return None;
        }
        let head = unsafe { *((manager + MANAGER_HEAD) as *const usize) };
        let count = unsafe { *((manager + MANAGER_COUNT) as *const usize) };
        if head == 0 || count > 1_000_000 {
            return None;
        }
        let mut index = Self::new(manager, head, count);
        let mut cur = unsafe { *(head as *const usize) };
        let mut seen = 0usize;
        while cur != head {
            seen += 1;
            if cur == 0 || seen > count + 1 {
                return None;
            }
            let id = unsafe { index.intern_game_node(cur + ENTRY_NODE, 0) }?;
            let cost = unsafe { *((cur + ENTRY_COST) as *const u64) };
            index.costs[id as usize] = cost;
            cur = unsafe { *(cur as *const usize) };
        }
        if seen != count {
            return None;
        }
        Some(index)
    }

    /// Matches a live `CTrigger` against the interned profile nodes, using
    /// `scratch` stacks to avoid heap allocations and `memo` (`u32::MAX` = no
    /// match in the profile) to avoid re-walking shared/nested sub-triggers.
    unsafe fn match_trigger(&self, trigger: usize, memo: &mut FastMap<usize, u32>, scratch: &mut Vec<u32>, depth: usize) -> Option<u32> {
        if trigger == 0 || depth > 128 {
            return None;
        }
        if let Some(&cached) = memo.get(&trigger) {
            return (cached != u32::MAX).then_some(cached);
        }
        let ty = unsafe { *((trigger + TRIGGER_TYPE) as *const u32) };
        if !self.may_have_type(ty) {
            memo.insert(trigger, u32::MAX);
            return None;
        }
        let count_raw = unsafe { *((trigger + TRIGGER_CHILD_COUNT) as *const i32) };
        if count_raw <= 0 {
            let found = self.leaf_ids.get(&ty).copied();
            memo.insert(trigger, found.unwrap_or(u32::MAX));
            return found;
        }
        let Some(by_ty) = self.compound_ids.get(&ty) else {
            memo.insert(trigger, u32::MAX);
            return None;
        };
        let children_ptr = unsafe { *((trigger + TRIGGER_CHILDREN) as *const usize) };
        if children_ptr == 0 {
            memo.insert(trigger, u32::MAX);
            return None;
        }
        let count = count_raw as usize;
        let base = scratch.len();
        for i in 0..count {
            let child = unsafe { *((children_ptr + i * 8) as *const usize) };
            let Some(child_id) = (unsafe { self.match_trigger(child, memo, scratch, depth + 1) }) else {
                scratch.truncate(base);
                memo.insert(trigger, u32::MAX);
                return None;
            };
            scratch.push(child_id);
        }
        scratch[base..].sort_unstable();
        // Deduplicate `scratch[base..]` in place.
        let mut write = base + 1;
        for read in (base + 1)..scratch.len() {
            if scratch[read] != scratch[write - 1] {
                scratch[write] = scratch[read];
                write += 1;
            }
        }
        scratch.truncate(write);
        let found = by_ty.get(&scratch[base..]).copied();
        scratch.truncate(base);
        memo.insert(trigger, found.unwrap_or(u32::MAX));
        found
    }

    unsafe fn lookup_cost(&self, trigger: usize, memo: &mut FastMap<usize, u32>, scratch: &mut Vec<u32>) -> u64 {
        if self.count == 0 {
            return 0;
        }
        match unsafe { self.match_trigger(trigger, memo, scratch, 0) } {
            Some(id) => self.costs[id as usize],
            None => 0,
        }
    }
}

static BUILT: Mutex<Option<Arc<ProfileIndex>>> = Mutex::new(None);
static BG_FREE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

struct ThreadMemo {
    manager: usize,
    head: usize,
    count: usize,
    memo: FastMap<usize, u32>,
    scratch: Vec<u32>,
}

thread_local! {
    static MEMO: RefCell<ThreadMemo> = RefCell::new(ThreadMemo {
        manager: 0,
        head: 0,
        count: 0,
        memo: FastMap::default(),
        scratch: Vec::new(),
    });
}

fn invalidate_all(_: &mut Call) {
    *BUILT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    clear_thread_memo();
}

fn clear_thread_memo() {
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        m.memo.clear();
        m.memo.shrink_to_fit();
        m.scratch.clear();
    });
}

unsafe fn index_for(manager: usize) -> Option<Arc<ProfileIndex>> {
    if manager == 0 {
        return None;
    }
    let head = unsafe { *((manager + MANAGER_HEAD) as *const usize) };
    let count = unsafe { *((manager + MANAGER_COUNT) as *const usize) };
    {
        let guard = BUILT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = guard.as_ref().filter(|idx| idx.manager == manager && idx.head == head && idx.count == count) {
            return Some(existing.clone());
        }
    }
    let built = Arc::new(unsafe { ProfileIndex::from_manager(manager) }?);
    let mut guard = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(built.clone());
    Some(built)
}

/// Appends every non-null trigger pointer from a `std::unordered_set` list
/// sentinel (`next` at `+0x00`, payload `CTrigger*` at `+0x10`).
unsafe fn collect_list(sentinel: usize, out: &mut Vec<usize>) -> bool {
    if sentinel == 0 {
        return true;
    }
    let mut cur = unsafe { *(sentinel as *const usize) };
    let mut steps = 0usize;
    while cur != sentinel {
        steps += 1;
        if cur == 0 || steps > 20_000_000 {
            return false;
        }
        let trigger = unsafe { *((cur + 0x10) as *const usize) };
        if trigger != 0 {
            out.push(trigger);
        }
        cur = unsafe { *(cur as *const usize) };
    }
    true
}

/// Stably sorts `trigger`'s direct child pointers in ascending order of
/// `[child + 0x58]` when all direct children have a non-zero cost, matching
/// `0xb980a0` (`0xbbd220` / `0xbbd310`).
#[inline]
unsafe fn sort_trigger_children(trigger: usize) {
    let count = unsafe { *((trigger + TRIGGER_CHILD_COUNT) as *const i32) };
    if count <= 1 {
        return;
    }
    let children_ptr = unsafe { *((trigger + TRIGGER_CHILDREN) as *const usize) };
    if children_ptr == 0 {
        return;
    }
    let kids = unsafe { std::slice::from_raw_parts_mut(children_ptr as *mut usize, count as usize) };
    for &child in kids.iter() {
        if child == 0 || unsafe { *((child + TRIGGER_COST) as *const u64) } == 0 {
            return;
        }
    }
    kids.sort_by_key(|&child| unsafe { *((child + TRIGGER_COST) as *const u64) });
}

/// Computes `[trigger + 0x58]` for every deduplicated trigger in `triggers`
/// and then stably sorts direct children, fanning out across `worker_count`
/// threads when the list is large enough.
unsafe fn process_triggers(index: &ProfileIndex, triggers: &[usize], worker_count: usize) {
    let sort_enabled = index.count != 0;
    let workers = if triggers.len() < PARALLEL_THRESHOLD { 1 } else { worker_count.clamp(1, 8) };
    if workers <= 1 {
        let mut memo = FastMap::default();
        let mut scratch = Vec::new();
        for &trigger in triggers {
            let cost = unsafe { index.lookup_cost(trigger, &mut memo, &mut scratch) };
            unsafe { *((trigger + TRIGGER_COST) as *mut u64) = cost };
        }
        if sort_enabled {
            for &trigger in triggers {
                unsafe { sort_trigger_children(trigger) };
            }
        }
        return;
    }

    let chunk_size = triggers.len().div_ceil(workers);
    let chunks: Vec<&[usize]> = triggers.chunks(chunk_size).collect();
    let barrier = Barrier::new(chunks.len());
    std::thread::scope(|scope| {
        for chunk in chunks {
            let barrier = &barrier;
            scope.spawn(move || {
                let mut memo = FastMap::default();
                let mut scratch = Vec::new();
                for &trigger in chunk {
                    let cost = unsafe { index.lookup_cost(trigger, &mut memo, &mut scratch) };
                    unsafe { *((trigger + TRIGGER_COST) as *mut u64) = cost };
                }
                barrier.wait();
                if sort_enabled {
                    for &trigger in chunk {
                        unsafe { sort_trigger_children(trigger) };
                    }
                }
            });
        }
    });
}

#[called_by_game]
extern "C" fn run_batch(registry: usize) -> u8 {
    crate::base::guarded("trigger profile batch", || {
        if registry == 0 || speed::checked(&SPEED_UP) {
            return 0;
        }
        let manager = unsafe { ORIGINAL_GET_MANAGER.get()() };
        let Some(index) = (unsafe { index_for(manager) }) else {
            return 0;
        };
        let list1 = unsafe { *((registry + REGISTRY_LIST1) as *const usize) };
        let list2 = unsafe { *((registry + REGISTRY_LIST2) as *const usize) };
        let mut triggers = Vec::new();
        if !unsafe { collect_list(list1, &mut triggers) } || !unsafe { collect_list(list2, &mut triggers) } {
            return 0;
        }
        // Deduplicating guarantees disjoint writes to `[trigger + 0x58]` and
        // `[trigger + 8]` across worker chunks while improving cache locality.
        triggers.sort_unstable();
        triggers.dedup();
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
        unsafe { process_triggers(&index, &triggers, workers) };
        1
    })
}

/// Replaces `0x205bed..0x205c94`: runs the parallel batch pass and jumps to
/// unlock (`0x205c94`), or replays the displaced head check when falling back
/// to the native loops (such as under `checked = ["trigger-profile"]`).
#[unsafe(naked)]
extern "C" fn batch_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rcx, r13", call run_batch, "mov rax, [rsp + 0x48]"),
        "test al, al",
        "jnz 2f",
        "mov rsi, [r13 + 8]",
        "mov rbx, [rsi]",
        "cmp rbx, rsi",
        "je 3f",
        "jmp qword ptr [rip + {list1_body}]",
        "3:",
        "jmp qword ptr [rip + {list2}]",
        "2:",
        "jmp qword ptr [rip + {done}]",
        run_batch = sym run_batch,
        list1_body = sym BATCH_LIST1_BODY_AT,
        list2 = sym BATCH_LIST2_AT,
        done = sym BATCH_DONE_AT,
    );
}

unsafe fn free_detached_registry(reg: usize) {
    unsafe {
        ORIGINAL_FREE_BUCKETS.get()(reg + REGISTRY_BUCKETS2);
        ORIGINAL_FREE_LIST.get()(reg + REGISTRY_LIST2);
        ORIGINAL_FREE_BUCKETS.get()(reg + REGISTRY_BUCKETS1);
        ORIGINAL_FREE_LIST.get()(reg + REGISTRY_LIST1);
    }
    let _ = crate::native::game_heap::delete(&crate::native::hooked_memory(), reg, REGISTRY_SIZE);
}

#[called_by_game]
unsafe extern "C" fn free_registered() {
    crate::base::guarded("registered triggers free", || {
        clear_thread_memo();
        if let Some(handle) = BG_FREE.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = handle.join();
        }
        let mem = crate::native::hooked_memory();
        let slot = mem.at(REGISTERED_TRIGGERS) as *mut usize;
        let reg = unsafe { *slot };
        if reg == 0 {
            return;
        }
        unsafe { *slot = 0 };
        match std::thread::Builder::new().name("harmony-trigger-free".into()).spawn(move || {
            unsafe { free_detached_registry(reg) };
        }) {
            Ok(handle) => {
                *BG_FREE.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
            }
            Err(_) => unsafe { free_detached_registry(reg) },
        }
    });
}

#[called_by_game]
unsafe extern "C" fn lookup(manager: usize, trigger: usize) -> u64 {
    crate::base::guarded("trigger profile lookup", || {
        let Some(index) = (unsafe { index_for(manager) }) else {
            return unsafe { ORIGINAL_LOOKUP.get()(manager, trigger) };
        };
        let ours = MEMO.with(|m| {
            let m = &mut *m.borrow_mut();
            if m.manager != index.manager || m.head != index.head || m.count != index.count {
                m.manager = index.manager;
                m.head = index.head;
                m.count = index.count;
                m.memo.clear();
            }
            m.scratch.clear();
            unsafe { index.lookup_cost(trigger, &mut m.memo, &mut m.scratch) }
        });
        if speed::checked(&SPEED_UP) {
            let game = unsafe { ORIGINAL_LOOKUP.get()(manager, trigger) };
            speed::compared(&SPEED_UP, ours == game, || format!("trigger {trigger:#x}: index answered {ours}, game {game}"));
            return game;
        }
        ours
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct FakeTrigger {
        _vtable: usize,
        children: usize,
        _cap: u32,
        count: i32,
        _rest: [u8; 0x38],
        ty: u32,
        _pad: u32,
        cost: u64,
    }

    fn make_trigger(ty: u32, children: &mut [usize]) -> FakeTrigger {
        FakeTrigger {
            _vtable: 0,
            children: if children.is_empty() { 0 } else { children.as_mut_ptr() as usize },
            _cap: children.len() as u32,
            count: children.len() as i32,
            _rest: [0; 0x38],
            ty,
            _pad: 0,
            cost: 0,
        }
    }

    #[test]
    fn canonical_profile_matches_unordered_deduped_children_and_rejects_unknowns() {
        assert_eq!(std::mem::offset_of!(FakeTrigger, children), TRIGGER_CHILDREN);
        assert_eq!(std::mem::offset_of!(FakeTrigger, count), TRIGGER_CHILD_COUNT);
        assert_eq!(std::mem::offset_of!(FakeTrigger, ty), TRIGGER_TYPE);
        assert_eq!(std::mem::offset_of!(FakeTrigger, cost), TRIGGER_COST);

        let mut index = ProfileIndex::new(1, 2, 3);
        let leaf_a = index.intern(10302, Vec::new());
        let leaf_b = index.intern(11808, Vec::new());
        let root_ab = index.intern(u32::MAX, vec![leaf_b, leaf_a, leaf_b]);
        index.costs[leaf_a as usize] = 126;
        index.costs[root_ab as usize] = 6040;
        // `leaf_b` is only a child in the profile, never a standalone root: its root cost stays 0.

        let t_a1 = make_trigger(10302, &mut []);
        let t_a2 = make_trigger(10302, &mut []);
        let t_b = make_trigger(11808, &mut []);
        let t_unknown = make_trigger(99999, &mut []);

        // Reversed order + duplicate child `10302` collapses to the same set `{10302, 11808}`.
        let mut kids_match = [&raw const t_a1 as usize, &raw const t_b as usize, &raw const t_a2 as usize];
        let t_root = make_trigger(u32::MAX, &mut kids_match);

        // Extra unknown child rejects the compound trigger.
        let mut kids_miss = [&raw const t_a1 as usize, &raw const t_unknown as usize];
        let t_miss = make_trigger(u32::MAX, &mut kids_miss);

        let mut memo = FastMap::default();
        let mut scratch = Vec::new();
        unsafe {
            assert_eq!(index.lookup_cost(&raw const t_a1 as usize, &mut memo, &mut scratch), 126);
            assert_eq!(index.lookup_cost(&raw const t_b as usize, &mut memo, &mut scratch), 0);
            assert_eq!(index.lookup_cost(&raw const t_root as usize, &mut memo, &mut scratch), 6040);
            assert_eq!(index.lookup_cost(&raw const t_miss as usize, &mut memo, &mut scratch), 0);
            assert_eq!(index.lookup_cost(&raw const t_unknown as usize, &mut memo, &mut scratch), 0);
        }
    }

    #[test]
    fn batch_pass_populates_costs_and_stably_sorts_only_when_all_children_have_cost() {
        let mut index = ProfileIndex::new(1, 2, 4);
        let id_slow = index.intern(10, Vec::new());
        let id_fast = index.intern(20, Vec::new());
        let id_tie = index.intern(30, Vec::new());
        let id_zero = index.intern(40, Vec::new());
        let id_parent = index.intern(99, vec![id_slow, id_fast, id_tie]);
        index.costs[id_slow as usize] = 500;
        index.costs[id_fast as usize] = 100;
        index.costs[id_tie as usize] = 100;
        index.costs[id_zero as usize] = 0;
        index.costs[id_parent as usize] = 700;

        let t_slow = make_trigger(10, &mut []);
        let t_tie_first = make_trigger(30, &mut []);
        let t_fast_second = make_trigger(20, &mut []);
        let t_zero = make_trigger(40, &mut []);

        let mut sortable_kids = [&raw const t_slow as usize, &raw const t_tie_first as usize, &raw const t_fast_second as usize];
        let t_sortable = make_trigger(99, &mut sortable_kids);

        let mut unsortable_kids = [&raw const t_slow as usize, &raw const t_zero as usize, &raw const t_fast_second as usize];
        let t_unsortable = make_trigger(99, &mut unsortable_kids);

        let triggers = [
            &raw const t_slow as usize,
            &raw const t_tie_first as usize,
            &raw const t_fast_second as usize,
            &raw const t_zero as usize,
            &raw const t_sortable as usize,
            &raw const t_unsortable as usize,
        ];
        unsafe { process_triggers(&index, &triggers, 4) };

        assert_eq!(t_slow.cost, 500);
        assert_eq!(t_tie_first.cost, 100);
        assert_eq!(t_fast_second.cost, 100);
        assert_eq!(t_sortable.cost, 700);
        // Stable ascending sort preserves tie order (`t_tie_first` before `t_fast_second`).
        assert_eq!(sortable_kids, [&raw const t_tie_first as usize, &raw const t_fast_second as usize, &raw const t_slow as usize]);
        // Presence of a zero-cost child leaves the original order untouched.
        assert_eq!(unsortable_kids, [&raw const t_slow as usize, &raw const t_zero as usize, &raw const t_fast_second as usize]);
    }
}
