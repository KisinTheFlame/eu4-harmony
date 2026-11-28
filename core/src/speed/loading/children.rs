//! A dated history node's child list (`0x7e7cd0`) maintained in O(1) duplicate
//! checks and O(log K) date-ordered insertion (docs/game/program/loading.md).
//!
//! 105 history classes share `0x7e7cd0(parent, child)` as their virtual `+0x98`
//! add method (and province history `0xa368c0` calls it directly). Every call
//! first walks the entire doubly-linked list at `parent+0x20` to ignore an
//! already-present pointer, then walks backward from the tail (`parent+0x28`)
//! to find the insertion point ordered by `(child_count != 0, date)`. Reading a
//! late-game save inserts tens of thousands of entries into country, province,
//! war and diplomacy histories (including out-of-order start/end dates in
//! diplomacy history), turning both walks into O(N^2) work.
//!
//! For short lists (< 32 nodes) the game's original code runs directly. Once a
//! list reaches 32 nodes, this module keeps a thread-local auxiliary index of
//! its member pointers and the last node for each `(has_children, date)` key.
//! Every call validates `(head, tail, count, *head, *tail)` and verifies the
//! chosen predecessor's local link order before inserting; any external list
//! mutation, deletion or non-monotonic ordering invalidates the index and falls
//! back to the game's original code.
use crate::base::called_by_game;
use crate::native::game_heap;
use crate::native::hooks::Call;
use crate::native::points::{Function, Point};
use crate::native::speed_up::SpeedUp;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
struct PtrHasher(u64);

impl Hasher for PtrHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.0 = (self.0.rotate_left(5) ^ i as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type FastHash = BuildHasherDefault<PtrHasher>;
type FastMap<K, V> = HashMap<K, V, FastHash>;
type FastSet<K> = HashSet<K, FastHash>;

/// `(parent, child)`: add a dated node to `parent`'s sorted child list.
const ADD: usize = 0x7e7cd0;
/// `(parent, child) -> bool`: remove a child node from `parent`'s list.
const REMOVE: usize = 0x7e7e70;
/// `(parent)`: recursively prune child nodes from `parent`'s list.
const PRUNE: usize = 0x7e92a0;
/// `(node, &date)`: propagate a date through a node's undated subtree.
const SET_DATE: usize = 0x7e7f80;
/// Sentinel date (year 5000.1.1) written to undated leaf nodes when their
/// parent's virtual `+0x90` returns true.
const UNDATED_SENTINEL: i32 = 0x029c_77f8;
/// Token id excluded from sentinel date propagation (`0x7e7d47`).
const SKIP_TOKEN_ID: i32 = 0x28d9;
/// Node size allocated on the game's heap (`[item, prev, next, removed_flag]`).
const NODE_SIZE: usize = 0x20;
/// Lists shorter than this stay on the game's original code with zero indexing
/// overhead.
const INDEX_THRESHOLD: u32 = 32;
/// Cap on simultaneously cached parent lists per thread to keep memory bounded
/// across world resets.
const MAX_CACHED_LISTS: usize = 512;

type AddChild = unsafe extern "C" fn(usize, usize);
type SetDate = unsafe extern "C" fn(usize, *mut i32);
type VirtBool = unsafe extern "C" fn(usize) -> u8;
type VirtI32 = unsafe extern "C" fn(usize) -> i32;

static ORIGINAL_ADD: Function<AddChild> = Function::new();
static SET_DATE_FN: Function<SetDate> = Function::new();

pub const SPEED_UP: SpeedUp = SpeedUp::new("children", "loading", points, false);

fn points() -> Vec<Point> {
    vec![
        Point::replace(ADD, "48 89 74 24 20 57 48 83 ec 20 48 8b 41 20 48 8b fa 48 8b f1", "dated node's add", add_child as AddChild, &ORIGINAL_ADD),
        Point::observe(REMOVE, "48 89 5c 24 08 57 48 83 ec 20 48 8b 59 20 48 8b", "dated node's remove", 2, Some(invalidate_parent), None),
        Point::observe(PRUNE, "48 89 74 24 18 57 48 83 ec 20 48 8b 79 20 48 8b", "dated node's prune", 1, Some(invalidate_parent), None),
        Point::keep(SET_DATE, "48 89 74 24 10 57 48 83 ec 20 48 8b 01 48 8b fa 48 8b f1", "dated node's date propagation").recorded(SET_DATE_FN.slot()),
        Point::keep(0x7e7d47usize, "3d d9 28 00 00 74 15 48 8d 54 24 30 c7 44 24 30 f8 77 9c 02", "dated node's sentinel date check")
            .uses(&[SKIP_TOKEN_ID as usize, UNDATED_SENTINEL as usize]),
        Point::keep(0x7e7d80usize, "48 8b 0b 83 79 30 00 0f 94 c0 3a d0 74 05 0f b6 c2 eb 09 8b 47 10 3b 41 10 0f 9c c0", "dated node's order key"),
    ]
}

fn evict_parent(parent: usize) {
    INDEXES.with(|cell| {
        if let Ok(mut map) = cell.try_borrow_mut() {
            map.remove(&parent);
        }
    });
}

fn invalidate_parent(call: &mut Call) {
    evict_parent(call.arg(0));
}

/// Sort key matching `0x7e7d80`: leaf nodes (`child_count == 0`) sort before
/// non-leaf nodes (`child_count != 0`), then by signed date ascending.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OrderKey {
    has_children: bool,
    date: i32,
}

