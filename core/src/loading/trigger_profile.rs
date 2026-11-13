//! Trigger optimization profile lookup without per-node hash-table allocations
//! (docs/research/localisation-loading.md, docs/plans/large-mod-startup.md, M2).
//!
//! At the start of `0x208620`, `0xb9b5a0` loads `trigger_profile.txt` (~6,593
//! trigger trees with historical costs) into `[0x242be60] + 0x48`. At the end
//! of `InitGame` (`0x205bed..0x205c51`), `0xb9b4e0(manager, trigger) -> u64`
//! runs for every compound trigger in `[0x2340010]`: it recursively builds a
//! temporary `CTriggerProfileNode` tree (`0xb9c160`), allocating a `0x48`-byte
//! `std::unordered_set` on the heap for every node in the trigger's subtree,
//! recomputing subtree sums (`0xb9c9a0`) at every level, looking the root up
//! in `manager + 0x48` (`0xbbd0b0`), and immediately deleting the temporary
//! tree (`0xbbdfe0`).
//!
//! A `CTriggerProfileNode` has only a 32-bit trigger type (`+0`) and an
//! unordered set of distinct child `CTriggerProfileNode`s (`+8`), so every
//! profile tree has a unique canonical structural ID when interned bottom-up
//! by `(type_id, sorted_deduped_child_ids)`. After `0xb9b5a0` loads the
//! profile table (or on the first call to `0xb9b4e0` for that `(manager,
//! count)`), the framework interns the ~6,593 profile trees into a read-only
//! table (`ProfileIndex`):
//!
//! - Any trigger whose `type_id` (`[trigger + 0x50]`) does not appear anywhere
//!   in the loaded profile table cannot match any profile node (or any child
//!   of one), so a recursive walk returns `None` as soon as it meets an
//!   unknown `type_id` or a `(type_id, children)` tuple absent from the
//!   interner, with zero heap allocations.
//! - Sub-triggers visited during `InitGame`'s pass (where `0xb9b4e0` visits
//!   every top-level trigger and then every nested compound trigger) memoize
//!   their canonical ID by trigger pointer for the duration of that manager
//!   and count, cleared when `0xb89560` frees `[0x2340010]`.
//!
//! Checked (`checked = ["trigger-profile"]`), `0xb9b4e0` computes the game's
//! own cost as well, compares the two for every trigger, and returns the
//! game's.
use crate::called_by_game;
use crate::hooks::Call;
use crate::points::{Function, Point};
use crate::speed::{self, SpeedUp};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// `0xb9b4e0(manager, trigger) -> u64`: looks `trigger` up in `manager + 0x48`.
const LOOKUP: usize = 0xb9b4e0;
/// `0xb9b5a0(manager)`: loads `trigger_profile.txt` into `manager + 0x48`.
const LOAD_PROFILE: usize = 0xb9b5a0;
/// `0xb89560()`: frees `[0x2340010]` after `InitGame`'s trigger pass.
const FREE_REGISTERED: usize = 0xb89560;
/// `0xb9dae0(_, manager)`: destroys a profile manager.
const DESTROY_MANAGER: usize = 0xb9dae0;

/// Offsets in a trigger (`0xb9c160`).
const TRIGGER_CHILDREN: usize = 0x08;
const TRIGGER_CHILD_COUNT: usize = 0x14;
const TRIGGER_TYPE: usize = 0x50;

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
static ORIGINAL_LOOKUP: Function<Lookup> = Function::new();

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            LOOKUP,
            "48 89 5c 24 08 48 89 74 24 10 57 48 81 ec 80 00 00 00 48 8b f1",
            "trigger optimization profile lookup",
            lookup as Lookup,
            &ORIGINAL_LOOKUP,
        ),
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
        Point::observe(FREE_REGISTERED, "40 53 48 83 ec 20 48 8b 1d a3 6a 7b 01 48 85 db", "registered triggers freed", 0, Some(clear_memo), None),
        Point::observe(DESTROY_MANAGER, "48 85 d2 0f 84 88 00 00 00 53 48 83 ec 20 48 8d", "trigger profile manager destroyed", 6, Some(invalidate_all), None),
    ]
}

/// Canonical interned representation of a loaded `trigger_profile.txt` table.
struct ProfileIndex {
    manager: usize,
    head: usize,
    count: usize,
    /// Sorted list of every `u32` trigger type ID that appears anywhere in the
    /// profile (roots or descendants), plus a 4096-bit bloom filter for $O(1)$
    /// rejection of unknown leaf/compound types.
    type_bits: [u64; 64],
    leaf_ids: HashMap<u32, u32>,
    compound_ids: HashMap<(u32, Box<[u32]>), u32>,
    /// Cost for each root canonical ID (`0` when only a proper subtree).
    costs: Vec<u64>,
}

