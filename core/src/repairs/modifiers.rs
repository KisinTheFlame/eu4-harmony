//! What a country's modifiers leak as they go or are made again, in play
//! too: the sets of modifiers they copy, which they alone own. Those they
//! are copied over from their own set still lists, and go with them.
use super::LettingGo;
use crate::{
    native::hooks::Call,
    native::memory::Memory,
    native::objects::campaign::{classes, modifiers},
    native::points::Point,
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

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset), ..super::Repair::NONE };

/// Tracks copied modifier sets across in-flight destructors and copy
/// assignments, plus counters for the world-reset summary log.
struct Tracker {
    /// Sets to delete once a country's modifiers finish destructing, or to
    /// queue into `waiting` once a copy finishes, keyed by the modifiers object.
    left: Mutex<Vec<(usize, Vec<usize>)>>,
    /// Sets a country's modifiers held before being copied over, still listed
    /// by their own set until those modifiers are destroyed.
    waiting: Mutex<Vec<(usize, Vec<usize>)>>,
    deleted: AtomicUsize,
    waiting_count: AtomicUsize,
    waited: AtomicUsize,
}

impl Tracker {
    const fn new() -> Self {
        Self {
            left: Mutex::new(Vec::new()),
            waiting: Mutex::new(Vec::new()),
            deleted: AtomicUsize::new(0),
            waiting_count: AtomicUsize::new(0),
            waited: AtomicUsize::new(0),
        }
    }

    fn since_reset(&self) -> String {
        format!(
            "{} sets of modifiers were deleted with what owned them, {} of them once the modifiers that listed them went, and {} wait for the modifiers that list them",
            self.deleted.swap(0, Ordering::Relaxed),
            self.waited.swap(0, Ordering::Relaxed),
            self.waiting_count.load(Ordering::Relaxed)
        )
    }

    fn keep_left(&self, modifiers: usize, sets: crate::base::Result<Vec<usize>>) {
        match sets {
            Ok(sets) if sets.is_empty() => {}
            Ok(sets) => self.left.lock().unwrap_or_else(|e| e.into_inner()).push((modifiers, sets)),
            Err(e) => log!("repair: a country's sets of modifiers were not read: {e}"),
        }
    }

    fn take(list: &Mutex<Vec<(usize, Vec<usize>)>>, modifiers: usize) -> Vec<usize> {
        let mut guard = list.lock().unwrap_or_else(|e| e.into_inner());
        match guard.iter().rposition(|(k, _)| *k == modifiers) {
            Some(at) => guard.swap_remove(at).1,
            None => Vec::new(),
        }
    }

    fn on_going(&self, m: &Memory, modifiers: usize) {
        let waiting = Self::take(&self.waiting, modifiers);
        self.waiting_count.fetch_sub(waiting.len(), Ordering::Relaxed);
        self.waited.fetch_add(waiting.len(), Ordering::Relaxed);
        let sets = modifier_copies(m, modifiers).map(|mut sets| {
            for set in waiting {
                if !sets.contains(&set) {
                    sets.push(set);
                }
            }
            sets
        });
        self.keep_left(modifiers, sets);
    }

    fn on_gone(&self, m: &Memory, modifiers: usize) {
        let sets = Self::take(&self.left, modifiers);
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
                    self.deleted.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => log!("repair: a set of modifiers was not deleted: {e}"),
            }
        }
    }

    fn on_copying(&self, m: &Memory, modifiers: usize, other: usize) {
        let lost = (|| {
            let kept = modifier_copies(m, other)?;
            Ok(modifier_copies(m, modifiers)?.into_iter().filter(|s| !kept.contains(s)).collect())
        })();
        self.keep_left(modifiers, lost);
    }

    fn on_copied(&self, m: &Memory, modifiers: usize, other: usize) {
        let lost = Self::take(&self.left, modifiers);
        if !lost.is_empty() {
            if super::config().check_repairs {
                check_listed(m, modifiers, &lost);
            }
            self.waiting_count.fetch_add(lost.len(), Ordering::Relaxed);
            let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
            match waiting.iter_mut().find(|(k, _)| *k == modifiers) {
                Some((_, sets)) => sets.extend(lost),
                None => waiting.push((modifiers, lost)),
            }
        }
        let moved = m.ptr(other + modifiers::COPIES).and_then(|begin| m.write(other + modifiers::COPIES + 8, &begin.to_le_bytes()));
        if let Err(e) = moved {
            log!("repair: the sets of modifiers copied were not moved: {e}");
        }
    }
}

static TRACKER: Tracker = Tracker::new();

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    TRACKER.since_reset()
}

