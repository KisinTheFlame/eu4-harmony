//! What a country's first history lists alone, which its clear drops
//! (docs/research/main-menu.md, "A country's two histories").
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
use crate::{hooks::Call, memory::Memory, points::Point};
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
/// A history's list, and in it each node's object, the next node and
/// whether the node was taken out, its object deleted.
const LIST: usize = 0x20;
use crate::objects::list::{ITEM as NODE_OBJECT, NEXT as NODE_NEXT, REMOVED as NODE_REMOVED};
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
fn listed(m: &Memory, history: usize) -> crate::Result<Vec<usize>> {
    let mut objects = Vec::new();
    let mut node = m.ptr(history + LIST)?;
    while node != 0 && objects.len() < MOST {
        let object = m.ptr(node + NODE_OBJECT)?;
        if object != 0 && m.u8(node + NODE_REMOVED)? == 0 {
            objects.push(object);
        }
        node = m.ptr(node + NODE_NEXT)?;
    }
    Ok(objects)
}

/// Everything under `objects`, at any depth, not counting them.
fn under(m: &Memory, objects: &[usize], found: &mut HashSet<usize>) -> crate::Result<()> {
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

fn delete_alone(m: &Memory, first: usize) -> crate::Result<()> {
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
        // A history's destructor deletes what its list holds (LIST, NODE_OBJECT, NODE_NEXT).
        Point::keep(0x7e7c10usize, "48 89 5c 24 08 57 48 83 ec 20 48 8b 41 20 48 8b f9 48 85 c0 74 28", "history list destructor"),
        // Taking a node out with the list kept marks it (NODE_REMOVED).
        Point::keep(0x7e930busize, "c6 43 18 01", "history list node marked taken out"),
    ]
}
