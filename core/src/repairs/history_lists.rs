//! What a country's first history lists alone, which its clear drops
//! (docs/game/interface/main-menu.md, "A country's two histories").
//!
//! A country keeps two histories of one class, the first at `+0x11a8` and
//! the second at `+0x11e8`, each a list of what it holds: records, and the
//! blocks of records under a date. Clearing a country (and the two other
//! places that clear its histories) empties the first without deleting what
//! it lists, then deletes what the second lists and empties it. After a
//! load from a save the two list the same; after a new game the first lists
//! the blocks and records the history files gave, which the second does not,
//! and nothing else holds them: each new game left them, about 2,100 blocks
//! of records under a date and the records in them.
//!
//! So as the first is emptied there, what it lists that the second does not
//! hold, at any depth, is deleted first, unless something it holds is the
//! second's too, or one of the first's others holds it.
use super::LettingGo;
use crate::{native::hooks::Call, native::memory::Memory, native::points::Point};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `(history)`: empties a history's list, letting its nodes go and not what
/// they hold.
const EMPTY_LIST: usize = 0x7e7f20;
/// Where the three clears empty the first history; each then deletes what
/// the second lists and empties it.
const FIRST_EMPTIED: [usize; 3] = [0x2e37de, 0x2cc06b, 0x463795];
/// The second history, from the first.
const SECOND: usize = 0x40;
/// A history's list, a [`crate::native::objects::list`] of objects, a node taken
/// out marked removed, its object deleted.
const LIST: usize = 0x20;
/// At most this many objects are followed under one history, against a
/// list that loops.
const MOST: usize = 1 << 20;

/// What the repair deleted, and what it left as the second's too, for the
/// log.
static DELETED: AtomicUsize = AtomicUsize::new(0);
static SHARED: AtomicUsize = AtomicUsize::new(0);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset), ..super::Repair::NONE };

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!(
        "{} records and blocks of records the countries' first histories listed alone were deleted, {} kept as their second histories held some of what they held",
        DELETED.swap(0, Ordering::Relaxed),
        SHARED.swap(0, Ordering::Relaxed)
    )
}

/// What a history lists, the nodes taken out left out.
fn listed(m: &Memory, history: usize) -> crate::base::Result<Vec<usize>> {
    crate::native::containers::items(m, history + LIST, MOST)
}

/// Everything under `objects`, at any depth, not counting them.
fn under(m: &Memory, objects: &[usize], found: &mut HashSet<usize>) -> crate::base::Result<()> {
    let mut next: Vec<usize> = Vec::new();
    for &object in objects {
        next.extend(listed(m, object)?);
    }
    while let Some(object) = next.pop() {
        if found.len() >= MOST {
            return Err("a history's list does not end".into());
        }
        if found.insert(object) {
            next.extend(listed(m, object)?);
        }
    }
    Ok(())
}

/// The first history is being emptied by a clear: what it lists alone goes
/// first.
fn emptying(call: &mut Call) {
    let m = call.memory();
    if !FIRST_EMPTIED.iter().any(|&at| call.caller() == m.at(at)) {
        return;
    }
    let first = call.arg(0);
    if let Err(e) = delete_alone(&m, first) {
        log!("repair: what a country's first history lists alone was not deleted: {e}");
    }
}

