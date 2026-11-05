//! What a country's modifiers leak as they go or are made again, in play
//! too: the sets of modifiers they copy, which they alone own. Those they
//! are copied over from their own set still lists, and go with them.
use super::LettingGo;
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::campaign::{classes, modifiers},
    points::Point,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

/// `(modifiers)`: the destructor of a country's modifiers; `(modifiers,
/// other)`: their copy from another's.
const MODIFIERS_DESTRUCTOR: usize = 0x93ae10;
const MODIFIERS_COPY: usize = 0x3296b0;
/// `(set, flags)`: a set of modifiers' deleting destructor, its first slot.
const MODIFIER_SET_DELETE: usize = 0x1b2290;

/// The sets of modifiers deleted since the last world reset, those of them
/// deleted once the modifiers that listed them went, and those that still
/// wait for them, for the log.
pub(super) static MODIFIER_SETS: AtomicUsize = AtomicUsize::new(0);
pub(super) static MODIFIER_SETS_WAITING_COUNT: AtomicUsize = AtomicUsize::new(0);
pub(super) static MODIFIER_SETS_WAITED: AtomicUsize = AtomicUsize::new(0);

/// The sets of modifiers to delete once a country's modifiers have gone, or
/// to wait for them once they are copied over, by those modifiers.
static MODIFIER_SETS_LEFT: Mutex<Vec<(usize, Vec<usize>)>> = Mutex::new(Vec::new());

/// The sets of modifiers a country's modifiers had before they were copied
/// over, which their own set still lists: deleted when they go, by them.
static MODIFIER_SETS_WAITING: Mutex<Vec<(usize, Vec<usize>)>> = Mutex::new(Vec::new());

/// The sets a country's modifiers copied, which nothing else points to.
fn modifier_copies(m: &Memory, modifiers: usize) -> crate::Result<Vec<usize>> {
    let (begin, end) = (m.ptr(modifiers + modifiers::COPIES)?, m.ptr(modifiers + modifiers::COPIES + 8)?);
    let mut sets = Vec::new();
    if end > begin {
        for at in (begin..end).step_by(8) {
            let set = m.ptr(at)?;
            if set != 0 && !sets.contains(&set) {
                sets.push(set);
            }
        }
    }
    Ok(sets)
}

