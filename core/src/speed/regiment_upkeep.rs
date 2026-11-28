//! Filtered country-modifier merge in unit regiment upkeep (`0xd54600`,
//! `docs/game/countries/modifiers.md`).
//!
//! For every regiment in a unit, `0xd54600` constructs a stack-allocated
//! pair-mode `CModifier` (inline capacity 8 pairs), merges the regiment's drill
//! and unit-type modifiers, merges the entire 646-slot country modifier set at
//! `0xd546f7` (`call 0x8ea420`), optionally merges mercenary company modifiers,
//! calls `0xaebeb0(regiment, &local_108, 0)`, and immediately destroys
//! `local_108`.
//!
//! Reading `0xaebeb0` to its end shows that with `param_3 == 0` (no tooltip),
//! it only ever queries 8 modifier IDs: `0x15b`, `0x15c`, `0x1fa`, `0x201`,
//! `0x202`, `0x203`, `0x204`, and `0x23f`. Merging only those 8 IDs from the
//! country's `by_modifier` set (when none of their `modifier_type` records has
//! a custom `+0x40` transform callback) avoids scanning 646 slots and overflowing
//! the 8-pair stack buffer onto the heap for every regiment.
//!
//! In checked mode (`speed::checked`), `0xd5474c` evaluates `0xaebeb0` before
//! and after merging the remaining 638 country modifiers into `local_108` and
//! verifies that every regiment's upkeep matches the game's answer.
use super::modifier_sums::{self, append_pair, linear_search, Pair, CAP, EMPTY_MODIFIER};
use crate::base::called_by_game;
use crate::native::objects::campaign::{
    modifier_set::{BY_MODIFIER, COUNT, PAIRS},
    modifier_type::{SIZE as MODIFIER_TYPE_SIZE, TYPES as MODIFIER_TYPES},
};
use crate::native::points::Point;
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::Cell;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// `lea rdx, [rax+8]; lea rcx, [rsp+0x60]; call 0x8ea420` inside `0xd54600`.
const UPKEEP_COUNTRY_MERGE: usize = 0xd546f7;
const UPKEEP_COUNTRY_MERGE_DONE: usize = 0xd54705;

/// `xor r8d, r8d; lea rdx, [rsp+0x60]; mov rcx, rbx; call 0xaebeb0` inside `0xd54600`.
const UPKEEP_CALC_CALL: usize = 0xd5474c;
const UPKEEP_CALC_DONE: usize = 0xd5475c;

/// Regiment upkeep computation (`0xaebeb0(regiment, modifier_set, tooltip_out)`).
const REGIMENT_UPKEEP_FN: usize = 0xaebeb0;

/// The 8 modifier IDs read by `0xaebeb0` when `param_3 == 0`.
const UPKEEP_MODIFIERS: [i32; 8] = [0x15b, 0x15c, 0x1fa, 0x201, 0x202, 0x203, 0x204, 0x23f];
/// Offset of the optional value-transform object pointer inside a `modifier_type` record.
const MODIFIER_TRANSFORM_OFFSET: usize = 0x40;

resume!(MERGE_RESUME: UPKEEP_COUNTRY_MERGE_DONE_AT = UPKEEP_COUNTRY_MERGE_DONE);
resume!(CALC_RESUME: UPKEEP_CALC_DONE_AT = UPKEEP_CALC_DONE);
static UPKEEP_FN_AT: AtomicUsize = AtomicUsize::new(0);
/// Non-zero once checked mode is observed so the unchecked path skips `call_rust!`.
static CHECKED_FLAG: AtomicU8 = AtomicU8::new(0);

thread_local! {
    /// Country modifier set that was filtered for the current regiment when `checked` is active.
    static PENDING_CHECK_COUNTRY_MODS: Cell<usize> = const { Cell::new(0) };
}

#[inline]
fn is_upkeep_modifier(id: i32) -> bool {
    UPKEEP_MODIFIERS.contains(&id)
}

