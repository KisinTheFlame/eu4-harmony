//! A province becoming a conversion center stops being another center's
//! target. Both creation paths forget the old center's target but leave the
//! province referring to it: normal later removal cannot find that province
//! and leaves a dangling pointer (docs/research/reformation-center-crash.md).
//! Complete that detach at the two writes, before the new center is installed,
//! using the game's setter so conversion progress and map colors follow it.
use crate::{
    called_by_game,
    points::{GameFunction, Point},
};

const CREATE_DETACH: usize = 0xdae38f;
const CREATE_DETACHED: usize = 0xdae397;
const POLICY_DETACH: usize = 0xd37b17;
const POLICY_DETACHED: usize = 0xd37b1f;
/// `(province, &progress, center)`: updates both province fields and marks
/// the map colors stale when the conversion is cleared.
const SET_CONVERSION: GameFunction<3> = GameFunction::at(0xa16a50);
const PROGRESS: usize = 0x830;
const CONVERTING_CENTER: usize = 0x838;
const CENTER_TARGET: usize = 0x20;

pub(super) const REPAIR: super::Repair = super::Repair { points, ..super::Repair::NONE };

/// Only reached after the game has discarded this province as the old
/// center's target. The source's located center is left for creation to set.
#[called_by_game]
extern "C" fn detached(province: usize) {
    let progress = 0i32;
    if let Err(e) = SET_CONVERSION.call(&crate::hooked_memory(), [province, (&raw const progress) as usize, 0]) {
        log!("repair: a new conversion center's source was not detached from its old conversion: {e}");
    }
}

crate::resume!(CREATE_RESUME: CREATE_NEXT = CREATE_DETACHED);
crate::resume!(POLICY_RESUME: POLICY_NEXT = POLICY_DETACHED);

/// The manager's successful creation: rax is the old center, rbp the
/// province, rdi the new center and rcx zero. Replay both displaced moves.
#[unsafe(naked)]
extern "C" fn create_detach_stub() {
    std::arch::naked_asm!(
        "mov [rax + {target}], rcx",
        "pushfq",
        // call_rust's frame saves the game's rbp at [rbp].
        crate::call_rust!("mov rcx, [rbp]", call detached),
        "popfq",
        "mov rbx, [rdi + 0x10]",
        "jmp qword ptr [rip + {next}]",
        target = const CENTER_TARGET,
        detached = sym detached,
        next = sym CREATE_NEXT,
    );
}

/// The trading policy's successful creation: rax is the old center, rdi
/// the province, rsi the new center and rdx zero. Earlier refusals never
/// reach this point; replay both displaced moves.
#[unsafe(naked)]
extern "C" fn policy_detach_stub() {
    std::arch::naked_asm!(
        "mov [rax + {target}], rdx",
        "pushfq",
        crate::call_rust!("mov rcx, rdi", call detached),
        "popfq",
        "mov rbp, [rsi + 0x10]",
        "jmp qword ptr [rip + {next}]",
        target = const CENTER_TARGET,
        detached = sym detached,
        next = sym POLICY_NEXT,
    );
}

fn points() -> Vec<Point> {
    vec![
        Point::keep(0xdae383usize, "48 8b 85 38 08 00 00 48 85 c0 74 08", "center creation tests the source's converting center").uses(&[CONVERTING_CENTER]),
        Point::jump(
            CREATE_DETACH,
            "48 89 48 20 48 8b 5f 10",
            "center creation detaches both conversion references",
            create_detach_stub as *const () as usize,
            &CREATE_RESUME,
        )
        .uses(&[CENTER_TARGET]),
        Point::keep(CREATE_DETACHED, "48 89 7c 24 50 48 c7 c5 ff ff ff ff", "center creation after detaching its source"),
        Point::keep(0xd37b0busize, "48 8b 87 38 08 00 00 48 85 c0 74 08", "policy center creation tests the source's converting center")
            .uses(&[CONVERTING_CENTER]),
        Point::jump(
            POLICY_DETACH,
            "48 89 50 20 48 8b 6e 10",
            "policy center creation detaches both conversion references",
            policy_detach_stub as *const () as usize,
            &POLICY_RESUME,
        )
        .uses(&[CENTER_TARGET]),
        Point::keep(POLICY_DETACHED, "48 89 74 24 60 49 c7 c7 ff ff ff ff", "policy center creation after detaching its source"),
        Point::keep(SET_CONVERSION, "8b 02 89 81 30 08 00 00 4c 89 81 38 08 00 00", "set a province's converting center and progress")
            .uses(&[PROGRESS, CONVERTING_CENTER]),
    ]
}
