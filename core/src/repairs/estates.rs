//! The modifiers the estates leave (docs/research/main-menu.md, "The
//! estates' modifiers").
//!
//! A country keeps its estates by value in a vector, each with its modifiers
//! in two vectors of pointers, which the estate deletes as they run out in
//! play and its destructor lets go without them. The world's clear empties
//! every country's estates so, and every modifier an estate still had stayed:
//! about 30 KB a generation of loads and new games. An estate's copy shares
//! its modifiers for a moment (a vector growing, the interface's copies), so
//! its destructor is not where they can go.
//!
//! So as the world's clear begins, before any country is cleared, every
//! country's estates' modifiers wait, and are deleted as a later clear begins
//! once a return has taken the session's interface down after they began to
//! wait ([`super::session_interface::RETURNS`]), as the trade nodes are.
use super::LettingGo;
use crate::{
    memory::Memory,
    objects::campaign::{countries as country_list, globals},
    points::{GameFunction, Point},
};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

/// A country's vector of estates (begin, end), each of [`ESTATE_SIZE`] bytes.
const ESTATES: usize = 0x21b0;
const ESTATE_SIZE: usize = 0xa8;
/// An estate's class, and its two vectors of modifiers (begin, end).
const ESTATE: usize = 0x1c56838;
const MODIFIERS: [usize; 2] = [0x58, 0x70];
/// An estate's modifier's class, and its deleting destructor.
const MODIFIER: usize = 0x1cacf28;
const MODIFIER_DELETE: GameFunction<2> = GameFunction::at(0x7158b0);

/// The modifiers of the estates cleared, each with the returns there had
/// been as it began to wait.
static WAITING: Mutex<Vec<(usize, u32)>> = Mutex::new(Vec::new());
/// The modifiers deleted since the last world reset, for the log.
static DELETED: AtomicUsize = AtomicUsize::new(0);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset) };

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!("{} modifiers the estates it cleared left were deleted", DELETED.swap(0, Ordering::Relaxed))
}

/// The world's clear begins: the modifiers that waited from before the last
/// return are deleted, and those of the estates it is about to clear wait.
pub(super) fn world_clearing(m: &Memory) {
    let returns = super::session_interface::RETURNS.load(Ordering::Relaxed);
    let due: Vec<usize> = {
        let mut waiting = WAITING.lock().unwrap_or_else(|e| e.into_inner());
        let due = waiting.iter().filter(|(_, since)| *since < returns).map(|(modifier, _)| *modifier).collect();
        waiting.retain(|(_, since)| *since >= returns);
        due
    };
    let deleted = (|| {
        for modifier in due {
            if m.ptr(modifier)? != m.at(MODIFIER) {
                log!("repair: an estate's modifier left was not one, {modifier:#x}; kept");
                continue;
            }
            let _letting_go = LettingGo::new("estate's modifier");
            MODIFIER_DELETE.call(m, [modifier, 1])?;
            DELETED.fetch_add(1, Ordering::Relaxed);
        }
        crate::Result::Ok(())
    })();
    if let Err(e) = deleted {
        log!("repair: the estates' modifiers left were not all deleted: {e}");
    }
    match modifiers(m) {
        Ok(found) => {
            let mut waiting = WAITING.lock().unwrap_or_else(|e| e.into_inner());
            let already: HashSet<usize> = waiting.iter().map(|(modifier, _)| *modifier).collect();
            waiting.extend(found.into_iter().filter(|modifier| !already.contains(modifier)).map(|modifier| (modifier, returns)));
        }
        Err(e) => log!("repair: the estates' modifiers were not read: {e}"),
    }
}

/// Every country's estates' modifiers, each once.
fn modifiers(m: &Memory) -> crate::Result<Vec<usize>> {
    let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
    let list = m.ptr(manager + country_list::LIST)?;
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for index in 0..m.i32(manager + country_list::COUNT)?.max(0) as usize {
        let country = m.ptr(list + index * 8)?;
        if country == 0 {
            continue;
        }
        let (begin, end) = (m.ptr(country + ESTATES)?, m.ptr(country + ESTATES + 8)?);
        for estate in (begin..end).step_by(ESTATE_SIZE) {
            if m.ptr(estate)? != m.at(ESTATE) {
                continue;
            }
            for vector in MODIFIERS {
                let (first, last) = (m.ptr(estate + vector)?, m.ptr(estate + vector + 8)?);
                for at in (first..last).step_by(8) {
                    let modifier = m.ptr(at)?;
                    if modifier != 0 && seen.insert(modifier) {
                        found.push(modifier);
                    }
                }
            }
        }
    }
    Ok(found)
}

pub(super) fn points() -> Vec<Point> {
    vec![
        // A country's clear empties its estates, each by its first slot,
        // not deleting it, which lets its modifiers' vectors go.
        Point::keep(0x2e217eusize, "48 8d 8f b0 21 00 00 e8 a6 49 13 00", "country's estates cleared").uses(&[ESTATES]),
        Point::keep(0x416b5ausize, "48 81 c3 a8 00 00 00", "estate size").uses(&[ESTATE_SIZE]),
        Point::keep(0x42ebbeusize, "48 8d 4f 70 e8", "estate's second modifiers let go").uses(&[MODIFIERS[1]]),
        Point::keep(0x42ebc7usize, "48 8d 4f 58 e8", "estate's first modifiers let go").uses(&[MODIFIERS[0]]),
        Point::keep(0x42ebcbusize, "e8 f0 fe c5 ff", "estate's modifiers' vector let go without them"),
        // An estate's move and its modifier's constructor set their classes.
        Point::keep(0x42ea4busize, "48 8d 05 e6 7d 82 01", "estate class").uses(&[ESTATE]),
        Point::keep(0x71570eusize, "48 8d 05 13 78 59 01", "estate modifier class").uses(&[MODIFIER]),
        Point::keep(MODIFIER_DELETE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b d9 8b f2 48 81 c1 90 00 00 00", "estate modifier deleting destructor"),
    ]
}
