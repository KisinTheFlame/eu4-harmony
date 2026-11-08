//! The entries a list's kill leaves (docs/research/main-menu.md, "The
//! entries a list's kill leaves").
//!
//! A list of the session's panels keeps its entries, window holders, as the
//! connections of its signal. As its window is killed, the list kills each
//! entry's window and disconnects them all, and deletes none: its destructor
//! finds no entry left, and nothing else holds them. The map modes', the
//! policies', the institutions' and the disasters' entries and the
//! diplomacy view's shields stayed so, about 20 KB a generation.
//!
//! So as such a list is killed, its entries of those classes wait, and are
//! deleted as the world's clear begins once a return has taken the
//! session's interface down after they began to wait
//! ([`super::waiting`]), as the trade nodes are. An entry deleted before
//! then, by whatever deletes it, waits no more.
use super::{
    waiting::{self, Moment, Waiting},
    LettingGo,
};
use crate::{
    hooks::Call,
    memory::Memory,
    points::{GameFunction, Point},
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// `(list)`: a list's kill, the method its window's kill calls on it.
const LIST_KILL: usize = 0x163d6f0;
/// A list's class, and the kill's place in it.
const LIST: usize = 0x1da4910;
const LIST_KILL_SLOT: usize = 0x78;
/// The list's flag that it was killed, and its signal, whose connections
/// are its entries: a list of nodes at `+0x10`, each with its entry at `+0`
/// and the next node at `+0x10`.
const KILLED_FLAG: usize = 0xc4;
const SIGNAL: usize = 0xf0;
const CONNECTIONS: usize = 0x10;
const CONNECTION_ITEM: usize = 0;
const CONNECTION_NEXT: usize = 0x10;
/// `(holder)`: a window holder's destructor, which every entry's runs, and
/// the holder's window, which the list's kill killed and forgot.
const HOLDER_DESTRUCTOR: GameFunction<1> = GameFunction::at(0x152e030);
const HOLDER_WINDOW: usize = 0x40;

/// The classes of the entries left, each with its deleting destructor: the
/// map modes' entry, the policies' (`countrydecisionsview_possiblepolicy`),
/// the institutions' (`countrytechnologyview_institution`), the disasters'
/// (`disaster_item`) and the diplomacy view's two shields.
const ENTRIES: [(usize, usize, &str); 6] = [
    (0x1d6bad0, 0x12444a0, "map mode entry deleting destructor"),
    (0x1d49e28, 0x8450f0, "policy entry deleting destructor"),
    (0x1d4e8a8, 0x10188f0, "institution entry deleting destructor"),
    (0x1d45188, 0xecfce0, "disaster entry deleting destructor"),
    (0x1d3a778, 0xef48f0, "diplomacy shield entry deleting destructor"),
    (0x1d3a648, 0xef5ba0, "diplomacy shield entry deleting destructor (second)"),
];
/// The diplomacy view's shields, which their destructors cannot delete once
/// their window is gone: each first takes its own delegate off the shield in
/// its window, which the kill killed and forgot. The window, and the
/// shield's list of delegates with it, is gone, so they are destroyed as
/// their destructors destroy them from there: their texts, then the
/// holder's, then freed. Each class with its size and its texts, its own
/// first.
const SHIELDS: [(usize, usize, &[usize]); 2] = [(0x1d3a778, 0x198, &[0x88, 0x68]), (0x1d3a648, 0x1d8, &[0x1b8, 0x198, 0x88, 0x68])];

/// The entries the kills left, each with its class.
static WAITING: Waiting<usize> = Waiting::new();
/// The entries deleted since the last world reset, for the log.
static DELETED: AtomicUsize = AtomicUsize::new(0);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset), world_clearing: Some(world_clearing), ..super::Repair::NONE };

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!("{} entries the lists' kills left were deleted", DELETED.swap(0, Ordering::Relaxed))
}

fn list_killing(call: &mut Call) {
    let list = call.arg(0);
    let m = call.memory();
    let found = (|| {
        let mut found = Vec::new();
        if m.u8(list + KILLED_FLAG)? != 0 {
            return Ok(found);
        }
        let classes: Vec<usize> = ENTRIES.iter().map(|(class, _, _)| m.at(*class)).collect();
        let mut node = m.ptr(list + SIGNAL + CONNECTIONS)?;
        while node != 0 {
            let entry = m.ptr(node + CONNECTION_ITEM)?;
            if entry != 0 {
                let class = m.ptr(entry)?;
                if classes.contains(&class) {
                    found.push((entry, class));
                }
            }
            node = m.ptr(node + CONNECTION_NEXT)?;
        }
        crate::Result::Ok(found)
    })();
    match found {
        Ok(found) => {
            for (entry, class) in found {
                WAITING.wait(entry, class, Moment::Return);
            }
        }
        Err(e) => log!("repair: the entries of a list being killed were not read: {e}"),
    }
}