/// Returns true when every one of the 8 upkeep modifier definitions has a null
/// `+0x40` transform object, guaranteeing `0xaebeb0` cannot read any other
/// modifier ID from `param_2`.
#[inline]
unsafe fn upkeep_transforms_absent() -> bool {
    let m = crate::native::hooked_memory();
    let base = unsafe { *(m.at(MODIFIER_TYPES) as *const usize) };
    if base == 0 {
        return false;
    }
    for &id in &UPKEEP_MODIFIERS {
        let transform = unsafe { *((base + (id as usize) * MODIFIER_TYPE_SIZE + MODIFIER_TRANSFORM_OFFSET) as *const usize) };
        if transform != 0 {
            return false;
        }
    }
    true
}

/// Merges only `UPKEEP_MODIFIERS` from `added_pairs` (a `by_modifier` array of
/// at least `0x240` slots) into pair-mode `set`.
unsafe fn merge_upkeep_modifiers(set: usize, set_pairs: &mut *mut Pair, set_count: &mut usize, set_cap: &mut i32, added_pairs: *const Pair) {
    for &id in &UPKEEP_MODIFIERS {
        let entry = unsafe { *added_pairs.add(id as usize) };
        if entry.modifier == EMPTY_MODIFIER {
            continue;
        }
        let val = if entry.modifier == id {
            entry.value
        } else if (0..EMPTY_MODIFIER).contains(&entry.modifier) {
            unsafe { (*added_pairs.offset(entry.modifier as isize)).value }
        } else {
            entry.value
        };
        if val == 0 {
            continue;
        }
        if let Some(place) = unsafe { linear_search(*set_pairs, *set_count, entry.modifier) } {
            let dst = unsafe { &mut (*(*set_pairs).add(place)).value };
            *dst = dst.wrapping_add(val);
        } else {
            unsafe { append_pair(set, set_pairs, set_count, set_cap, entry.modifier, val) };
        }
    }
}

#[called_by_game]
unsafe extern "C" fn filter_country_modifiers(set: usize, country_mods: usize) {
    let checked = speed::checked(&SPEED_UP);
    if checked {
        CHECKED_FLAG.store(1, Ordering::Relaxed);
    }
    if set == 0 || country_mods == 0 || set == country_mods {
        PENDING_CHECK_COUNTRY_MODS.with(|c| c.set(0));
        unsafe { modifier_sums::sum_sets(set, country_mods, false) };
        return;
    }
    let set_by_mod = unsafe { *((set + BY_MODIFIER) as *const u8) } != 0;
    let added_by_mod = unsafe { *((country_mods + BY_MODIFIER) as *const u8) } != 0;
    let added_count = unsafe { *((country_mods + COUNT) as *const i32) };
    if set_by_mod || !added_by_mod || added_count <= 0x23f || !unsafe { upkeep_transforms_absent() } {
        PENDING_CHECK_COUNTRY_MODS.with(|c| c.set(0));
        unsafe { modifier_sums::sum_sets(set, country_mods, false) };
        return;
    }

    let mut set_pairs = unsafe { *((set + PAIRS) as *const *mut Pair) };
    let mut set_count = unsafe { *((set + COUNT) as *const i32) }.max(0) as usize;
    let mut set_cap = unsafe { *((set + CAP) as *const i32) };
    let added_pairs = unsafe { *((country_mods + PAIRS) as *const *const Pair) };
    if added_pairs.is_null() {
        PENDING_CHECK_COUNTRY_MODS.with(|c| c.set(0));
        unsafe { modifier_sums::sum_sets(set, country_mods, false) };
        return;
    }

    unsafe { merge_upkeep_modifiers(set, &mut set_pairs, &mut set_count, &mut set_cap, added_pairs) };
    PENDING_CHECK_COUNTRY_MODS.with(|c| c.set(if checked { country_mods } else { 0 }));
}

