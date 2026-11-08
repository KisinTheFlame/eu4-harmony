//! What the repairs let go only once the game can no longer use it. An
//! object of the game's that nothing will delete waits from the moment a
//! repair finds it until a later moment of the sessions has passed: a return
//! that took the session's interface down, which may show it until then, or
//! a world reset that ended, which may still use it until then. It goes as
//! its repair next acts after that, unless the game deleted it itself in the
//! meantime, which takes it out of the waiting. Letting one go too early is
//! what crashed the game with a pure call (`super::session_interface`), so
//! the rule is kept here, once.
use super::LettingGo;
use crate::memory::Memory;
use crate::points::GameFunction;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering},
        Mutex, MutexGuard,
    },
};

/// The moments an object waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Moment {
    /// A return whose session's interface is gone (`super::session_interface`).
    Return,
    /// A world reset that ended (`crate::sessions`).
    Reset,
}

/// The returns whose session's interface is gone, and the world resets that
/// ended.
static RETURNS: AtomicU32 = AtomicU32::new(0);
static RESETS: AtomicU32 = AtomicU32::new(0);

impl Moment {
    fn count(self) -> &'static AtomicU32 {
        match self {
            Moment::Return => &RETURNS,
            Moment::Reset => &RESETS,
        }
    }

    /// The moment passed once more: what waited for it from before is due.
    pub(super) fn passed(self) {
        self.count().fetch_add(1, Ordering::Relaxed);
    }

    /// How many times it has passed.
    fn now(self) -> u32 {
        self.count().load(Ordering::Relaxed)
    }
}

/// The objects one repair keeps waiting, each with what the repair knows of
/// it (`K`): its class, or which of the things it lets go it is.
pub(super) struct Waiting<K> {
    objects: Mutex<Option<Objects<K>>>,
}

/// The objects waiting, each with what it is, the moment it waits for, how
/// many times that had passed as it began to wait, and its place in the
/// order they began.
struct Objects<K> {
    waiting: HashMap<usize, (K, Moment, u32, u64)>,
    next: u64,
}

impl<K: Copy> Objects<K> {
    fn new() -> Self {
        Self { waiting: HashMap::new(), next: 0 }
    }

    /// `object` waits for `moment` to pass, which had passed `since` times;
    /// one already waiting keeps the time it began to.
    fn wait(&mut self, object: usize, kind: K, moment: Moment, since: u32) {
        let next = &mut self.next;
        self.waiting.entry(object).or_insert_with(|| {
            *next += 1;
            (kind, moment, since, *next)
        });
    }

    /// Takes out those that wait for `moment` and began before it last
    /// passed, its `now`th time, in the order they began to wait.
    fn take_due(&mut self, moment: Moment, now: u32) -> Vec<(usize, K)> {
        let mut due: Vec<(u64, usize, K)> = self
            .waiting
            .iter()
            .filter(|(_, (_, waits_for, since, _))| *waits_for == moment && *since < now)
            .map(|(&object, &(kind, _, _, order))| (order, object, kind))
            .collect();
        due.sort_unstable_by_key(|(order, _, _)| *order);
        for (_, object, _) in &due {
            self.waiting.remove(object);
        }
        due.into_iter().map(|(_, object, kind)| (object, kind)).collect()
    }
}