#[derive(Default)]
struct ListIndex {
    head: usize,
    tail: usize,
    count: u32,
    head_item: usize,
    tail_item: usize,
    seen: FastSet<usize>,
    /// Maps each distinct `OrderKey` currently in the list to the *last*
    /// `Node*` carrying that key.
    last_by_key: BTreeMap<OrderKey, usize>,
}

thread_local! {
    static INDEXES: RefCell<FastMap<usize, ListIndex>> = RefCell::new(FastMap::default());
}

#[inline]
unsafe fn read_usize(addr: usize) -> usize {
    (addr as *const usize).read()
}

#[inline]
unsafe fn write_usize(addr: usize, value: usize) {
    (addr as *mut usize).write(value);
}

#[inline]
unsafe fn read_i32(addr: usize) -> i32 {
    (addr as *const i32).read()
}

#[inline]
unsafe fn write_u32(addr: usize, value: u32) {
    (addr as *mut u32).write(value);
}

#[inline]
unsafe fn order_key(item: usize) -> OrderKey {
    OrderKey { has_children: read_i32(item + 0x30) != 0, date: read_i32(item + 0x10) }
}

/// Reads the list header `(head, tail, count)` from `parent`.
#[inline]
unsafe fn list_header(parent: usize) -> (usize, usize, u32) {
    (read_usize(parent + 0x20), read_usize(parent + 0x28), read_i32(parent + 0x30) as u32)
}

/// Scans an existing list once to build a `ListIndex`. Returns `None` if the
/// list is malformed, contains cycles, or is not monotonically ordered.
unsafe fn build_index(parent: usize) -> Option<ListIndex> {
    let (head, tail, count) = list_header(parent);
    if head == 0 || tail == 0 || count < INDEX_THRESHOLD {
        return None;
    }
    let mut seen = FastSet::with_capacity_and_hasher(count as usize + 16, FastHash::default());
    let mut last_by_key = BTreeMap::new();
    let mut node = head;
    let mut visited = 0u32;
    let mut prev_node = 0usize;
    let mut prev_key: Option<OrderKey> = None;

    while node != 0 {
        if visited >= count {
            return None;
        }
        let item = read_usize(node);
        if item == 0 {
            return None;
        }
        let key = order_key(item);
        if let Some(pk) = prev_key {
            if key < pk {
                return None;
            }
        }
        seen.insert(item);
        last_by_key.insert(key, node);
        prev_key = Some(key);
        prev_node = node;
        node = read_usize(node + 0x10);
        visited += 1;
    }

    if visited != count || prev_node != tail {
        return None;
    }
    Some(ListIndex { head, tail, count, head_item: read_usize(head), tail_item: read_usize(tail), seen, last_by_key })
}