#[called_by_game]
unsafe extern "C" fn checked_upkeep_calc(regiment: usize, set: usize) -> i32 {
    type UpkeepFn = unsafe extern "C" fn(usize, usize, usize) -> i32;
    let upkeep: UpkeepFn = unsafe { std::mem::transmute(UPKEEP_FN_AT.load(Ordering::Acquire)) };
    let fast = unsafe { upkeep(regiment, set, 0) };
    let country_mods = PENDING_CHECK_COUNTRY_MODS.with(|c| c.replace(0));
    if country_mods != 0 && speed::checked(&SPEED_UP) {
        // Merge every non-upkeep country modifier into `set` and re-evaluate
        // `0xaebeb0` to verify that omitting them cannot change the answer.
        let added_count = unsafe { *((country_mods + COUNT) as *const i32) }.max(0) as usize;
        let added_pairs = unsafe { *((country_mods + PAIRS) as *const *const Pair) };
        if added_pairs.is_null() {
            return fast;
        }
        let mut set_pairs = unsafe { *((set + PAIRS) as *const *mut Pair) };
        let mut set_count = unsafe { *((set + COUNT) as *const i32) }.max(0) as usize;
        let mut set_cap = unsafe { *((set + CAP) as *const i32) };
        for i in 0..added_count {
            if is_upkeep_modifier(i as i32) {
                continue;
            }
            let entry = unsafe { *added_pairs.add(i) };
            let modifier = entry.modifier;
            if modifier == EMPTY_MODIFIER {
                continue;
            }
            let val = if modifier == i as i32 {
                entry.value
            } else if (0..EMPTY_MODIFIER).contains(&modifier) && (modifier as usize) < added_count {
                unsafe { (*added_pairs.offset(modifier as isize)).value }
            } else {
                entry.value
            };
            if let Some(place) = unsafe { linear_search(set_pairs, set_count, modifier) } {
                let dst = unsafe { &mut (*set_pairs.add(place)).value };
                *dst = dst.wrapping_add(val);
            } else {
                unsafe { append_pair(set, &mut set_pairs, &mut set_count, &mut set_cap, modifier, val) };
            }
        }
        let game = unsafe { upkeep(regiment, set, 0) };
        speed::compared(&SPEED_UP, fast == game, || format!("regiment upkeep for {regiment:#x}: filtered {fast}, full merge {game}"));
    }
    fast
}

#[unsafe(naked)]
extern "C" fn filter_country_modifiers_stub() {
    std::arch::naked_asm!(
        // `call_rust!` saves game `rbp` at `[rbp]` and game `rsp` at `rbp + 8`,
        // so `local_108` at `[game_rsp + 0x60]` is `[rbp + 0x68]`.
        call_rust!(
            "lea rcx, [rbp + 0x68]",
            "lea rdx, [rax + 8]",
            call filter_country_modifiers
        ),
        "jmp qword ptr [rip + {done}]",
        filter_country_modifiers = sym filter_country_modifiers,
        done = sym UPKEEP_COUNTRY_MERGE_DONE_AT,
    );
}

#[unsafe(naked)]
extern "C" fn upkeep_calc_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rip + {checked}], 0",
        "jne 2f",
        "xor r8d, r8d",
        "lea rdx, [rsp + 0x60]",
        "mov rcx, rbx",
        "call qword ptr [rip + {upkeep_fn}]",
        "jmp qword ptr [rip + {done}]",
        "2:",
        call_rust!(
            "mov rcx, rbx",
            "lea rdx, [rbp + 0x68]",
            call checked_upkeep_calc -> eax,
        ),
        "jmp qword ptr [rip + {done}]",
        checked = sym CHECKED_FLAG,
        upkeep_fn = sym UPKEEP_FN_AT,
        checked_upkeep_calc = sym checked_upkeep_calc,
        done = sym UPKEEP_CALC_DONE_AT,
    );
}

