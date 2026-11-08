//! Adding one set of modifiers to another, its lookups made through an
//! index (docs/research/late-game.md, "Adding sets of modifiers").
//!
//! `0x8ea420(set, added)` adds each (modifier, value) of `added` to `set`,
//! and `0x8ea730` takes them away. A set that keeps its values by modifier
//! (its byte `+0x10`) has them at the modifier's place; one kept as pairs
//! (its pairs at `+0x58`, their count at `+0x64`, 8 bytes each: the
//! modifier, then its value) is searched pair by pair for each modifier
//! added (`0x8ea4d0`, `0x8ea7e2`), so that adding costs the product of the
//! two sets' sizes; late in a game it is a large part of the monthly update.
//!
//! The framework replaces those searches alone: the first pair holding the
//! modifier, or none, which the game's code then adds to or appends after
//! as before. After a few searches in one call, the set's pairs are put in
//! an index of this thread's, which takes up the pairs the game appends as
//! the call goes on. Each answer is what the game's search gives, so the
//! sums, their order and the set's growth are the game's own; the developer
//! build's `check-modifier-sums` searches as the game does as well and
//! counts any answer that differs, and `game-sums-modifiers` leaves the
//! game's code as it is.
use crate::called_by_game;
use crate::points::{Function, Point};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// The two functions, `(set, added) -> set`.
const ADD: usize = 0x8ea420;
const TAKE_AWAY: usize = 0x8ea730;
/// Their searches: `movsxd rdx, [rbp + 0x64]` and `xor ecx, ecx` (in
/// `0x8ea420`) or `xor eax, eax` (in `0x8ea730`), `rbp` the set and `edi`
/// the modifier; the pair found is added to at `FOUND` with its place in
/// `rcx` or `rax`, a modifier not found appended at `NOT_FOUND`.
const ADD_SEARCH: usize = 0x8ea4d0;
const ADD_FOUND: usize = 0x8ea57e;
const ADD_NOT_FOUND: usize = 0x8ea4f4;
const TAKE_AWAY_SEARCH: usize = 0x8ea7e2;
const TAKE_AWAY_FOUND: usize = 0x8ea88e;
const TAKE_AWAY_NOT_FOUND: usize = 0x8ea805;
/// A set: whether it keeps its values by modifier, its pairs and their
/// count.
const BY_MODIFIER: usize = 0x10;
const PAIRS: usize = 0x58;
const COUNT: usize = 0x64;

/// Searches in a call made pair by pair before the set is put in an index,
/// and the least pairs a set has for one.
const SEARCHES_BEFORE_INDEX: u32 = 4;
const PAIRS_FOR_INDEX: i32 = 16;

type Sum = unsafe extern "C" fn(usize, usize) -> usize;
static ORIGINAL_ADD: Function<Sum> = Function::new();
static ORIGINAL_TAKE_AWAY: Function<Sum> = Function::new();

/// The first of `count` pairs at `pairs` that holds `modifier`, as the
/// game's search finds it.
unsafe fn search(pairs: usize, count: i32, modifier: i32) -> Option<u32> {
    (0..count.max(0) as u32).find(|&i| unsafe { *((pairs + 8 * i as usize) as *const i32) } == modifier)
}

/// Where each modifier's first pair is in one set, by open addressing.
#[derive(Default)]
struct Index {
    /// (stamp, modifier, place); a slot of another stamp is empty.
    slots: Vec<(u32, i32, u32)>,
    stamp: u32,
    /// How many of the set's pairs it holds.
    known: u32,
}

impl Index {
    fn slot(&self, modifier: i32) -> usize {
        (modifier as u32).wrapping_mul(0x9e37_79b9) as usize & (self.slots.len() - 1)
    }

    /// Empties it, with room for `pairs` pairs.
    fn clear(&mut self, pairs: u32) {
        let wanted = (pairs as usize * 2).next_power_of_two().max(64);
        if self.slots.len() < wanted {
            self.slots = vec![(0, 0, 0); wanted];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.slots.iter_mut().for_each(|s| s.0 = 0);
            self.stamp = 1;
        }
        self.known = 0;
    }

    /// Takes up the pairs from `known` to `count`, each modifier at its
    /// first place.
    unsafe fn take_up(&mut self, pairs: usize, count: i32) {
        let count = count.max(0) as u32;
        // Grown past half its room: made again, larger, from every pair.
        if (count as usize) * 2 > self.slots.len() {
            self.clear(count);
        }
        for place in self.known..count {
            let modifier = unsafe { *((pairs + 8 * place as usize) as *const i32) };
            let mut at = self.slot(modifier);
            loop {
                let (stamp, held, _) = self.slots[at];
                if stamp != self.stamp {
                    self.slots[at] = (self.stamp, modifier, place);
                    break;
                }
                if held == modifier {
                    break;
                }
                at = (at + 1) & (self.slots.len() - 1);
            }
        }
        self.known = count;
    }