#[inline]
unsafe fn index_matches(idx: &ListIndex, parent: usize) -> bool {
    let (head, tail, count) = list_header(parent);
    head != 0
        && tail != 0
        && idx.head == head
        && idx.tail == tail
        && idx.count == count
        && idx.head_item == read_usize(head)
        && idx.tail_item == read_usize(tail)
}

/// Runs the native date propagation block (`0x7e7d0e..0x7e7d63`).
#[inline]
unsafe fn maybe_propagate_sentinel_date(parent: usize, child: usize) {
    let parent_vtable = read_usize(parent);
    let is_root: VirtBool = std::mem::transmute(read_usize(parent_vtable + 0x90));
    if is_root(parent) == 0 || read_i32(child + 0x30) != 0 {
        return;
    }
    let child_vtable = read_usize(child);
    let has_date: VirtBool = std::mem::transmute(read_usize(child_vtable + 0x60));
    if has_date(child) != 0 {
        return;
    }
    let get_token: VirtI32 = std::mem::transmute(read_usize(child_vtable + 0x80));
    if get_token(child) == SKIP_TOKEN_ID {
        return;
    }
    let mut date = UNDATED_SENTINEL;
    SET_DATE_FN.get()(child, &mut date);
}

/// Finds the insertion predecessor for `new_key` using `idx.last_by_key` and
/// verifies the local link invariant directly in the native list.
/// Returns `Some(prev_node)` (`0` for head insertion) if valid, or `None` if
/// the list's ordering changed externally and we must fall back.
unsafe fn find_verified_predecessor(idx: &ListIndex, new_key: OrderKey) -> Option<usize> {
    if let Some((&prev_key, &prev_node)) = idx.last_by_key.range(..=new_key).next_back() {
        let prev_item = read_usize(prev_node);
        if prev_item == 0 || order_key(prev_item) != prev_key {
            return None;
        }
        let next_node = read_usize(prev_node + 0x10);
        if next_node == 0 {
            if prev_node != idx.tail {
                return None;
            }
        } else {
            let next_item = read_usize(next_node);
            if next_item == 0 || order_key(next_item) <= new_key {
                return None;
            }
        }
        Some(prev_node)
    } else {
        let head_item = read_usize(idx.head);
        if head_item == 0 || order_key(head_item) <= new_key {
            return None;
        }
        Some(0)
    }
}

/// Links `new_node` (already allocated with size `NODE_SIZE`) into `parent`
/// right after `prev_node` (`0` means before `head`) and updates `idx`.
unsafe fn link_node(parent: usize, child: usize, new_key: OrderKey, prev_node: usize, new_node: usize, idx: &mut ListIndex) {
    if prev_node == 0 {
        let old_head = idx.head;
        write_usize(new_node, child);
        write_usize(new_node + 0x08, 0);
        write_usize(new_node + 0x10, old_head);
        *(new_node as *mut u8).add(0x18) = 0;
        write_usize(old_head + 0x08, new_node);
        write_usize(parent + 0x20, new_node);
        idx.head = new_node;
        idx.head_item = child;
    } else {
        let next_node = read_usize(prev_node + 0x10);
        write_usize(new_node, child);
        write_usize(new_node + 0x08, prev_node);
        write_usize(new_node + 0x10, next_node);
        *(new_node as *mut u8).add(0x18) = 0;
        write_usize(prev_node + 0x10, new_node);
        if next_node != 0 {
            write_usize(next_node + 0x08, new_node);
        } else {
            write_usize(parent + 0x28, new_node);
            idx.tail = new_node;
            idx.tail_item = child;
        }
    }
    idx.count += 1;
    write_u32(parent + 0x30, idx.count);
    idx.seen.insert(child);
    idx.last_by_key.insert(new_key, new_node);
}