fn delete_alone(m: &Memory, first: usize) -> crate::base::Result<()> {
    let listed_first = listed(m, first)?;
    if listed_first.is_empty() {
        return Ok(());
    }
    let second = first + SECOND;
    let mut seconds: HashSet<usize> = listed(m, second)?.into_iter().collect();
    let roots: Vec<usize> = seconds.iter().copied().collect();
    under(m, &roots, &mut seconds)?;
    let alone: Vec<usize> = {
        let mut seen = HashSet::new();
        listed_first.into_iter().filter(|o| !seconds.contains(o) && seen.insert(*o)).collect()
    };
    // What each lists, and what any of them lists: one listed under another
    // goes with it.
    let mut held = Vec::with_capacity(alone.len());
    let mut under_any = HashSet::new();
    for &object in &alone {
        let mut below = HashSet::new();
        under(m, &[object], &mut below)?;
        under_any.extend(below.iter().copied());
        held.push((object, below));
    }
    for (object, below) in held {
        if under_any.contains(&object) {
            continue;
        }
        if below.iter().any(|o| seconds.contains(o)) {
            SHARED.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let _letting_go = LettingGo::new("country's first history");
        m.call(m.vslot(object, 0)?, &[object, 1])?;
        DELETED.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(EMPTY_LIST, "40 57 48 83 ec 20 48 8b f9 48 8b 49 20 48 85 c9", "history list emptied", 1, Some(emptying), None),
        // Each clear empties the first (+0x11a8), deletes what the second
        // (+0x11a8 + SECOND) lists, and empties it.
        Point::keep(0x2e37d2usize, "48 8d 8f a8 11 00 00 e8 42 47 50 00 48 8b 8f 08 12 00 00", "country clear empties its first history"),
        Point::keep(0x2e380eusize, "48 8d 8f e8 11 00 00", "country clear empties its second history"),
        Point::keep(0x2cc05cusize, "48 8d ab a8 11 00 00 48 8b cd e8 b5 be 51 00 48 8b 8b 08 12 00 00", "country histories emptied"),
        Point::keep(0x2cc09eusize, "48 8d 8b e8 11 00 00", "country second history emptied"),
        Point::keep(0x463789usize, "49 8d 8e a8 11 00 00 e8 8b 47 38 00 49 8b 8e 08 12 00 00", "country histories set again"),
        Point::keep(0x4637bfusize, "49 8d 8e e8 11 00 00", "country second history set again"),
        // A history's destructor deletes what its list holds (LIST, the
        // list's ITEM and NEXT).
        Point::keep(0x7e7c10usize, "48 89 5c 24 08 57 48 83 ec 20 48 8b 41 20 48 8b f9 48 85 c0 74 28", "history list destructor"),
        // Taking a node out with the list kept marks it (the list's REMOVED).
        Point::keep(0x7e930busize, "c6 43 18 01", "history list node marked taken out"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_game::buffers::{Image, Object};
    use crate::native::objects::list;
    use std::cell::RefCell;

    thread_local! {
        static DELETED_OBJECTS: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
    }

    extern "C" fn delete_record(object: usize, flags: usize) -> usize {
        DELETED_OBJECTS.with(|d| d.borrow_mut().push((object, flags)));
        object
    }

    /// Links `items` into the native intrusive list header at `owner + LIST`,
    /// returning the backing node buffers so they remain alive for the test.
    fn link_list(owner: &Object, items: &[usize]) -> Vec<Object> {
        let nodes: Vec<Object> = items.iter().map(|&item| Object::new(list::NODE_SIZE).u64(list::ITEM, item as u64)).collect();
        for i in 0..nodes.len() {
            if i + 1 < nodes.len() {
                let next = nodes[i + 1].at();
                nodes[i].set(list::NEXT, &next.to_le_bytes());
            }
        }
        let head = nodes.first().map_or(0, Object::at);
        let tail = nodes.last().map_or(0, Object::at);
        owner.set(LIST + list::HEAD, &head.to_le_bytes());
        owner.set(LIST + list::TAIL, &tail.to_le_bytes());
        owner.set(LIST + list::COUNT, &(nodes.len() as u32).to_le_bytes());
        nodes
    }

    #[test]
    fn delete_alone_removes_only_top_level_unshared_blocks_and_skips_blocks_holding_second_history_items() {
        DELETED_OBJECTS.with(|d| d.borrow_mut().clear());
        let _image = Image::new();
        let m = Memory { base: 0 };
        let vtable = Object::new(0x10).u64(0, delete_record as *const () as u64);

        let make_rec = || Object::new(LIST + 0x20).u64(0, vtable.at() as u64);
        let exclusive_leaf = make_rec();
        let exclusive_block = make_rec();
        let nested_child = make_rec();
        let shared_leaf = make_rec();
        let partial_block = make_rec();
        let second_only = make_rec();

        // `exclusive_block` owns `nested_child`; `partial_block` owns `shared_leaf` (which `second` also lists).
        let _n1 = link_list(&exclusive_block, &[nested_child.at()]);
        let _n2 = link_list(&partial_block, &[shared_leaf.at()]);

        // Country holds first history at offset 0 and second history at offset SECOND (0x40).
        let pair = Object::new(SECOND + LIST + 0x20);
        let first_nodes = link_list(&pair, &[exclusive_leaf.at(), exclusive_block.at(), nested_child.at(), partial_block.at(), shared_leaf.at()]);
        let second_nodes: Vec<Object> =
            [shared_leaf.at(), second_only.at()].iter().map(|&item| Object::new(list::NODE_SIZE).u64(list::ITEM, item as u64)).collect();
        second_nodes[0].set(list::NEXT, &second_nodes[1].at().to_le_bytes());
        pair.set(SECOND + LIST + list::HEAD, &second_nodes[0].at().to_le_bytes());
        pair.set(SECOND + LIST + list::TAIL, &second_nodes[1].at().to_le_bytes());
        pair.set(SECOND + LIST + list::COUNT, &2u32.to_le_bytes());

        let shared_before = SHARED.load(Ordering::Relaxed);
        delete_alone(&m, pair.at()).unwrap();

        // Only `exclusive_leaf` and `exclusive_block` are deleted:
        // `nested_child` goes with `exclusive_block`, `shared_leaf` is in `second`,
        // and `partial_block` is skipped because it holds `shared_leaf`.
        assert_eq!(DELETED_OBJECTS.with(|d| d.borrow().clone()), vec![(exclusive_leaf.at(), 1), (exclusive_block.at(), 1)]);
        assert_eq!(SHARED.load(Ordering::Relaxed) - shared_before, 1);
        drop((first_nodes, second_nodes));
    }
}