    fn get(&self, modifier: i32) -> Option<u32> {
        let mut at = self.slot(modifier);
        loop {
            let (stamp, held, place) = self.slots[at];
            if stamp != self.stamp {
                return None;
            }
            if held == modifier {
                return Some(place);
            }
            at = (at + 1) & (self.slots.len() - 1);
        }
    }
}

thread_local! {
    /// The call of `0x8ea420` or `0x8ea730` this thread is in, counted.
    static CALL: Cell<u64> = const { Cell::new(0) };
    /// The call its index and searches are of, the searches made, whether
    /// the set is in the index.
    static STATE: RefCell<(u64, u32, bool, Index)> = RefCell::new((0, 0, false, Index::default()));
}

/// The first pair of `set` holding `modifier`, or -1, as the game's search
/// gives it.
unsafe fn find(set: usize, modifier: i32) -> isize {
    let (pairs, count) = unsafe { (*((set + PAIRS) as *const usize), *((set + COUNT) as *const i32)) };
    // A set by modifier writes modifiers into places below 0x286 as the
    // call goes on, which an index would not see.
    if unsafe { *((set + BY_MODIFIER) as *const u8) } != 0 || count < PAIRS_FOR_INDEX {
        return unsafe { search(pairs, count, modifier) }.map_or(-1, |p| p as isize);
    }
    let call = CALL.with(Cell::get);
    let found = STATE.with(|state| {
        let (of, searches, indexed, index) = &mut *state.borrow_mut();
        if *of != call {
            (*of, *searches, *indexed) = (call, 0, false);
        }
        *searches += 1;
        if !*indexed {
            if *searches <= SEARCHES_BEFORE_INDEX {
                return unsafe { search(pairs, count, modifier) };
            }
            index.clear(count as u32);
            *indexed = true;
        }
        // The pairs the game appended since are taken up first.
        unsafe { index.take_up(pairs, count) };
        index.get(modifier)
    });
    if checking() {
        note_check(found == unsafe { search(pairs, count, modifier) });
    }
    found.map_or(-1, |p| p as isize)
}

#[called_by_game]
unsafe extern "C" fn add(set: usize, added: usize) -> usize {
    CALL.with(|c| c.set(c.get() + 1));
    unsafe { ORIGINAL_ADD.get()(set, added) }
}

#[called_by_game]
unsafe extern "C" fn take_away(set: usize, added: usize) -> usize {
    CALL.with(|c| c.set(c.get() + 1));
    unsafe { ORIGINAL_TAKE_AWAY.get()(set, added) }
}

#[called_by_game]
extern "C" fn find_for_stub(set: usize, modifier: i32) -> isize {
    unsafe { find(set, modifier) }
}

/// A developer build's test switch `check-modifier-sums`: every search
/// made through the index is made as the game's too, and a difference
/// counted in the log.
fn checking() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.check_modifier_sums;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}

/// A developer build's test switch `game-sums-modifiers`: the searches are
/// the game's own code, the framework's points left out.
pub(crate) fn left_to_the_game() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.game_sums_modifiers;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}

static CHECKED: AtomicU64 = AtomicU64::new(0);
static DIFFERENT: AtomicU64 = AtomicU64::new(0);
static LOGGED: AtomicUsize = AtomicUsize::new(0);
const LOG_EVERY: u64 = 1_000_000;
const MOST_LOGGED: usize = 100;

fn note_check(same: bool) {
    if !same {
        DIFFERENT.fetch_add(1, Ordering::Relaxed);
    }
    let checked = CHECKED.fetch_add(1, Ordering::Relaxed) + 1;
    if checked.is_multiple_of(LOG_EVERY) && LOGGED.fetch_add(1, Ordering::Relaxed) < MOST_LOGGED {
        log!("modifier sums: {checked} searches through the index checked, {} differing from the game's", DIFFERENT.load(Ordering::Relaxed));
    }
}

crate::resume!(ADD_RESUME: ADD_FOUND_AT = ADD_FOUND, ADD_NOT_FOUND_AT = ADD_NOT_FOUND);
crate::resume!(TAKE_AWAY_RESUME: TAKE_AWAY_FOUND_AT = TAKE_AWAY_FOUND, TAKE_AWAY_NOT_FOUND_AT = TAKE_AWAY_NOT_FOUND);

/// Replaces the search of `0x8ea420`: the pair found in `rcx`.
#[unsafe(naked)]
extern "C" fn add_search_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, [rbp]", "mov edx, edi", call find_for_stub, "mov r11, [rsp + 0x48]"),
        "test r11, r11",
        "js 2f",
        "mov rcx, r11",
        "jmp qword ptr [rip + {found}]",
        "2:",
        "jmp qword ptr [rip + {not_found}]",
        find_for_stub = sym find_for_stub,
        found = sym ADD_FOUND_AT,
        not_found = sym ADD_NOT_FOUND_AT,
    );
}