#[called_by_game]
unsafe extern "C" fn add_child(parent: usize, child: usize) {
    if parent == 0 || child == 0 {
        ORIGINAL_ADD.get()(parent, child);
        return;
    }
    let count = read_i32(parent + 0x30) as u32;
    if count < INDEX_THRESHOLD {
        // Evict any stale index for `parent` (e.g. a cached list that shrank
        // below the threshold or a recreated list reusing the same address)
        // before native `0x7e7cd0` mutates it back up to the threshold.
        evict_parent(parent);
        ORIGINAL_ADD.get()(parent, child);
        return;
    }

    // Step 1: ensure an up-to-date index and check membership without holding
    // the RefCell borrow across any native virtual calls.
    let status = INDEXES.with(|cell| {
        let Ok(mut map) = cell.try_borrow_mut() else {
            return Err(());
        };
        if let Some(existing) = map.get(&parent) {
            if index_matches(existing, parent) {
                return Ok(existing.seen.contains(&child));
            }
        }
        if map.len() >= MAX_CACHED_LISTS {
            map.clear();
        }
        match build_index(parent) {
            Some(built) => {
                let duplicate = built.seen.contains(&child);
                map.insert(parent, built);
                Ok(duplicate)
            }
            None => {
                map.remove(&parent);
                Err(())
            }
        }
    });

    match status {
        // Already present (or index unavailable/reentrant): defer to native so
        // real duplicates return immediately and any external anomaly is safe.
        Ok(true) | Err(()) => {
            ORIGINAL_ADD.get()(parent, child);
            return;
        }
        Ok(false) => {}
    }

    // Step 2: run the exact native sentinel-date propagation before computing
    // the sort key.
    maybe_propagate_sentinel_date(parent, child);
    let new_key = order_key(child);

    // Step 3: locate and verify the insertion predecessor in O(log K).
    let predecessor = INDEXES.with(|cell| {
        let Ok(mut map) = cell.try_borrow_mut() else {
            return None;
        };
        let valid_pred = map.get(&parent).and_then(|idx| {
            if !index_matches(idx, parent) {
                return None;
            }
            find_verified_predecessor(idx, new_key)
        });
        if valid_pred.is_none() {
            map.remove(&parent);
        }
        valid_pred
    });

    let Some(prev_node) = predecessor else {
        ORIGINAL_ADD.get()(parent, child);
        return;
    };

    // Step 4: allocate the 32-byte list node from the game's heap (outside the
    // RefCell borrow).
    let m = crate::native::hooked_memory();
    let Ok(new_node) = game_heap::new(&m, NODE_SIZE) else {
        INDEXES.with(|cell| {
            if let Ok(mut map) = cell.try_borrow_mut() {
                map.remove(&parent);
            }
        });
        ORIGINAL_ADD.get()(parent, child);
        return;
    };
    if new_node == 0 {
        INDEXES.with(|cell| {
            if let Ok(mut map) = cell.try_borrow_mut() {
                map.remove(&parent);
            }
        });
        ORIGINAL_ADD.get()(parent, child);
        return;
    }

    // Step 5: splice the node in O(1) and update the index.
    let linked = INDEXES.with(|cell| {
        let Ok(mut map) = cell.try_borrow_mut() else {
            return false;
        };
        let Some(idx) = map.get_mut(&parent) else {
            return false;
        };
        if !index_matches(idx, parent) {
            map.remove(&parent);
            return false;
        }
        link_node(parent, child, new_key, prev_node, new_node, idx);
        true
    });

    if !linked {
        let _ = game_heap::delete(&m, new_node, NODE_SIZE);
        ORIGINAL_ADD.get()(parent, child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_game::buffers::Object;

    fn make_item(date: i32, child_count: i32) -> Object {
        Object::new(0x40).u32(0x10, date as u32).u32(0x30, child_count as u32)
    }

    /// Reference implementation of `0x7e7cd0`'s dedup + backward insertion on
    /// a Rust vector of `(item_addr, OrderKey)`.
    fn reference_insert(list: &mut Vec<(usize, OrderKey)>, item: usize, key: OrderKey) {
        if list.iter().any(|&(existing, _)| existing == item) {
            return;
        }
        let mut pos = list.len();
        while pos > 0 && key < list[pos - 1].1 {
            pos -= 1;
        }
        list.insert(pos, (item, key));
    }

    #[test]
    fn indexed_insertion_matches_backward_walk_for_out_of_order_and_leaf_nodes() {
        let parent = Object::new(0x40);
        let mut items = Vec::new();
        let mut nodes = Vec::new();
        let mut expected = Vec::new();

        // Seed 40 sorted items so the list crosses INDEX_THRESHOLD.
        let mut prev_node = 0usize;
        for i in 0..40 {
            let has_children = i >= 10;
            let date = if has_children { 1444 + i * 2 } else { 1444 + i };
            let item = make_item(date, if has_children { 1 } else { 0 });
            let node = Object::new(NODE_SIZE);
            let node_addr = node.at();
            unsafe {
                write_usize(node_addr, item.at());
                write_usize(node_addr + 0x08, prev_node);
                write_usize(node_addr + 0x10, 0);
                if prev_node != 0 {
                    write_usize(prev_node + 0x10, node_addr);
                } else {
                    write_usize(parent.at() + 0x20, node_addr);
                }
                write_usize(parent.at() + 0x28, node_addr);
                write_u32(parent.at() + 0x30, (i + 1) as u32);
            }
            expected.push((item.at(), OrderKey { has_children, date }));
            prev_node = node_addr;
            items.push(item);
            nodes.push(node);
        }

        let mut idx = unsafe { build_index(parent.at()).expect("sorted list should index") };

        // Insert out-of-order dates, duplicate keys, head/middle/tail leaves and non-leaves.
        let cases = [
            (1400, 0), // before head (leaf)
            (1449, 0), // middle of leaves, duplicate date
            (1500, 0), // end of leaves, before non-leaves
            (1450, 2), // start of non-leaves
            (1480, 1), // middle of non-leaves, duplicate date
            (1480, 3), // second duplicate date at 1480
            (2000, 1), // after tail (non-leaf)
        ];

        for (date, child_count) in cases {
            let item = make_item(date, child_count);
            let key = OrderKey { has_children: child_count != 0, date };
            let pred = unsafe { find_verified_predecessor(&idx, key).expect("predecessor must verify") };
            let new_node = Object::new(NODE_SIZE);
            unsafe {
                link_node(parent.at(), item.at(), key, pred, new_node.at(), &mut idx);
            }
            reference_insert(&mut expected, item.at(), key);
            items.push(item);
            nodes.push(new_node);
        }

        // Walk forward and backward to verify exact order and double-link integrity.
        let mut forward = Vec::new();
        let mut cur = unsafe { read_usize(parent.at() + 0x20) };
        let mut prev = 0usize;
        while cur != 0 {
            let item = unsafe { read_usize(cur) };
            assert_eq!(unsafe { read_usize(cur + 0x08) }, prev);
            forward.push((item, unsafe { order_key(item) }));
            prev = cur;
            cur = unsafe { read_usize(cur + 0x10) };
        }
        assert_eq!(prev, unsafe { read_usize(parent.at() + 0x28) });
        assert_eq!(forward.len() as u32, unsafe { read_i32(parent.at() + 0x30) as u32 });
        assert_eq!(forward, expected);
    }

    #[test]
    fn external_order_mutation_invalidates_predecessor_check() {
        let parent = Object::new(0x40);
        let mut items = Vec::new();
        let mut nodes = Vec::new();
        let mut prev_node = 0usize;
        for i in 0..INDEX_THRESHOLD {
            let item = make_item(1444 + i as i32, 1);
            let node = Object::new(NODE_SIZE);
            unsafe {
                write_usize(node.at(), item.at());
                write_usize(node.at() + 0x08, prev_node);
                write_usize(node.at() + 0x10, 0);
                if prev_node != 0 {
                    write_usize(prev_node + 0x10, node.at());
                } else {
                    write_usize(parent.at() + 0x20, node.at());
                }
                write_usize(parent.at() + 0x28, node.at());
                write_u32(parent.at() + 0x30, i + 1);
            }
            prev_node = node.at();
            items.push(item);
            nodes.push(node);
        }
        let idx = unsafe { build_index(parent.at()).unwrap() };
        // Corrupt the date of node 10 externally.
        items[10].set(0x10, &9999i32.to_le_bytes());
        let pred = unsafe { find_verified_predecessor(&idx, OrderKey { has_children: true, date: 1454 }) };
        assert!(pred.is_none());
    }
}