/// Deletes sets of modifiers by their first slots, those of their class.
fn delete_modifier_sets(m: &Memory, sets: Vec<usize>) {
    for set in sets {
        let deleted = (|| {
            if m.ptr(set)? != m.at(classes::MODIFIER_SET) {
                return Err(format!("{set:#x} is not a set of modifiers"));
            }
            let _letting_go = LettingGo::new("set of modifiers");
            m.call(m.vslot(set, 0)?, &[set, 1])
        })();
        match deleted {
            Ok(_) => {
                MODIFIER_SETS.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => log!("repair: a set of modifiers was not deleted: {e}"),
        }
    }
}

fn keep_modifier_sets(modifiers: usize, sets: crate::Result<Vec<usize>>) {
    match sets {
        Ok(sets) if sets.is_empty() => {}
        Ok(sets) => MODIFIER_SETS_LEFT.lock().unwrap_or_else(|e| e.into_inner()).push((modifiers, sets)),
        Err(e) => log!("repair: a country's sets of modifiers were not read: {e}"),
    }
}

fn take_modifier_sets(list: &Mutex<Vec<(usize, Vec<usize>)>>, modifiers: usize) -> Vec<usize> {
    let mut left = list.lock().unwrap_or_else(|e| e.into_inner());
    match left.iter().rposition(|(k, _)| *k == modifiers) {
        Some(at) => left.swap_remove(at).1,
        None => Vec::new(),
    }
}

fn kept_modifier_sets(modifiers: usize) -> Vec<usize> {
    take_modifier_sets(&MODIFIER_SETS_LEFT, modifiers)
}

/// A country's modifiers go, or are made again in place: the sets they
/// copied, which their destructor leaves, and those they had before they
/// were copied over, which their own set listed until now, are kept to be
/// deleted once they are gone ([`modifiers_gone`]) with what of them points
/// to the sets.
extern "C" fn modifiers_going(frame: *mut HookFrame) {
    crate::guarded("modifiers destructor", || {
        let m = crate::hooked_memory();
        let modifiers = unsafe { (*frame).args[0] };
        let waiting = take_modifier_sets(&MODIFIER_SETS_WAITING, modifiers);
        MODIFIER_SETS_WAITING_COUNT.fetch_sub(waiting.len(), Ordering::Relaxed);
        MODIFIER_SETS_WAITED.fetch_add(waiting.len(), Ordering::Relaxed);
        let sets = modifier_copies(&m, modifiers).map(|mut sets| {
            for set in waiting {
                if !sets.contains(&set) {
                    sets.push(set);
                }
            }
            sets
        });
        keep_modifier_sets(modifiers, sets);
    });
}

extern "C" fn modifiers_gone(frame: *mut HookFrame) {
    crate::guarded("modifiers destructor", || {
        let m = crate::hooked_memory();
        delete_modifier_sets(&m, kept_modifier_sets(unsafe { (*frame).args[0] }));
    });
}

/// A country's modifiers are copied over from other modifiers: the sets
/// they copied that the others do not hold would be lost, and wait for them
/// to go once the copy is made ([`modifiers_copied`]).
extern "C" fn modifiers_copying(frame: *mut HookFrame) {
    crate::guarded("modifiers copy", || {
        let m = crate::hooked_memory();
        let (modifiers, other) = unsafe { ((*frame).args[0], (*frame).args[1]) };
        let lost = (|| {
            let kept = modifier_copies(&m, other)?;
            Ok(modifier_copies(&m, modifiers)?.into_iter().filter(|s| !kept.contains(s)).collect())
        })();
        keep_modifier_sets(modifiers, lost);
    });
}

/// The copy is made: their own set still lists the lost sets, as a set's
/// copy leaves the list it has, so the lost sets wait for the modifiers to
/// go; and the other modifiers no longer point to the sets they now share,
/// so that the sets go with these alone: the copy moves them.
extern "C" fn modifiers_copied(frame: *mut HookFrame) {
    crate::guarded("modifiers copy", || {
        let m = crate::hooked_memory();
        let (modifiers, other) = unsafe { ((*frame).args[0], (*frame).args[1]) };
        let lost = kept_modifier_sets(modifiers);
        if !lost.is_empty() {
            #[cfg(feature = "dev-heap-census")]
            check_listed(&m, modifiers, &lost);
            MODIFIER_SETS_WAITING_COUNT.fetch_add(lost.len(), Ordering::Relaxed);
            let mut waiting = MODIFIER_SETS_WAITING.lock().unwrap_or_else(|e| e.into_inner());
            match waiting.iter_mut().find(|(k, _)| *k == modifiers) {
                Some((_, sets)) => sets.extend(lost),
                None => waiting.push((modifiers, lost)),
            }
        }
        let moved = m.ptr(other + modifiers::COPIES).and_then(|begin| m.write(other + modifiers::COPIES + 8, &begin.to_le_bytes()));
        if let Err(e) = moved {
            log!("repair: the sets of modifiers copied were not moved: {e}");
        }
    });
}

/// Logs the lost sets the modifiers' own set does not list: a check, in
/// builds with the heap census, of what the repair takes the game to do.
#[cfg(feature = "dev-heap-census")]
fn check_listed(m: &Memory, modifiers: usize, lost: &[usize]) {
    let listed = (|| {
        let added = modifiers + modifiers::OWN + crate::objects::campaign::modifier_set::ADDED;
        let (begin, end) = (m.ptr(added)?, m.ptr(added + 8)?);
        let mut sets = Vec::new();
        if end > begin {
            for at in (begin..end).step_by(8) {
                sets.push(m.ptr(at)?);
            }
        }
        crate::Result::Ok(sets)
    })();
    match listed {
        Ok(listed) => {
            let unlisted = lost.iter().filter(|s| !listed.contains(s)).count();
            if unlisted > 0 {
                log!("repair: {unlisted} of {} sets of modifiers copied over are not listed by the modifiers' own set", lost.len());
            }
        }
        Err(e) => log!("repair: the modifiers' own set's list was not read: {e}"),
    }
}

pub(super) fn points() -> Vec<Point> {
    vec![
        // A country's modifiers' destructor and copy, which leave the sets
        // they copied; a set goes by its first slot.
        Point::observe(
            MODIFIERS_DESTRUCTOR,
            "40 53 48 83 ec 20 48 8b d9 48 81 c1 78 6c 00 00 e8 8b 96 75 ff",
            "modifiers destructor",
            1,
            Some(modifiers_going),
            Some(modifiers_gone),
        ),
        Point::observe(
            MODIFIERS_COPY,
            "48 89 5c 24 10 48 89 6c 24 18 56 57 41 56 48 83 ec 20 8b 02 48 8b f2 89 01",
            "modifiers copy",
            2,
            Some(modifiers_copying),
            Some(modifiers_copied),
        ),
        Point::method(classes::MODIFIER_SET, 0, MODIFIER_SET_DELETE, "modifier set deleting destructor"),
    ]
}
