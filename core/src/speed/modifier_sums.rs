//! Adding or subtracting one set of modifiers from another (`0x8ea420` /
//! `0x8ea730`, `docs/game/countries/modifiers.md`).
//!
//! A modifier set keeps its values either indexed by modifier (`+0x10 != 0`,
//! for built-in modifiers below `0x286`) or as a packed array of 8-byte
//! `(modifier: i32, value: i32)` pairs (`+0x58`, capacity `+0x60`, count
//! `+0x64`). The game searches a pair-mode set linearly for every entry in
//! `added`, making large merges quadratic.
//!
//! Replacing `0x8ea420` and `0x8ea730` whole-function removes per-pair hook
//! round-trips and uses a per-thread O(1) stamped direct-mapped table for
//! built-in modifiers (`0..0x287`, with a small open-addressing fallback for
//! any modifier outside that range) whenever a pair-mode set is large enough
//! to benefit (`count >= 16` and `added.count > 4`). Small merges run in a
//! tight inline linear scan without touching TLS. Pair growth and the
//! source-set vector at `+0x38` use the game's own allocator and helpers, so
//! sums, ordering, and heap ownership match the game bit for bit.
use crate::base::called_by_game;
use crate::native::game_heap;
use crate::native::objects::campaign::modifier_set::{BY_MODIFIER, COUNT, PAIRS};
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;

/// The two functions, `(set, added) -> set`.
const ADD: usize = 0x8ea420;
const TAKE_AWAY: usize = 0x8ea730;
/// `std::vector<CModifier*>::_Emplace_reallocate(vec, end, &added)` for `set + 0x38`.
const ADDED_VEC_PUSH: usize = 0x8f0e0;

const ADDED_VEC: usize = 0x38;
const ADDED_END: usize = 0x40;
const ADDED_CAP: usize = 0x48;
const PAIRS_OWNER: usize = 0x50;
const CAP: usize = 0x60;

/// Sentinel stored in empty slots of a by-modifier set, skipped during merges.
const EMPTY_MODIFIER: i32 = 0x286;
/// Number of built-in modifier indices (`0..0x287`).
const BUILTIN_MODIFIERS: usize = 0x287;
/// Minimum destination pair count and incoming count before indexing pays off.
const PAIRS_FOR_INDEX: usize = 16;
const ADDED_FOR_INDEX: usize = 5;

#[repr(C)]
#[derive(Clone, Copy)]
struct Pair {
    modifier: i32,
    value: i32,
}

type Sum = unsafe extern "C" fn(usize, usize) -> usize;
type VecPush = unsafe extern "C" fn(usize, usize, *const usize);
type FreePairs = unsafe extern "C" fn(usize);

static ORIGINAL_ADD: Function<Sum> = Function::new();
static ORIGINAL_TAKE_AWAY: Function<Sum> = Function::new();

