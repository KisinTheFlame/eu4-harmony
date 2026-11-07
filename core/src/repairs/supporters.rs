//! What a subject relation's end walks, in vanilla too: the subject's list
//! of the countries it shares a support of independence with, which the walk
//! frees under itself (docs/research/dependency-teardown.md).
//!
//! As the relation ends (`0xa61900`), the game walks the subject's
//! `support_independence` list and ends each of those supports, and after
//! each it refreshes the subject's diplomatic caches (`0x93fe30`), which
//! deletes every node of that very list and builds it again. The walk then
//! takes the next node from the one it stood on, which is freed: while the
//! heap leaves that block alone the walk goes on as before; once the refresh
//! has given it to another list, the walk goes on into that list, and a
//! country there that supports nothing of the subject's crashes the game.
//!
//! So while the walk goes on, a refresh of its subject on its thread waits,
//! and the walk's end does it once: the list stays whole under the walk, and
//! the caches end as the game's last refresh in the walk leaves them. Every
//! other refresh, and every other thread's, is the game's.
use crate::{
    objects::campaign::country,
    points::{Function, Point},
};
use std::cell::RefCell;

/// `(country, source label)`: a country's refresh of its diplomatic caches,
/// its lists of allies, neighbors, supporters and the like cleared and built
/// again from its relations; the label (`file.cpp:line`) is only logged.
const REFRESH: usize = 0x93fe30;
/// In the subject relation's end `0xa61900`, once it knows the walk is asked
/// for: `mov rbx, [rsi + 0x19a0]; test rbx, rbx; je`, the subject's list
/// (`rsi`, the country) and the jump past the walk when it is empty; the
/// walk's first node; the subject's refresh in the walk; where the walk takes
/// the next node; where it is over; and what follows it.
const WALK: usize = 0xa6196d;
const WALK_FIRST: usize = 0xa6197d;
const SUBJECT_REFRESHED: usize = 0xa61a0a;
const WALK_NEXT: usize = 0xa61ace;
const WALKED: usize = 0xa61b15;
const PAST_WALK: usize = 0xa61b1d;

type Refresh = unsafe extern "C" fn(usize, usize);
static ORIGINAL_REFRESH: Function<Refresh> = Function::new();

/// The repair as the world reset sees it ([`super::REPAIRS`]): it counts
/// nothing.
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: None };

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::replace(
            REFRESH,
            "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 10 fe ff ff",
            "country's diplomatic caches refreshed",
            refresh as Refresh,
            &ORIGINAL_REFRESH,
        ),
        Point::jump(
            WALK,
            "48 8b 9e a0 19 00 00 48 85 db 0f 84 a0 01 00 00",
            "subject relation's end walks the supporters",
            walk_stub as *const () as usize,
            &WALK_RESUME,
        )
        .uses(&[country::SUPPORT_INDEPENDENCE]),
        Point::keep(WALK_FIRST, "0f 1f 00 48 8b 03", "subject relation's end at the first supporter"),
        // `mov rax, [r14 + 0x18]` the subject, `lea rdx, [relation.cpp:1248]`,
        // `call 0x93fe30`: the refresh that frees the list.
        Point::keep(
            SUBJECT_REFRESHED,
            "48 8b 15 bf be 8d 01 49 8b 46 18 48 c1 e8 20 48 0f bf c8 48 8b 82 18 01 00 00 48 8d 15 4d bc 27 01 48 8b 0c c8 e8 fc e3 ed ff",
            "subject refreshed in the walk",
        ),
        Point::keep(WALK_NEXT, "48 8b 43 10 48 8b d8 48 85 c0 74 3b", "subject relation's end takes the next supporter"),
        Point::jump(WALKED, "44 0f b6 bd f8 00 00 00", "subject relation's end past the supporters", walked_stub as *const () as usize, &WALKED_RESUME),
    ]
}

/// A walk going on: its subject, the countries in its list as the walk
/// began, and the label of the subject's refresh that waits for its end, 0
/// while none does.
struct Walk {
    subject: usize,
    listed: String,
    waiting: usize,
}

thread_local! {
    /// The walks going on on this thread, the innermost last.
    static WALKS: RefCell<Vec<Walk>> = const { RefCell::new(Vec::new()) };
}