/// Replaces the search of `0x8ea730`: the pair found in `rax`.
#[unsafe(naked)]
extern "C" fn take_away_search_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, [rbp]", "mov edx, edi", call find_for_stub, "mov r11, [rsp + 0x48]"),
        "test r11, r11",
        "js 2f",
        "mov rax, r11",
        "jmp qword ptr [rip + {found}]",
        "2:",
        "jmp qword ptr [rip + {not_found}]",
        find_for_stub = sym find_for_stub,
        found = sym TAKE_AWAY_FOUND_AT,
        not_found = sym TAKE_AWAY_NOT_FOUND_AT,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(ADD, "40 55 41 55 48 83 ec 48 4c 63 4a 64 45 33 c0 4c", "modifiers added to a set", add as Sum, &ORIGINAL_ADD),
        Point::replace(TAKE_AWAY, "40 55 41 55 48 83 ec 48 4c 63 4a 64 45 33 c0 4c", "modifiers taken from a set", take_away as Sum, &ORIGINAL_TAKE_AWAY),
        Point::jump(ADD_SEARCH, "48 63 55 64 33 c9", "set's search for a modifier added", add_search_stub as *const () as usize, &ADD_RESUME).uses(&[COUNT]),
        Point::keep(ADD_FOUND, "48 8b 45 58 01 5c c8 04", "set's pair found, added to").uses(&[PAIRS]),
        Point::keep(ADD_NOT_FOUND, "8b 45 64 8b 4d 60 ff c0 3b c1", "set's pair appended").uses(&[COUNT]),
        Point::keep(ADD_SEARCH + 6, "48 85 d2 7e 19 48 8b 45 58 90 39 38 0f 84", "set's search pair by pair").uses(&[PAIRS]),
        Point::jump(TAKE_AWAY_SEARCH, "48 63 55 64 33 c0", "set's search for a modifier taken", take_away_search_stub as *const () as usize, &TAKE_AWAY_RESUME)
            .uses(&[COUNT]),
        Point::keep(TAKE_AWAY_FOUND, "48 8b 4d 58 01 5c c1 04", "set's pair found, taken from").uses(&[PAIRS]),
        Point::keep(TAKE_AWAY_NOT_FOUND, "8b 45 64 8b 4d 60 ff c0 3b c1", "set's pair appended, taken from"),
        Point::keep(0x8ea4a6usize, "80 7d 10 00 74 24", "set kept by modifier, added to").uses(&[BY_MODIFIER]),
        Point::keep(0x8ea7b8usize, "80 7d 10 00 74 24", "set kept by modifier, taken from").uses(&[BY_MODIFIER]),
        Point::keep(TAKE_AWAY_SEARCH + 6, "48 85 d2 7e 18 48 8b 4d 58 39 39 0f 84", "set's search pair by pair, taken from").uses(&[PAIRS]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pairs as the game lays them out.
    fn pairs(modifiers: &[i32]) -> Vec<i32> {
        modifiers.iter().flat_map(|&m| [m, 1]).collect()
    }

    #[test]
    fn the_index_finds_the_first_pair_of_each_modifier() {
        let held = pairs(&[7, 3, 7, 900, 3, 12]);
        let at = held.as_ptr() as usize;
        let mut index = Index::default();
        index.clear(6);
        unsafe { index.take_up(at, 6) };
        for m in [7, 3, 900, 12, 5, 0x286] {
            assert_eq!(index.get(m), unsafe { search(at, 6, m) }, "modifier {m}");
        }
    }

    #[test]
    fn pairs_appended_are_taken_up_where_they_are() {
        let mut held = pairs(&[1, 2, 3]);
        let mut index = Index::default();
        index.clear(3);
        unsafe { index.take_up(held.as_ptr() as usize, 3) };
        held.extend(pairs(&[40, 2, 41]));
        unsafe { index.take_up(held.as_ptr() as usize, 6) };
        assert_eq!(index.get(40), Some(3));
        assert_eq!(index.get(2), Some(1), "the first place of a modifier held twice");
        assert_eq!(index.get(41), Some(5));
        // Grown past its room, it is made again from the pairs.
        let many: Vec<i32> = (100..400).collect();
        held.extend(pairs(&many));
        let count = held.len() as i32 / 2;
        unsafe { index.take_up(held.as_ptr() as usize, count) };
        for m in [1, 2, 3, 40, 41, 100, 399] {
            assert_eq!(index.get(m), unsafe { search(held.as_ptr() as usize, count, m) }, "modifier {m}");
        }
    }
}