/// Linear search matching the game's loop: first pair holding `modifier`.
#[inline]
unsafe fn linear_search(pairs: *const Pair, count: usize, modifier: i32) -> Option<usize> {
    let mut i = 0;
    while i < count {
        if unsafe { (*pairs.add(i)).modifier } == modifier {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Fast per-thread index for pair-mode modifier sets: O(1) direct-mapped
/// stamped array for built-in modifiers (`0..0x287`), plus a small
/// open-addressing fallback table for any modifier outside that range.
struct FastIndex {
    /// `(stamp, place)` for `0 <= modifier < BUILTIN_MODIFIERS`.
    direct: Box<[(u32, u32); BUILTIN_MODIFIERS]>,
    /// `(stamp, modifier, place)` for modifiers outside `0..BUILTIN_MODIFIERS`.
    overflow: Vec<(u32, i32, u32)>,
    overflow_used: usize,
    stamp: u32,
}

impl Default for FastIndex {
    fn default() -> Self {
        Self { direct: Box::new([(0, 0); BUILTIN_MODIFIERS]), overflow: Vec::new(), overflow_used: 0, stamp: 0 }
    }
}

impl FastIndex {
    #[inline]
    fn clear(&mut self) {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.direct.fill((0, 0));
            for slot in &mut self.overflow {
                slot.0 = 0;
            }
            self.stamp = 1;
        }
        self.overflow_used = 0;
    }

    #[inline]
    fn overflow_slot(len: usize, modifier: i32) -> usize {
        (modifier as u32).wrapping_mul(0x9e37_79b9) as usize & (len - 1)
    }

    fn grow_overflow(&mut self, min_entries: usize) {
        let wanted = (min_entries * 2).next_power_of_two().max(32);
        let old = std::mem::replace(&mut self.overflow, vec![(0, 0, 0); wanted]);
        let stamp = self.stamp;
        for (s, modifier, place) in old {
            if s != stamp {
                continue;
            }
            let mut at = Self::overflow_slot(wanted, modifier);
            while self.overflow[at].0 == stamp {
                at = (at + 1) & (wanted - 1);
            }
            self.overflow[at] = (stamp, modifier, place);
        }
    }

    #[inline]
    fn insert_if_absent(&mut self, modifier: i32, place: usize) {
        let stamp = self.stamp;
        if (0..BUILTIN_MODIFIERS as i32).contains(&modifier) {
            let slot = &mut self.direct[modifier as usize];
            if slot.0 != stamp {
                *slot = (stamp, place as u32);
            }
            return;
        }
        if (self.overflow_used + 1) * 2 > self.overflow.len() {
            self.grow_overflow(self.overflow_used + 1);
        }
        let len = self.overflow.len();
        let mut at = Self::overflow_slot(len, modifier);
        loop {
            let (s, held, _) = self.overflow[at];
            if s != stamp {
                self.overflow[at] = (stamp, modifier, place as u32);
                self.overflow_used += 1;
                return;
            }
            if held == modifier {
                return;
            }
            at = (at + 1) & (len - 1);
        }
    }

    #[inline]
    fn populate(&mut self, pairs: *const Pair, count: usize) {
        self.clear();
        let mut i = 0;
        while i < count {
            let modifier = unsafe { (*pairs.add(i)).modifier };
            self.insert_if_absent(modifier, i);
            i += 1;
        }
    }

    #[inline]
    fn get(&self, modifier: i32) -> Option<usize> {
        let stamp = self.stamp;
        if (0..BUILTIN_MODIFIERS as i32).contains(&modifier) {
            let (s, place) = self.direct[modifier as usize];
            return (s == stamp).then_some(place as usize);
        }
        if self.overflow_used == 0 {
            return None;
        }
        let len = self.overflow.len();
        let mut at = Self::overflow_slot(len, modifier);
        loop {
            let (s, held, place) = self.overflow[at];
            if s != stamp {
                return None;
            }
            if held == modifier {
                return Some(place as usize);
            }
            at = (at + 1) & (len - 1);
        }
    }
}

thread_local! {
    static INDEX: RefCell<FastIndex> = RefCell::new(FastIndex::default());
}

/// Grows `set`'s pair array using the game's allocator (`0x1a332d4`) and
/// releases the old storage through `(set + 0x50)`'s vtable slot `+0x10`,
/// matching `0x8ea504..0x8ea57c` identically.
#[cold]
#[inline(never)]
unsafe fn grow_pairs(set: usize, needed: i32) -> *mut Pair {
    let cap = unsafe { *((set + CAP) as *const i32) };
    let mut new_cap = cap.wrapping_mul(2);
    if new_cap < needed {
        new_cap = needed;
    }
    let bytes = (new_cap as u32 as usize).saturating_mul(std::mem::size_of::<Pair>());
    let m = crate::native::hooked_memory();
    let op_new: unsafe extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(m.at(game_heap::NEW.rva())) };
    let new_ptr = unsafe { op_new(bytes) } as *mut Pair;
    let count = unsafe { *((set + COUNT) as *const i32) };
    if count > 0 {
        let old_ptr = unsafe { *((set + PAIRS) as *const *const Pair) };
        unsafe { std::ptr::copy_nonoverlapping(old_ptr, new_ptr, count as usize) };
    }
    let owner = set + PAIRS_OWNER;
    let vtable = unsafe { *(owner as *const usize) };
    let free_fn: FreePairs = unsafe { std::mem::transmute(*((vtable + 0x10) as *const usize)) };
    unsafe { free_fn(owner) };
    unsafe {
        *((set + PAIRS) as *mut usize) = new_ptr as usize;
        *((set + CAP) as *mut i32) = new_cap;
    }
    new_ptr
}

#[inline]
unsafe fn append_pair(set: usize, set_pairs: &mut *mut Pair, set_count: &mut usize, set_cap: &mut i32, modifier: i32, value: i32) -> usize {
    let needed = (*set_count as i32).wrapping_add(1);
    if needed > *set_cap {
        *set_pairs = unsafe { grow_pairs(set, needed) };
        *set_cap = unsafe { *((set + CAP) as *const i32) };
    }
    let place = *set_count;
    unsafe {
        *(*set_pairs).add(place) = Pair { modifier, value };
        *((set + COUNT) as *mut i32) = needed;
    }
    *set_count = place + 1;
    place
}

#[inline]
unsafe fn push_added_source(set: usize, added: usize) {
    let end = unsafe { *((set + ADDED_END) as *const usize) };
    let cap = unsafe { *((set + ADDED_CAP) as *const usize) };
    if end != cap {
        unsafe {
            *(end as *mut usize) = added;
            *((set + ADDED_END) as *mut usize) = end + 8;
        }
    } else {
        let m = crate::native::hooked_memory();
        let push: VecPush = unsafe { std::mem::transmute(m.at(ADDED_VEC_PUSH)) };
        unsafe { push(set + ADDED_VEC, end, &added) };
    }
}

unsafe fn sum_sets(set: usize, added: usize, subtract: bool) -> usize {
    let added_count = unsafe { *((added + COUNT) as *const i32) };
    if added_count > 0 {
        let added_count_usize = added_count as usize;
        let added_by_mod = unsafe { *((added + BY_MODIFIER) as *const u8) } != 0;
        let set_by_mod = unsafe { *((set + BY_MODIFIER) as *const u8) } != 0;
        let aliased = set == added;
        let checked = speed::checked(&SPEED_UP);

        let mut set_pairs = unsafe { *((set + PAIRS) as *const *mut Pair) };
        let mut set_count = unsafe { *((set + COUNT) as *const i32) }.max(0) as usize;
        let mut set_cap = unsafe { *((set + CAP) as *const i32) };

        if !set_by_mod && set_count >= PAIRS_FOR_INDEX && added_count_usize >= ADDED_FOR_INDEX {
            INDEX.with(|cell| {
                let mut index = cell.borrow_mut();
                index.populate(set_pairs, set_count);
                for i in 0..added_count_usize {
                    let added_pairs = if aliased { set_pairs as *const Pair } else { unsafe { *((added + PAIRS) as *const *const Pair) } };
                    let entry = unsafe { *added_pairs.add(i) };
                    let modifier = entry.modifier;
                    if modifier == EMPTY_MODIFIER {
                        continue;
                    }
                    let raw = if added_by_mod && modifier <= EMPTY_MODIFIER { unsafe { (*added_pairs.offset(modifier as isize)).value } } else { entry.value };
                    let val = if subtract { raw.wrapping_neg() } else { raw };
                    let found = index.get(modifier);
                    if checked {
                        let game = unsafe { linear_search(set_pairs, set_count, modifier) };
                        speed::compared(&SPEED_UP, found == game, || {
                            format!("modifier {modifier}: the index found pair {found:?}, the game's search {game:?}")
                        });
                    }
                    if let Some(place) = found {
                        let dst = unsafe { &mut (*set_pairs.add(place)).value };
                        *dst = dst.wrapping_add(val);
                    } else {
                        let place = unsafe { append_pair(set, &mut set_pairs, &mut set_count, &mut set_cap, modifier, val) };
                        index.insert_if_absent(modifier, place);
                    }
                }
            });
        } else {
            let mut added_pairs = unsafe { *((added + PAIRS) as *const *const Pair) };
            for i in 0..added_count_usize {
                if aliased {
                    added_pairs = set_pairs as *const Pair;
                }
                let entry = unsafe { *added_pairs.add(i) };
                let modifier = entry.modifier;
                if modifier == EMPTY_MODIFIER {
                    continue;
                }
                let raw = if added_by_mod && modifier <= EMPTY_MODIFIER { unsafe { (*added_pairs.offset(modifier as isize)).value } } else { entry.value };
                let val = if subtract { raw.wrapping_neg() } else { raw };
                if set_by_mod && modifier < EMPTY_MODIFIER {
                    let slot = unsafe { &mut *set_pairs.offset(modifier as isize) };
                    slot.modifier = modifier;
                    slot.value = slot.value.wrapping_add(val);
                } else if let Some(place) = unsafe { linear_search(set_pairs, set_count, modifier) } {
                    let dst = unsafe { &mut (*set_pairs.add(place)).value };
                    *dst = dst.wrapping_add(val);
                } else {
                    unsafe { append_pair(set, &mut set_pairs, &mut set_count, &mut set_cap, modifier, val) };
                }
            }
        }
    }
    unsafe { push_added_source(set, added) };
    set
}

#[called_by_game]
unsafe extern "C" fn add(set: usize, added: usize) -> usize {
    unsafe { sum_sets(set, added, false) }
}

#[called_by_game]
unsafe extern "C" fn take_away(set: usize, added: usize) -> usize {
    unsafe { sum_sets(set, added, true) }
}

/// Checked, every search made through the index is made as the game's too.
pub const SPEED_UP: SpeedUp = SpeedUp::new("modifier-sums", "frames", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(ADD, "40 55 41 55 48 83 ec 48 4c 63 4a 64 45 33 c0 4c", "modifiers added to a set", add as Sum, &ORIGINAL_ADD).uses(&[COUNT]),
        Point::replace(TAKE_AWAY, "40 55 41 55 48 83 ec 48 4c 63 4a 64 45 33 c0 4c", "modifiers taken from a set", take_away as Sum, &ORIGINAL_TAKE_AWAY)
            .uses(&[COUNT]),
        Point::keep(ADDED_VEC_PUSH, "48 89 5c 24 20 55 56 41 54 48 83 ec 20 48 8b 41 08", "modifier set's source vector push"),
        Point::keep(0x8ea4a6, "80 7d 10 00 74 24", "set kept by modifier, added to").uses(&[BY_MODIFIER]),
        Point::keep(0x8ea4f4, "8b 45 64 8b 4d 60 ff c0 3b c1", "set's pair capacity check").uses(&[COUNT, CAP]),
        Point::keep(0x8ea558, "48 8b 45 50 48 8d 4d 50 ff 50 10", "set's old pairs released").uses(&[PAIRS_OWNER]),
        Point::keep(0x8ea5de, "48 3b 55 48", "set's source vector capacity").uses(&[ADDED_CAP]),
        Point::keep(0x8ea7b8, "80 7d 10 00 74 24", "set kept by modifier, taken from").uses(&[BY_MODIFIER]),
        Point::keep(0x8ea57e, "48 8b 45 58 01 5c c8 04", "set's pair found, added to").uses(&[PAIRS]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(modifiers: &[i32]) -> Vec<Pair> {
        modifiers.iter().map(|&modifier| Pair { modifier, value: 1 }).collect()
    }

    #[test]
    fn fast_index_finds_first_pair_for_builtin_and_overflow_modifiers() {
        let held = pairs(&[7, 3, 7, 900, 3, 12, -5, 900]);
        let mut index = FastIndex::default();
        index.populate(held.as_ptr(), held.len());
        for m in [7, 3, 900, 12, -5, 5, EMPTY_MODIFIER, 1200] {
            assert_eq!(index.get(m), unsafe { linear_search(held.as_ptr(), held.len(), m) }, "modifier {m}");
        }
    }

    #[test]
    fn fast_index_tracks_appended_pairs_and_resets_cleanly() {
        let mut held = pairs(&[1, 2, 3]);
        let mut index = FastIndex::default();
        index.populate(held.as_ptr(), held.len());
        for m in [40, 2, 41, 700, 701, 700] {
            let place = held.len();
            held.push(Pair { modifier: m, value: 1 });
            index.insert_if_absent(m, place);
        }
        assert_eq!(index.get(40), Some(3));
        assert_eq!(index.get(2), Some(1), "keeps first place of duplicate modifier");
        assert_eq!(index.get(700), Some(6));
        assert_eq!(index.get(701), Some(7));

        // Populate again with a disjoint set to verify O(1) stamp clearing.
        let next = pairs(&[10, 800]);
        index.populate(next.as_ptr(), next.len());
        assert_eq!(index.get(40), None);
        assert_eq!(index.get(700), None);
        assert_eq!(index.get(10), Some(0));
        assert_eq!(index.get(800), Some(1));
    }
}
