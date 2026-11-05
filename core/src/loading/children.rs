//! A save's dated nodes added to their parent without walking its list
//! (docs/loading.md).
//!
//! A node of the game's (the method `+0x98` at `0x7e7cd0`, which many classes
//! share) adds a child to its list (`+0x20`) after walking the whole list for
//! the child already there; reading a save, `0xda66a0` makes each child of
//! such a node, reads it, and adds it so, which walks the parent's list once
//! for every child: 24% of a large save's load (docs/research/loading.md).
//!
//! The child `0xda66a0` adds is one it has just made, in no list yet, so the
//! walk cannot find it. Where `0xda66a0` calls the add, the framework marks
//! the thread; the add, seeing the mark on its thread, clears it and goes on
//! past the walk, as it does when the walk ends without finding the child.
//! Any other add, the adds the add itself makes and every other thread's
//! walk the list as before.
use crate::points::Point;
use std::sync::atomic::AtomicU32;

/// In `0xda66a0`: `call qword ptr [rax + 0x98]`, the parent's add of the
/// child it made (`rdx`).
const ADD_MADE: usize = 0xda69e5;
const ADD_MADE_NEXT: usize = 0xda69eb;
/// The add: where it begins its walk for the child (`mov rax, [rcx + 0x20];
/// mov rdi, rdx; mov rsi, rcx; test rax, rax; je`), the walk, and where it
/// goes on when the walk did not find it.
const ADD: usize = 0x7e7cd0;
const WALK: usize = 0x7e7cda;
const WALK_LOOP: usize = 0x7e7ce9;
const WALKED: usize = 0x7e7d0e;
/// The child `0xda66a0` makes: `new(0x40)` and the class's vtable.
const MADE: usize = 0xda6905;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(
            MADE,
            "b9 40 00 00 00 e8 c5 c9 c8 00 48 8b f8 48 89 45 48 49 8b 4e 38 c7 40 08 65 01 00 00 c7 40 10 f8 77 9c 02 4c 89 60 18 4c 89 60 20 4c 89 \
             60 28 44 89 60 30 c6 40 34 00",
            "save's dated node made",
        ),
        Point::jump(ADD_MADE, "ff 90 98 00 00 00", "save's dated node added", add_made_stub as *const () as usize, &ADD_MADE_RESUME),
        Point::keep(ADD, "48 89 74 24 20 57 48 83 ec 20", "dated node's add"),
        Point::jump(WALK, "48 8b 41 20 48 8b fa 48 8b f1 48 85 c0 74 25", "dated node's add walks its list", walk_stub as *const () as usize, &WALK_RESUME),
        Point::keep(WALK_LOOP, "0f 1f 80 00 00 00 00 48 39 38 0f 84 5f 01 00 00 48 8b 48 10 48 8b c1 48 85 c9 75 eb", "dated node's walk"),
        Point::keep(WALKED, "48 8b 06 48 8b ce 48 89", "dated node's add past its walk"),
    ]
}

/// The thread whose next walk is left out, by its id; 0 for none.
static MARKED: AtomicU32 = AtomicU32::new(0);

crate::resume!(ADD_MADE_RESUME: ADD_MADE_AT = ADD_MADE_NEXT);
crate::resume!(WALK_RESUME: LOOP_AT = WALK_LOOP, WALKED_AT = WALKED);

/// Replaces the call of the add: the thread marked around it.
#[unsafe(naked)]
extern "C" fn add_made_stub() {
    std::arch::naked_asm!(
        // The thread's id, from its TEB.
        "mov r10d, gs:[0x48]",
        "mov dword ptr [rip + {marked}], r10d",
        "call qword ptr [rax + 0x98]",
        "mov dword ptr [rip + {marked}], 0",
        "jmp qword ptr [rip + {next}]",
        marked = sym MARKED,
        next = sym ADD_MADE_AT,
    );
}

/// Replaces the start of the add's walk: past it once on the marked thread,
/// as the add goes on when the child is not in the list.
#[unsafe(naked)]
extern "C" fn walk_stub() {
    std::arch::naked_asm!(
        "mov eax, gs:[0x48]",
        "cmp eax, dword ptr [rip + {marked}]",
        "jne 2f",
        "mov dword ptr [rip + {marked}], 0",
        "cmp byte ptr [rip + {checking}], 0",
        "jne 2f",
        "mov rdi, rdx",
        "mov rsi, rcx",
        "jmp qword ptr [rip + {walked}]",
        "2:",
        "mov rax, [rcx + 0x20]",
        "mov rdi, rdx",
        "mov rsi, rcx",
        "test rax, rax",
        "je 3f",
        "jmp qword ptr [rip + {walk}]",
        "3:",
        "jmp qword ptr [rip + {walked}]",
        marked = sym MARKED,
        checking = sym CHECKING,
        walk = sym LOOP_AT,
        walked = sym WALKED_AT,
    );
}

/// With `check-loading` or `game-loads`, every add walks as the game's does:
/// the walk then finds what the game finds, and the saves written after a
/// load show the lists the same either way.
static CHECKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Called once the developer build's switches are read.
pub fn set_checking() {
    CHECKING.store(super::checking() || super::game_loads(), std::sync::atomic::Ordering::Relaxed);
}