impl ProfileIndex {
    fn new(manager: usize, head: usize, count: usize) -> Self {
        Self { manager, head, count, type_bits: [0; 64], leaf_ids: HashMap::new(), compound_ids: HashMap::new(), costs: Vec::new() }
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
        let key = (ty, children.into_boxed_slice());
        if let Some(&id) = self.compound_ids.get(&key) {
            return id;
        }
        let id = self.costs.len() as u32;
        self.costs.push(0);
        self.compound_ids.insert(key, id);
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
    unsafe fn match_trigger(&self, trigger: usize, memo: &mut HashMap<usize, u32>, scratch: &mut Vec<u32>, depth: usize) -> Option<u32> {
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
        let found = self.compound_ids.get(&(ty, scratch[base..].into())).copied();
        scratch.truncate(base);
        memo.insert(trigger, found.unwrap_or(u32::MAX));
        found
    }

    unsafe fn lookup_cost(&self, trigger: usize, memo: &mut HashMap<usize, u32>, scratch: &mut Vec<u32>) -> u64 {
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

struct ThreadMemo {
    manager: usize,
    head: usize,
    count: usize,
    memo: HashMap<usize, u32>,
    scratch: Vec<u32>,
}

thread_local! {
    static MEMO: RefCell<ThreadMemo> = RefCell::new(ThreadMemo {
        manager: 0,
        head: 0,
        count: 0,
        memo: HashMap::new(),
        scratch: Vec::new(),
    });
}

fn invalidate_all(_: &mut Call) {
    *BUILT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    clear_thread_memo();
}

fn clear_memo(_: &mut Call) {
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

#[called_by_game]
unsafe extern "C" fn lookup(manager: usize, trigger: usize) -> u64 {
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
    }

    fn make_trigger(ty: u32, children: &[*const FakeTrigger]) -> FakeTrigger {
        FakeTrigger {
            _vtable: 0,
            children: if children.is_empty() { 0 } else { children.as_ptr() as usize },
            _cap: children.len() as u32,
            count: children.len() as i32,
            _rest: [0; 0x38],
            ty,
        }
    }

    #[test]
    fn canonical_profile_matches_unordered_deduped_children_and_rejects_unknowns() {
        assert_eq!(std::mem::offset_of!(FakeTrigger, children), TRIGGER_CHILDREN);
        assert_eq!(std::mem::offset_of!(FakeTrigger, count), TRIGGER_CHILD_COUNT);
        assert_eq!(std::mem::offset_of!(FakeTrigger, ty), TRIGGER_TYPE);

        let mut index = ProfileIndex::new(1, 2, 3);
        let leaf_a = index.intern(10302, Vec::new());
        let leaf_b = index.intern(11808, Vec::new());
        let root_ab = index.intern(u32::MAX, vec![leaf_b, leaf_a, leaf_b]);
        index.costs[leaf_a as usize] = 126;
        index.costs[root_ab as usize] = 6040;
        // `leaf_b` is only a child in the profile, never a standalone root: its root cost stays 0.

        let t_a1 = make_trigger(10302, &[]);
        let t_a2 = make_trigger(10302, &[]);
        let t_b = make_trigger(11808, &[]);
        let t_unknown = make_trigger(99999, &[]);

        // Reversed order + duplicate child `10302` collapses to the same set `{10302, 11808}`.
        let kids_match = [&raw const t_a1, &raw const t_b, &raw const t_a2];
        let t_root = make_trigger(u32::MAX, &kids_match);

        // Extra unknown child rejects the compound trigger.
        let kids_miss = [&raw const t_a1, &raw const t_unknown];
        let t_miss = make_trigger(u32::MAX, &kids_miss);

        let mut memo = HashMap::new();
        let mut scratch = Vec::new();
        unsafe {
            assert_eq!(index.lookup_cost(&raw const t_a1 as usize, &mut memo, &mut scratch), 126);
            assert_eq!(index.lookup_cost(&raw const t_b as usize, &mut memo, &mut scratch), 0);
            assert_eq!(index.lookup_cost(&raw const t_root as usize, &mut memo, &mut scratch), 6040);
            assert_eq!(index.lookup_cost(&raw const t_miss as usize, &mut memo, &mut scratch), 0);
            assert_eq!(index.lookup_cost(&raw const t_unknown as usize, &mut memo, &mut scratch), 0);
        }
    }
}