/// A window holder is destroyed: if it was an entry waiting, it waits no
/// more.
fn holder_going(call: &mut Call) {
    WAITING.withdraw(call.arg(0));
}

/// The world's clear begins: the entries that waited from before the last
/// return are deleted.
fn world_clearing(m: &Memory) {
    for (entry, class) in WAITING.take_due(Moment::Return) {
        let deleted = (|| {
            if m.ptr(entry)? != class || m.ptr(entry + HOLDER_WINDOW)? != 0 {
                log!("repair: a list's entry left was no longer one, {entry:#x}; kept");
                return Ok(());
            }
            match SHIELDS.iter().find(|(shield, _, _)| m.at(*shield) == class) {
                Some((_, size, texts)) => {
                    let _letting_go = LettingGo::new("list entry");
                    for text in *texts {
                        crate::game_string::destroy_checked(m, entry + text)?;
                    }
                    HOLDER_DESTRUCTOR.call(m, [entry])?;
                    crate::game_heap::delete(m, entry, *size)?;
                }
                None => waiting::delete_by_its_class(m, entry, "list entry")?,
            }
            DELETED.fetch_add(1, Ordering::Relaxed);
            crate::Result::Ok(())
        })();
        if let Err(e) = deleted {
            log!("repair: an entry a list's kill left, {entry:#x}, was not deleted: {e}");
        }
    }
}

pub(super) fn points() -> Vec<Point> {
    let mut points = vec![
        Point::observe(LIST_KILL, "40 57 48 83 ec 20 80 b9 c4 00 00 00 00 48 8b f9 75 4c 48 81 c1 f0 00 00 00", "list kill", 1, Some(list_killing), None)
            .uses(&[KILLED_FLAG, SIGNAL]),
        Point::method(LIST, LIST_KILL_SLOT, LIST_KILL, "list's kill"),
        // The signal's disconnecting of every connection walks them so, each
        // unlinked from its entry's own connections too, and frees them.
        Point::keep(0x153015ausize, "48 8b 59 10 48 8b e9 48", "signal's connections").uses(&[CONNECTIONS]),
        Point::keep(0x15301f3usize, "48 8b 5b 10 48 85 db 75", "signal's next connection").uses(&[CONNECTION_NEXT]),
        Point::observe(
            HOLDER_DESTRUCTOR,
            "40 53 48 83 ec 20 48 8b d9 48 8d 05 20 73 83 00 48 89 01 48 8d 05 f6 c0 81 00 48 89 41 38",
            "window holder destructor",
            1,
            Some(holder_going),
            None,
        ),
        Point::keep(0x152e04eusize, "48 8b 49 40 48 85 c9", "window holder's window").uses(&[HOLDER_WINDOW]),
        // The list's kill has each entry kill its window and forget it.
        Point::keep(0x152e105usize, "ff 90 30 01 00 00 48 c7 43 08 00 00 00 00", "window holder kills its window and forgets it"),
    ];
    points.extend(ENTRIES.iter().map(|(class, delete, what)| Point::method(*class, 0, *delete, what)));
    points.extend([
        // The shields' destructors: first their delegate off their window's
        // shield, then their texts and the holder's destructor.
        Point::keep(0xef4952usize, "48 8b 49 40 48 8b 01", "diplomacy shield entry's window"),
        Point::keep(
            0xef49b1usize,
            "48 8d 8f 88 00 00 00 e8 a3 0c 1a ff 48 8d 4f 68 e8 9a 0c 1a ff 48 8b cf 48 8b 5c 24 30 48 83 c4 20 5f e9 58 96 63 00",
            "diplomacy shield entry's texts and holder destroyed",
        ),
        Point::keep(0xef4909usize, "ba 98 01 00 00", "diplomacy shield entry size"),
        Point::keep(
            0xef5bc4usize,
            "48 81 c1 b8 01 00 00 e8 90 fa 19 ff 48 8d 8f 98 01 00 00 e8 84 fa 19 ff 48 8b cf e8 4c ed ff ff",
            "second diplomacy shield entry's texts destroyed",
        ),
        Point::keep(0xef5be9usize, "ba d8 01 00 00", "second diplomacy shield entry size"),
    ]);
    points
}