impl<K: Copy> Waiting<K> {
    pub(super) const fn new() -> Self {
        Self { objects: Mutex::new(None) }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Objects<K>>> {
        self.objects.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `object` waits from now on until `moment` has passed; one already
    /// waiting keeps the time it began to.
    pub(super) fn wait(&self, object: usize, kind: K, moment: Moment) {
        self.lock().get_or_insert_with(Objects::new).wait(object, kind, moment, moment.now());
    }

    /// `object`, if it waits, waits no more: the game deleted it.
    pub(super) fn withdraw(&self, object: usize) {
        if let Some(objects) = self.lock().as_mut() {
            objects.waiting.remove(&object);
        }
    }

    /// Takes out those that are due, having waited for `moment` from before
    /// it last passed, in the order they began to wait. They are out before
    /// the repair deletes them, whose destructors may withdraw what they
    /// delete.
    pub(super) fn take_due(&self, moment: Moment) -> Vec<(usize, K)> {
        match self.lock().as_mut() {
            Some(objects) => objects.take_due(moment, moment.now()),
            None => Vec::new(),
        }
    }
}

/// A kind of object a repair lets go, all of one class: what the log calls
/// it, its class, and its deleting destructor, `(object, flags)`.
pub(super) struct Kind {
    pub what: &'static str,
    pub class: usize,
    pub delete: GameFunction<2>,
}

impl Kind {
    /// Deletes `object` if it is still one of the kind, by its class, and
    /// answers whether it did; one that is no longer is kept and logged.
    pub(super) fn delete(&self, m: &Memory, object: usize) -> crate::Result<bool> {
        if m.ptr(object)? != m.at(self.class) {
            log!("repair: a {} left was not one, {object:#x}; kept", self.what);
            return Ok(false);
        }
        let _letting_go = LettingGo::new(self.what);
        self.delete.call(m, [object, 1])?;
        Ok(true)
    }
}

impl Waiting<()> {
    /// Deletes those of `kind` that are due after `moment`, counting each in
    /// `deleted`.
    pub(super) fn let_go_due(&self, m: &Memory, moment: Moment, kind: &Kind, deleted: &AtomicUsize) {
        for (object, ()) in self.take_due(moment) {
            match kind.delete(m, object) {
                Ok(true) => {
                    deleted.fetch_add(1, Ordering::Relaxed);
                }
                Ok(false) => {}
                Err(e) => log!("repair: a {} left, {object:#x}, was not deleted: {e}", kind.what),
            }
        }
    }
}

/// Deletes `object` by its own deleting destructor, its vtable's first slot,
/// as `what` lets it go.
pub(super) fn delete_by_its_class(m: &Memory, object: usize, what: &'static str) -> crate::Result<()> {
    let _letting_go = LettingGo::new(what);
    m.call(m.vslot(object, 0)?, &[object, 1])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_object_is_due_only_once_the_moment_it_waits_for_passed_after_it_began() {
        let mut objects = Objects::new();
        objects.wait(0x10, (), Moment::Return, 3);
        assert!(objects.take_due(Moment::Return, 3).is_empty(), "the return it began after has not passed");
        assert!(objects.take_due(Moment::Reset, 9).is_empty(), "it waits for a return, not a reset");
        assert_eq!(objects.take_due(Moment::Return, 4), vec![(0x10, ())]);
        assert!(objects.take_due(Moment::Return, 5).is_empty(), "it was taken out");
    }

    #[test]
    fn one_already_waiting_keeps_the_time_it_began_to() {
        let mut objects = Objects::new();
        objects.wait(0x10, 'a', Moment::Return, 1);
        objects.wait(0x10, 'b', Moment::Return, 2);
        assert_eq!(objects.take_due(Moment::Return, 2), vec![(0x10, 'a')]);
    }

    #[test]
    fn the_due_come_in_the_order_they_began_to_wait() {
        let mut objects = Objects::new();
        for object in [0x50, 0x10, 0x40, 0x20, 0x30] {
            objects.wait(object, (), Moment::Reset, 0);
        }
        objects.wait(0x60, (), Moment::Reset, 1);
        let due: Vec<usize> = objects.take_due(Moment::Reset, 1).into_iter().map(|(o, _)| o).collect();
        assert_eq!(due, vec![0x50, 0x10, 0x40, 0x20, 0x30]);
    }

    #[test]
    fn what_waits_for_each_moment_is_due_apart() {
        let mut objects = Objects::new();
        objects.wait(0x10, 'r', Moment::Return, 0);
        objects.wait(0x20, 'p', Moment::Reset, 0);
        assert_eq!(objects.take_due(Moment::Reset, 1), vec![(0x20, 'p')]);
        assert_eq!(objects.take_due(Moment::Return, 1), vec![(0x10, 'r')]);
    }

    #[test]
    fn one_withdrawn_is_never_due() {
        let waiting: Waiting<()> = Waiting::new();
        waiting.wait(0x10, (), Moment::Return);
        waiting.wait(0x20, (), Moment::Return);
        waiting.withdraw(0x10);
        Moment::Return.passed();
        assert_eq!(waiting.take_due(Moment::Return), vec![(0x20, ())]);
    }
}
