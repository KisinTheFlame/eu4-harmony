//! The trade nodes each world clear leaves (docs/research/main-menu.md,
//! "The trade nodes").
//!
//! The world's clear reads the trade nodes again (`0x891a80`, `0x8be0f0`):
//! it makes a new database of them and puts it in place of the one before,
//! which it deletes with the vector of its nodes, and not the nodes. The
//! provinces are given the new ones first; the windows of the session's
//! interface may still show the old ones until they go, and the windows of
//! an in-game load's build before stay until the return. Each world reset
//! left the nodes, about 80, with their routes.
//!
//! So as the database is put in place of another, the nodes of the one
//! before wait, and are deleted as the world's next clear begins once a
//! return has taken the session's interface down after they began to wait
//! ([`super::waiting`]).
use super::waiting::{Kind, Moment, Waiting};
use crate::{hooks::Call, memory::Memory, points::Point};
use std::sync::atomic::{AtomicUsize, Ordering};

/// `(_, database)`: puts a database of trade nodes in place of the one
/// before, and deletes that one.
const REPLACE_DATABASE: usize = 0xb682f0;
/// The database in place, and its vector of nodes (begin, end).
const DATABASE: usize = 0x242be48;
const NODES: usize = 0;
/// The node that is none, made once in the image and first in every
/// database.
const NO_NODE: usize = 0x2462c90;
/// A node's class, and its deleting destructor, which deletes its routes.
const NODE: usize = 0x1ce9460;
const NODE_DELETE: usize = 0xb50d60;

/// What the repair lets go.
const NODES_LEFT: Kind = Kind { what: "trade node", class: NODE, delete: NODE_DELETE };

/// The nodes of the databases replaced.
static WAITING: Waiting<()> = Waiting::new();
/// The nodes deleted since the last world reset, for the log.
static DELETED: AtomicUsize = AtomicUsize::new(0);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset), world_clearing: Some(world_clearing), ..super::Repair::NONE };

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!("{} trade nodes the world's clear replaced were deleted", DELETED.swap(0, Ordering::Relaxed))
}

fn replacing(call: &mut Call) {
    let m = call.memory();
    let nodes = (|| {
        let database = m.ptr(m.at(DATABASE))?;
        if database == 0 {
            return Ok(Vec::new());
        }
        let (begin, end) = (m.ptr(database + NODES)?, m.ptr(database + NODES + 8)?);
        (begin..end).step_by(8).map(|at| m.ptr(at)).filter(|n| !matches!(n, Ok(0))).collect::<crate::Result<Vec<_>>>()
    })();
    match nodes {
        Ok(nodes) => {
            for node in nodes.into_iter().filter(|&n| n != m.at(NO_NODE)) {
                WAITING.wait(node, (), Moment::Return);
            }
        }
        Err(e) => log!("repair: the trade nodes being replaced were not read: {e}"),
    }
}

/// The world's clear begins: the nodes that waited from before the last
/// return are deleted.
fn world_clearing(m: &Memory) {
    WAITING.let_go_due(m, Moment::Return, &NODES_LEFT, &DELETED);
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::observe(
            REPLACE_DATABASE,
            "40 53 48 83 ec 20 48 8b 1d 4b 3b 8c 01 48 89 15 44 3b 8c 01",
            "trade node database replaced",
            2,
            Some(replacing),
            None,
        )
        .uses(&[DATABASE]),
        // It lets the vector of nodes go, at the database's start (NODES), not
        // the nodes.
        Point::keep(0xb6832dusize, "48 8b cb e8 8b 67 52 ff", "trade node database's nodes let go"),
        Point::method(NODE, 0, NODE_DELETE, "trade node deleting destructor"),
        Point::keep(0xb60aa0usize, "48 8d 05 e9 21 90 01", "the trade node that is none").uses(&[NO_NODE]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::buffers::{Image, Object};
    use std::sync::Mutex;

    /// The nodes the test's deleting destructor was called with, and its
    /// flags.
    static DELETED_NODES: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

    extern "C" fn delete_node(node: usize, flags: usize) -> usize {
        DELETED_NODES.lock().unwrap().push((node, flags));
        node
    }

    #[test]
    fn a_node_that_waited_from_before_a_return_is_deleted_and_counted_and_one_no_longer_a_node_is_kept() {
        let image = Image::new();
        image.function(NODE_DELETE, delete_node as *const ());
        let m = image.memory();
        let node = Object::new(0x40).u64(0, m.at(NODE) as u64);
        let other = Object::new(0x40).u64(0, 0x1234);
        let waiting: Waiting<()> = Waiting::new();
        let deleted = AtomicUsize::new(0);
        waiting.wait(node.at(), (), Moment::Return);
        waiting.wait(other.at(), (), Moment::Return);
        waiting.let_go_due(m, Moment::Return, &NODES_LEFT, &deleted);
        assert!(DELETED_NODES.lock().unwrap().is_empty(), "no return passed since they began to wait");
        Moment::Return.passed();
        waiting.let_go_due(m, Moment::Return, &NODES_LEFT, &deleted);
        assert_eq!(*DELETED_NODES.lock().unwrap(), vec![(node.at(), 1)]);
        assert_eq!(deleted.load(Ordering::Relaxed), 1);
        assert!(waiting.take_due(Moment::Return).is_empty(), "the one kept no longer waits");
    }
}