/// The sets a country's modifiers copied, which nothing else points to.
fn modifier_copies(m: &Memory, modifiers: usize) -> crate::base::Result<Vec<usize>> {
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

/// A country's modifiers go, or are made again in place: the sets they
/// copied, which their destructor leaves, and those they had before they
/// were copied over, which their own set listed until now, are kept to be
/// deleted once they are gone ([`modifiers_gone`]) with what of them points
/// to the sets.
fn modifiers_going(call: &mut Call) {
    TRACKER.on_going(&call.memory(), call.arg(0));
}

fn modifiers_gone(call: &mut Call) {
    TRACKER.on_gone(&call.memory(), call.arg(0));
}

/// A country's modifiers are copied over from other modifiers: the sets
/// they copied that the others do not hold would be lost, and wait for them
/// to go once the copy is made ([`modifiers_copied`]).
fn modifiers_copying(call: &mut Call) {
    TRACKER.on_copying(&call.memory(), call.arg(0), call.arg(1));
}

/// The copy is made: their own set still lists the lost sets, as a set's
/// copy leaves the list it has, so the lost sets wait for the modifiers to
/// go; and the other modifiers no longer point to the sets they now share,
/// so that the sets go with these alone: the copy moves them.
fn modifiers_copied(call: &mut Call) {
    TRACKER.on_copied(&call.memory(), call.arg(0), call.arg(1));
}

/// Logs the lost sets the modifiers' own set does not list: a check, when the
/// heap census is enabled, of what the repair takes the game to do.
fn check_listed(m: &Memory, modifiers: usize, lost: &[usize]) {
    let listed = (|| {
        let added = modifiers + modifiers::OWN + crate::native::objects::campaign::modifier_set::ADDED;
        let (begin, end) = (m.ptr(added)?, m.ptr(added + 8)?);
        let mut sets = Vec::new();
        if end > begin {
            for at in (begin..end).step_by(8) {
                sets.push(m.ptr(at)?);
            }
        }
        crate::base::Result::Ok(sets)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_game::buffers::{Image, Object};
    use std::cell::RefCell;

    thread_local! {
        static DELETED: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
    }

    extern "C" fn delete_set(set: usize, flags: usize) -> usize {
        DELETED.with(|d| d.borrow_mut().push((set, flags)));
        set
    }

    #[test]
    fn copy_moves_source_sets_queues_overwritten_sets_and_destructor_deletes_only_valid_modifier_sets() {
        DELETED.with(|d| d.borrow_mut().clear());
        let image = Image::new();
        let m = image.memory();
        image.u64(classes::MODIFIER_SET, delete_set as *const () as u64);

        let lost_set = Object::new(0x40).u64(0, m.at(classes::MODIFIER_SET) as u64);
        let shared_set = Object::new(0x40).u64(0, m.at(classes::MODIFIER_SET) as u64);
        let new_set = Object::new(0x40).u64(0, m.at(classes::MODIFIER_SET) as u64);
        let wrong_class = Object::new(0x40).u64(0, 0xdead_beef);

        let dst = Object::new(modifiers::COPIES + 0x20).vector(modifiers::COPIES, &[lost_set.at() as u64, shared_set.at() as u64]);
        let src = Object::new(modifiers::COPIES + 0x20).vector(modifiers::COPIES, &[shared_set.at() as u64, new_set.at() as u64, wrong_class.at() as u64]);

        let tracker = Tracker::new();
        tracker.on_copying(m, dst.at(), src.at());
        // Simulate native copy assigning src's COPIES into dst.
        let src_begin = m.ptr(src.at() + modifiers::COPIES).unwrap();
        let src_end = m.ptr(src.at() + modifiers::COPIES + 8).unwrap();
        dst.set(modifiers::COPIES, &src_begin.to_le_bytes());
        dst.set(modifiers::COPIES + 8, &src_end.to_le_bytes());
        tracker.on_copied(m, dst.at(), src.at());

        // Ownership moved out of `src`: its COPIES vector is now empty, while `lost_set` waits for `dst`.
        assert_eq!(m.ptr(src.at() + modifiers::COPIES + 8).unwrap(), src_begin);
        assert_eq!(tracker.waiting_count.load(Ordering::Relaxed), 1);

        // Destroying `src` deletes nothing because its sets were moved to `dst`.
        tracker.on_going(m, src.at());
        tracker.on_gone(m, src.at());
        assert!(DELETED.with(|d| d.borrow().is_empty()));

        // Destroying `dst` deletes `shared_set`, `new_set`, and `lost_set` once each, skipping `wrong_class`.
        tracker.on_going(m, dst.at());
        tracker.on_gone(m, dst.at());
        assert_eq!(DELETED.with(|d| d.borrow().clone()), vec![(shared_set.at(), 1), (new_set.at(), 1), (lost_set.at(), 1)]);
        assert_eq!(tracker.waiting_count.load(Ordering::Relaxed), 0);
        assert_eq!(tracker.waited.load(Ordering::Relaxed), 1);
        assert_eq!(tracker.deleted.load(Ordering::Relaxed), 3);
    }
}