/// Checked, every regiment's upkeep is verified against a full country-modifier merge.
pub const SPEED_UP: SpeedUp = SpeedUp::new("regiment-upkeep", "frames", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::jump(
            UPKEEP_COUNTRY_MERGE,
            "48 8d 50 08 48 8d 4c 24 60 e8 1b 5d b9 ff",
            "unit upkeep merges country modifiers into regiment modifier set",
            filter_country_modifiers_stub as *const () as usize,
            &MERGE_RESUME,
        ),
        Point::keep(UPKEEP_COUNTRY_MERGE_DONE, "48 8b 4b 40 48 85", "unit upkeep after country modifier merge"),
        Point::jump(
            UPKEEP_CALC_CALL,
            "45 33 c0 48 8d 54 24 60 48 8b cb e8 54 77 d9 ff",
            "unit upkeep calls regiment upkeep without tooltip",
            upkeep_calc_stub as *const () as usize,
            &CALC_RESUME,
        ),
        Point::keep(UPKEEP_CALC_DONE, "44 03 e0", "unit upkeep accumulates regiment upkeep"),
        Point::keep(REGIMENT_UPKEEP_FN, "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8b", "regiment upkeep computation").recorded(&UPKEEP_FN_AT),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn capture_args(local_set: usize, country_mods: usize) -> usize {
        local_set ^ country_mods.rotate_left(17)
    }

    #[unsafe(naked)]
    extern "C" fn stack_and_rax_probe(_game_rax: usize) -> usize {
        std::arch::naked_asm!(
            "sub rsp, 0x80",
            "mov rax, rcx",
            // Save expected `[game_rsp + 0x60]` in r10 before `call_rust!`.
            "lea r10, [rsp + 0x60]",
            call_rust!(
                "lea rcx, [rbp + 0x68]",
                "lea rdx, [rax + 8]",
                call capture_args -> rax,
            ),
            // Compute the same hash from `r10` and original `game_rax + 8` via caller verification:
            // store `(actual - expected)` in rax by returning `actual ^ expected`? Even simpler:
            // return `actual` vs `r10` difference in another probe. Here compare `rax` directly
            // by packing `actual == (r10 ^ (game_rax+8).rotate_left(17))`.
            "lea r11, [rcx + 8]",
            "rol r11, 17",
            "xor r10, r11",
            "xor rax, r10",
            "add rsp, 0x80",
            "ret",
            capture_args = sym capture_args,
        );
    }

    #[test]
    fn stub_reads_game_rsp_plus_0x60_and_rax_plus_8_through_call_rust() {
        assert_eq!(stack_and_rax_probe(0x1234_5678_0000), 0);
    }

    #[test]
    fn merge_upkeep_modifiers_copies_only_the_eight_upkeep_ids_and_preserves_existing_pairs() {
        let mut country = vec![Pair { modifier: EMPTY_MODIFIER, value: 0 }; 0x287];
        country[0x10] = Pair { modifier: 0x10, value: 9999 };
        country[0x15b] = Pair { modifier: 0x15b, value: -150 };
        country[0x1fa] = Pair { modifier: 0x1fa, value: 200 };
        country[0x201] = Pair { modifier: 0x201, value: 0 };
        country[0x23f] = Pair { modifier: 0x23f, value: 500 };
        country[0x250] = Pair { modifier: 0x250, value: 7777 };

        let mut dst_buf = [Pair { modifier: 0, value: 0 }; 8];
        dst_buf[0] = Pair { modifier: 0x15b, value: 50 };
        dst_buf[1] = Pair { modifier: 0x42, value: 12 };

        let mut set_storage = [0u8; 0x80];
        let set_addr = set_storage.as_mut_ptr() as usize;
        unsafe {
            *((set_addr + COUNT) as *mut i32) = 2;
        }
        let mut set_pairs = dst_buf.as_mut_ptr();
        let mut set_count = 2usize;
        let mut set_cap = 8i32;

        unsafe {
            merge_upkeep_modifiers(set_addr, &mut set_pairs, &mut set_count, &mut set_cap, country.as_ptr());
        }

        assert_eq!(set_count, 4);
        assert_eq!(dst_buf[0].modifier, 0x15b);
        assert_eq!(dst_buf[0].value, -100, "added onto existing regiment pair");
        assert_eq!(dst_buf[1].modifier, 0x42);
        assert_eq!(dst_buf[1].value, 12);
        assert_eq!(dst_buf[2].modifier, 0x1fa);
        assert_eq!(dst_buf[2].value, 200);
        assert_eq!(dst_buf[3].modifier, 0x23f);
        assert_eq!(dst_buf[3].value, 500);
    }
}