/// A refresh of a country: put off while a walk of that country goes on on
/// this thread, the game's otherwise.
unsafe extern "C" fn refresh(country: usize, label: usize) {
    let waits = WALKS.with(|walks| match walks.borrow_mut().iter_mut().rev().find(|walk| walk.subject == country) {
        Some(walk) => {
            walk.waiting = label;
            true
        }
        None => false,
    });
    if !waits {
        unsafe { ORIGINAL_REFRESH.get()(country, label) };
    }
}

/// The walk of `subject`'s supporters begins. With the developer build's
/// test switch `game-ends-dependencies` its refreshes are the game's, and
/// with the heap census's quarantine on, what the walk frees is kept and
/// filled, so that a walk reading the freed list fails there at once.
extern "C" fn walk_begins(subject: usize) {
    #[cfg(feature = "dev-heap-census")]
    crate::census::quarantine::begin("subject relation's end");
    let listed = listed(subject);
    if game_ends_dependencies() {
        log!("repair: off (game-ends-dependencies): a subject relation's end walks {}'s list ({listed}) as the game does", tag(subject));
    } else {
        WALKS.with(|walks| walks.borrow_mut().push(Walk { subject, listed, waiting: 0 }));
    }
}

/// The walk is over: the subject's refresh that waited is done.
extern "C" fn walk_ended() {
    #[cfg(feature = "dev-heap-census")]
    crate::census::quarantine::end();
    if game_ends_dependencies() {
        return;
    }
    let Some(walk) = WALKS.with(|walks| walks.borrow_mut().pop()) else { return };
    log!(
        "repair: a subject relation's end walked {}'s list ({}){}",
        tag(walk.subject),
        walk.listed,
        if walk.waiting != 0 { "; its refresh waited for the walk's end" } else { "" }
    );
    if walk.waiting != 0 {
        unsafe { refresh(walk.subject, walk.waiting) };
    }
}

/// A country's tag, from its three letters.
fn tag(country: usize) -> String {
    let m = crate::hooked_memory();
    match m.u64(country + country::TAG) {
        Ok(tag) => String::from_utf8_lossy(&tag.to_le_bytes()[..3]).into_owned(),
        Err(_) => "?".into(),
    }
}

/// The tags in a country's `support_independence` list, the first 32.
fn listed(country: usize) -> String {
    let m = crate::hooked_memory();
    let mut tags = Vec::new();
    let mut node = m.ptr(country + country::SUPPORT_INDEPENDENCE).unwrap_or(0);
    while node != 0 && tags.len() < 32 {
        let Ok(tag) = m.u64(node) else { break };
        tags.push(String::from_utf8_lossy(&tag.to_le_bytes()[..3]).into_owned());
        node = m.ptr(node + 0x10).unwrap_or(0);
    }
    tags.join(" ")
}

/// A developer build's test switch `game-ends-dependencies`: a subject
/// relation's end is the game's own, so that the defect can be tried.
fn game_ends_dependencies() -> bool {
    #[cfg(feature = "dev-test-switches")]
    return crate::dev::options().test_switches.game_ends_dependencies;
    #[cfg(not(feature = "dev-test-switches"))]
    false
}

crate::resume!(WALK_RESUME: WALK_EMPTY_AT = PAST_WALK, WALK_FIRST_AT = WALK_FIRST);
crate::resume!(WALKED_RESUME: WALKED_AT = PAST_WALK);

/// Replaces the start of the walk: the list's first node, and the walk
/// marked when there is one.
#[unsafe(naked)]
extern "C" fn walk_stub() {
    std::arch::naked_asm!(
        "mov rbx, [rsi + {list}]",
        "test rbx, rbx",
        "jnz 2f",
        "jmp qword ptr [rip + {empty}]",
        "2:",
        crate::call_rust!("mov rcx, rsi", call walk_begins),
        "jmp qword ptr [rip + {first}]",
        list = const country::SUPPORT_INDEPENDENCE,
        walk_begins = sym walk_begins,
        empty = sym WALK_EMPTY_AT,
        first = sym WALK_FIRST_AT,
    );
}

/// Replaces the first instruction past the walk, `movzx r15d, byte ptr
/// [rbp + 0xf8]`: the walk ended first.
#[unsafe(naked)]
extern "C" fn walked_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call walk_ended),
        "movzx r15d, byte ptr [rbp + 0xf8]",
        "jmp qword ptr [rip + {next}]",
        walk_ended = sym walk_ended,
        next = sym WALKED_AT,
    );
}
