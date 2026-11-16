//! Keep the target province list's allocation during one country's distance
//! rebuild. With source provinces, the native rebuild still fills the list
//! afresh for every target. All distance writes stay in their original order.
//! See docs/game/program/saves.md, "Country distance scratch space".
use crate::{called_by_game, points::Point, speed::SpeedUp};

const INITIALIZE: usize = 0x2d0b5d;
const INITIALIZED: usize = 0x2d0b65;
const TARGET: usize = 0x2d1220;
const FILL: usize = 0x2d1236;
const RESERVE: usize = 0x2d1251;
const GROW: usize = 0x2d1256;
const APPEND: usize = 0x2d12c3;
const CLEAR: usize = 0x2d2036;
const CLEARED: usize = 0x2d2048;
const STORE: usize = 0x2d2052;
const FINISH: usize = 0x2d20c8;
const FINISHED: usize = 0x2d20d0;

pub const SPEED_UP: SpeedUp = SpeedUp { name: "distance-scratch", group: "loading", points, checkable: false };

fn points() -> Vec<Point> {
    vec![
        Point::keep(0x2d0b45usize, "48 8d ac 24 08 fc ff ff 48 81 ec f8 04 00 00", "country distance scratch stack frame"),
        Point::keep(0x2d11ddusize, "66 89 bd 58 04 00 00 66 89 bd 50 04 00 00", "country distance minima initialized for each target"),
        Point::keep(
            0x2d15d0usize,
            "48 8b 4d d8 48 89 4c 24 68 48 63 45 e4 48 8d 04 81 48 89 45 80 48 3b c8 0f 84 34 0a 00 00",
            "country distance pair loop uses the assembled source list",
        ),
        Point::jump(
            INITIALIZE,
            "89 44 24 30 89 44 24 60",
            "country distance scratch initialized once",
            initialize_stub as *const () as usize,
            &INITIALIZE_RESUME,
        ),
        Point::jump(
            TARGET,
            "48 89 54 24 38 4c 89 64 24 40 41 8b fc 48 c7 44 24 48 00 00 00 00",
            "country distance target retains its scratch capacity",
            target_stub as *const () as usize,
            &TARGET_RESUME,
        ),
        Point::keep(FILL, "45 8b c4 4d 8b a5 40 23 00 00", "country distance target filled afresh"),
        Point::jump(RESERVE, "45 85 ff 7e 6d", "country distance target uses its retained capacity", reserve_stub as *const () as usize, &RESERVE_RESUME),
        Point::keep(APPEND, "4d 3b f4 0f 84 2c 01 00 00", "country distance target appends its current provinces"),
        Point::jump(
            CLEAR,
            "4c 8b 4c 24 40 4d 85 c9 74 08 49 8b c9 e8 a8 74 46 01",
            "country distance target keeps its scratch allocation",
            clear_stub as *const () as usize,
            &CLEAR_RESUME,
        ),
        Point::keep(CLEARED, "4c 8b 7c 24 70 4c 8b 74 24 78", "country distance proceeds to the next target"),
        Point::keep(STORE, "44 0f b7 8d 50 04 00 00 44 0f b7 85 58 04 00 00", "country distance writes both default minima without source provinces"),
        Point::jump(FINISH, "48 89 45 d0 44 89 65 e4", "country distance scratch released once", finish_stub as *const () as usize, &FINISH_RESUME),
    ]
}

crate::resume!(INITIALIZE_RESUME: INITIALIZE_AT = INITIALIZED);
crate::resume!(TARGET_RESUME: FILL_AT = FILL, STORE_AT = STORE);
crate::resume!(RESERVE_RESUME: GROW_AT = GROW, APPEND_AT = APPEND);
crate::resume!(CLEAR_RESUME: CLEARED_AT = CLEARED);
crate::resume!(FINISH_RESUME: FINISH_AT = FINISHED);

#[called_by_game]
extern "C" fn release(block: usize, capacity: usize) {
    crate::guarded("country distance scratch", || {
        if block != 0 {
            crate::game_heap::delete(&crate::hooked_memory(), block, capacity * 4).expect("the native scratch allocation must be released");
        }
    });
}

/// The scratch vector is at rsp+0x38: vtable, pointer, capacity, length.
/// No allocation exists on the early return before the country loop.
#[unsafe(naked)]
extern "C" fn initialize_stub() {
    std::arch::naked_asm!(
        "mov [rsp + 0x30], eax",
        "mov [rsp + 0x60], eax",
        "mov qword ptr [rsp + 0x40], 0",
        "mov qword ptr [rsp + 0x48], 0",
        "jmp qword ptr [rip + {next}]",
        next = sym INITIALIZE_AT,
    );
}

#[unsafe(naked)]
extern "C" fn target_stub() {
    std::arch::naked_asm!(
        // With no source provinces the native product loop is empty. Its
        // target list is discarded without reading an item, and both
        // minima stay at the native 64000 initialized before this site.
        "cmp dword ptr [rbp - 0x1c], 0",
        "je 2f",
        "mov [rsp + 0x38], rdx",
        "mov edi, [rsp + 0x48]",
        "mov dword ptr [rsp + 0x4c], 0",
        "jmp qword ptr [rip + {next}]",
        "2:",
        "jmp qword ptr [rip + {store}]",
        next = sym FILL_AT,
        store = sym STORE_AT,
    );
}

#[unsafe(naked)]
extern "C" fn reserve_stub() {
    std::arch::naked_asm!(
        "cmp r15d, edi",
        "jle 2f",
        "jmp qword ptr [rip + {grow}]",
        "2:",
        "jmp qword ptr [rip + {append}]",
        grow = sym GROW_AT,
        append = sym APPEND_AT,
    );
}

/// The native instructions just before this site clear the length. Only
/// the free is omitted; the buffer and capacity stay local to this call.
#[unsafe(naked)]
extern "C" fn clear_stub() {
    std::arch::naked_asm!("jmp qword ptr [rip + {next}]", next = sym CLEARED_AT);
}

#[unsafe(naked)]
extern "C" fn finish_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, [rbp + 0x48]", "mov edx, [rbp + 0x50]", call release),
        "mov [rbp - 0x30], rax",
        "mov [rbp - 0x1c], r12d",
        "jmp qword ptr [rip + {next}]",
        release = sym release,
        next = sym FINISH_AT,
    );
}
