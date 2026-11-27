//! Per-search caching of invariant army checks and zero-allocation route
//! history walks (`docs/game/military/army-checks-in-searches.md`).
//!
//! During a route query (`0x96c520`), an army search (`0x1022b0`), or a threat
//! map look (`0x195750`), the game evaluates the same army across many
//! neighboring edges without modifying its leader, regiments, mercenary
//! company, route, or owner's modifiers:
//!
//! 1. **Fort-pass modifier (`0xd45291` -> `0xd40ae0(army, 0xe0, 0)`)**:
//!    Rebuilds the army's drill modifier set on the heap at every edge. Cached
//!    per `(scope, army)` on the current thread for the duration of the search.
//! 2. **Route history walk (`0xd4533a` -> `0x1beba0` + `operator delete`)**:
//!    Deep-copies the doubly-linked route list at `army+0x100` onto the heap,
//!    optionally prepends the locked current province, walks backward to find
//!    the previous and last non-ZoC land provinces, and immediately frees every
//!    copied node. Replaced with a read-only in-place backward walk that
//!    allocates nothing.
//! 3. **Army movement speed modifier (`0x1a6988` -> `0xd40790(army, &out, 0xfb, 0, 0)`)**:
//!    Rebuilds the army's drill modifier set for every edge whose travel cost
//!    is evaluated. Cached per `(scope, army)` on the current thread.
//!
//! All three are exact: checked mode (`speed::checked`) compares every cached
//! answer and every in-place route walk against the game's own computation.
use crate::base::called_by_game;
use crate::native::game_heap::DELETE;
use crate::native::objects::campaign::{
    functions::ROUTE_QUERY,
    geometry::{LAND, TERRAIN},
    globals::WORLD,
    province::SIZE as PROVINCE_SIZE,
    unit::{LOCATION, PATH},
    world::PROVINCES,
};
use crate::native::points::{Function, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use std::cell::RefCell;

const THREAT_LOOK: usize = 0x195750;
const ARMY_SEARCH: usize = 0x1022b0;

const FORT_PASS_CALL: usize = 0xd45289;
const FORT_PASS_DONE: usize = 0xd45296;
const UNIT_HAS_MODIFIER: usize = 0xd40ae0;
const MODIFIER_IGNORE_FORTS: u32 = 0xe0;

const ROUTE_WALK_START: usize = 0xd4533a;
const ROUTE_WALK_DONE: usize = 0xd45482;
const COPY_ROUTE_LIST: usize = 0x1beba0;
const PROVINCE_FORT_ZOC: usize = 0xa275d0;
const UNIT_LOCKED_IN_MOVE_SLOT: usize = 0x118;
const PROVINCE_ID_OFFSET: usize = 0x20;
const PROVINCE_GEOMETRY_OFFSET: usize = 0x28;

const SPEED_MOD_CALL: usize = 0x1a6977;
const SPEED_MOD_DONE: usize = 0x1a698d;
const UNIT_MODIFIER_VALUE: usize = 0xd40790;
const MODIFIER_MOVEMENT_SPEED: u32 = 0xfb;

const MAX_SCOPE_DEPTH: usize = 16;

type RouteQuery = unsafe extern "C" fn(usize, usize, usize, usize, usize, usize, usize, usize) -> u8;
type ThreatLook = unsafe extern "C" fn(usize, usize, i32, u8, usize, usize) -> usize;
type ArmySearch = unsafe extern "C" fn(usize, i32, usize, usize, *mut f32, *mut i32, *mut u8, *mut u8, *mut u8, u8, u8, *mut u8) -> i32;

static ORIGINAL_ROUTE_QUERY: Function<RouteQuery> = Function::new();
static ORIGINAL_THREAT_LOOK: Function<ThreatLook> = Function::new();
static ORIGINAL_ARMY_SEARCH: Function<ArmySearch> = Function::new();

#[derive(Clone, Copy, Default)]
struct Scope {
    fort_army: usize,
    fort_pass: bool,
    speed_army: usize,
    speed_val: i32,
}

struct ScopeStack {
    frames: [Scope; MAX_SCOPE_DEPTH],
    depth: usize,
}

impl ScopeStack {
    const fn new() -> Self {
        Self { frames: [Scope { fort_army: 0, fort_pass: false, speed_army: 0, speed_val: 0 }; MAX_SCOPE_DEPTH], depth: 0 }
    }

    fn push(&mut self) -> bool {
        if self.depth >= MAX_SCOPE_DEPTH {
            return false;
        }
        self.frames[self.depth] = Scope::default();
        self.depth += 1;
        true
    }

    fn pop(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }

    fn top_mut(&mut self) -> Option<&mut Scope> {
        self.depth.checked_sub(1).map(|i| &mut self.frames[i])
    }
}

thread_local! {
    static SCOPES: RefCell<ScopeStack> = const { RefCell::new(ScopeStack::new()) };
}

/// RAII scope guard for one search on the current thread.
struct SearchGuard {
    pushed: bool,
}

impl Drop for SearchGuard {
    fn drop(&mut self) {
        if self.pushed {
            SCOPES.with(|s| s.borrow_mut().pop());
        }
    }
}

/// Enters a per-search cache scope on the current thread when `army-checks` is
/// active, around `0x96c520`, `0x195750`, and `0x1022b0`.
fn search_scope() -> SearchGuard {
    if speed::game_path(&SPEED_UP) {
        return SearchGuard { pushed: false };
    }
    let pushed = SCOPES.with(|s| s.borrow_mut().push());
    SearchGuard { pushed }
}

#[called_by_game]
unsafe extern "C" fn route_query(a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize, a7: usize) -> u8 {
    let _scope = search_scope();
    unsafe { ORIGINAL_ROUTE_QUERY.get()(a0, a1, a2, a3, a4, a5, a6, a7) }
}

#[called_by_game]
unsafe extern "C" fn threat_look(a0: usize, a1: usize, a2: i32, a3: u8, a4: usize, a5: usize) -> usize {
    let _scope = search_scope();
    unsafe { ORIGINAL_THREAT_LOOK.get()(a0, a1, a2, a3, a4, a5) }
}

#[called_by_game]
unsafe extern "C" fn army_search(
    callback: usize,
    depth: i32,
    unit: usize,
    argument: usize,
    score: *mut f32,
    a: *mut i32,
    b: *mut u8,
    c: *mut u8,
    d: *mut u8,
    flag: u8,
    other: u8,
    out: *mut u8,
) -> i32 {
    let _scope = search_scope();
    unsafe { ORIGINAL_ARMY_SEARCH.get()(callback, depth, unit, argument, score, a, b, c, d, flag, other, out) }
}

unsafe fn native_fort_pass(army: usize) -> bool {
    type HasMod = unsafe extern "C" fn(usize, u32, u8) -> u8;
    let f: HasMod = unsafe { std::mem::transmute(crate::native::hooked_memory().at(UNIT_HAS_MODIFIER)) };
    unsafe { f(army, MODIFIER_IGNORE_FORTS, 0) != 0 }
}

unsafe fn native_speed_mod(army: usize, out: *mut i32) -> i32 {
    type ModVal = unsafe extern "C" fn(usize, *mut i32, u32, u8, usize) -> *mut i32;
    let f: ModVal = unsafe { std::mem::transmute(crate::native::hooked_memory().at(UNIT_MODIFIER_VALUE)) };
    let ptr = unsafe { f(army, out, MODIFIER_MOVEMENT_SPEED, 0, 0) };
    unsafe { *ptr }
}

fn fort_pass_with(army: usize, compute: impl FnOnce(usize) -> bool) -> (bool, bool) {
    let cached = SCOPES.with(|s| {
        let stack = s.borrow();
        stack.depth.checked_sub(1).and_then(|i| {
            let top = &stack.frames[i];
            (top.fort_army == army && army != 0).then_some(top.fort_pass)
        })
    });
    if let Some(hit) = cached {
        return (hit, true);
    }
    let val = compute(army);
    if army != 0 {
        SCOPES.with(|s| {
            if let Some(top) = s.borrow_mut().top_mut() {
                top.fort_army = army;
                top.fort_pass = val;
            }
        });
    }
    (val, false)
}

fn speed_mod_with(army: usize, compute: impl FnOnce(usize) -> i32) -> (i32, bool) {
    let cached = SCOPES.with(|s| {
        let stack = s.borrow();
        stack.depth.checked_sub(1).and_then(|i| {
            let top = &stack.frames[i];
            (top.speed_army == army && army != 0).then_some(top.speed_val)
        })
    });
    if let Some(hit) = cached {
        return (hit, true);
    }
    let val = compute(army);
    if army != 0 {
        SCOPES.with(|s| {
            if let Some(top) = s.borrow_mut().top_mut() {
                top.speed_army = army;
                top.speed_val = val;
            }
        });
    }
    (val, false)
}

#[called_by_game]
extern "C" fn fort_pass_for_stub(army: usize) -> u32 {
    let (val, hit) = fort_pass_with(army, |a| unsafe { native_fort_pass(a) });
    if hit && speed::checked(&SPEED_UP) {
        let game = unsafe { native_fort_pass(army) };
        speed::compared(&SPEED_UP, val == game, || format!("fort pass for army {army:#x}: cached {val}, game {game}"));
    }
    u32::from(val)
}

#[called_by_game]
extern "C" fn speed_mod_for_stub(army: usize, out: *mut i32) -> usize {
    let (val, hit) = speed_mod_with(army, |a| unsafe { native_speed_mod(a, out) });
    unsafe { *out = val };
    if hit && speed::checked(&SPEED_UP) {
        let mut game_out = 0i32;
        let game = unsafe { native_speed_mod(army, &mut game_out) };
        speed::compared(&SPEED_UP, val == game, || format!("speed modifier for army {army:#x}: cached {val}, game {game}"));
    }
    out as usize
}

/// Read-only in-place backward walk of an army's route list (`0xd4533a..0xd45482`),
/// returning `(new_r12, new_r13)` without copying or freeing any list nodes.
fn walk_route_history(
    mut tail: usize,
    prepend_id: Option<i32>,
    current: usize,
    mut r12: usize,
    mut r13: usize,
    read_node: impl Fn(usize) -> (i32, usize),
    mut visit_province: impl FnMut(i32) -> (usize, bool),
) -> (usize, usize) {
    let mut found_prev = false;
    let mut step = |prov_id: i32| -> bool {
        let (prov, clears_zoc) = visit_province(prov_id);
        if !found_prev && prov != current {
            r13 = prov;
            found_prev = true;
        }
        if clears_zoc {
            r12 = prov;
            return true;
        }
        false
    };
    while tail != 0 {
        let (prov_id, prev) = read_node(tail);
        if step(prov_id) {
            return (r12, r13);
        }
        tail = prev;
    }
    if let Some(prov_id) = prepend_id {
        let _ = step(prov_id);
    }
    (r12, r13)
}

#[called_by_game]
unsafe extern "C" fn route_walk_for_stub(army: usize, current: usize, country_handle: usize, init_r12: usize, init_r13: usize, out_r13: *mut usize) -> usize {
    let m = crate::native::hooked_memory();
    let head = unsafe { *((army + PATH) as *const usize) };
    let tail = unsafe { *((army + PATH + 8) as *const usize) };
    let count = unsafe { *((army + PATH + 0x10) as *const i32) };

    let loc = unsafe { *((army + LOCATION) as *const usize) };
    let prepend_id = if loc != 0 {
        let vtable = unsafe { *(army as *const usize) };
        let locked_fn: unsafe extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(*((vtable + UNIT_LOCKED_IN_MOVE_SLOT) as *const usize)) };
        if unsafe { locked_fn(army) } != 0 {
            let loc_id = unsafe { *((loc + PROVINCE_ID_OFFSET) as *const i32) };
            if count == 0 || unsafe { *(head as *const i32) } != loc_id {
                Some(loc_id)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let provinces = unsafe {
        let world = *(m.at(WORLD) as *const usize);
        *((world + PROVINCES) as *const usize)
    };
    type FortZoc = unsafe extern "C" fn(usize, usize) -> usize;
    let fort_zoc: FortZoc = unsafe { std::mem::transmute(m.at(PROVINCE_FORT_ZOC)) };

    let (r12, r13) = walk_route_history(
        tail,
        prepend_id,
        current,
        init_r12,
        init_r13,
        |node| unsafe { (*(node as *const i32), *((node + 8) as *const usize)) },
        |prov_id| {
            let prov = (provinces as isize + (prov_id as isize) * (PROVINCE_SIZE as isize)) as usize;
            let geom = unsafe { *((prov + PROVINCE_GEOMETRY_OFFSET) as *const usize) };
            let land = unsafe { (*((geom + TERRAIN) as *const u8) & LAND) != 0 };
            let clears_zoc = land && unsafe { fort_zoc(prov, country_handle) } == 0;
            (prov, clears_zoc)
        },
    );

    if speed::checked(&SPEED_UP) {
        let (game_r12, game_r13) = unsafe { native_route_walk(army, current, country_handle, init_r12, init_r13, prepend_id) };
        speed::compared(&SPEED_UP, (r12, r13) == (game_r12, game_r13), || {
            format!("route walk for army {army:#x}: in-place ({r12:#x}, {r13:#x}), game ({game_r12:#x}, {game_r13:#x})")
        });
    }

    unsafe { *out_r13 = r13 };
    r12
}

/// Runs the game's original copy-prepend-walk-free route walk for checked-mode
/// verification.
unsafe fn native_route_walk(army: usize, current: usize, country_handle: usize, mut r12: usize, mut r13: usize, prepend_id: Option<i32>) -> (usize, usize) {
    let m = crate::native::hooked_memory();
    let provinces = unsafe {
        let world = *(m.at(WORLD) as *const usize);
        *((world + PROVINCES) as *const usize)
    };
    type FortZoc = unsafe extern "C" fn(usize, usize) -> usize;
    let fort_zoc: FortZoc = unsafe { std::mem::transmute(m.at(PROVINCE_FORT_ZOC)) };
    #[repr(C)]
    struct List {
        head: usize,
        tail: usize,
        count: i32,
        pad: u32,
    }
    let mut copy = List { head: 0, tail: 0, count: 0, pad: 0 };
    type CopyList = unsafe extern "C" fn(*mut List, usize) -> *mut List;
    let copy_fn: CopyList = unsafe { std::mem::transmute(m.at(COPY_ROUTE_LIST)) };
    unsafe { copy_fn(&mut copy, army + PATH) };

    let mut prepended = [0u64; 4];
    if let Some(loc_id) = prepend_id {
        let node = prepended.as_mut_ptr() as usize;
        unsafe {
            *(node as *mut i32) = loc_id;
            *((node + 8) as *mut usize) = 0;
            *((node + 0x10) as *mut usize) = copy.head;
            *((node + 0x18) as *mut u8) = 0;
        }
        let old_head = copy.head;
        let old_count = copy.count;
        copy.head = node;
        copy.count = old_count + 1;
        if old_count == 0 {
            copy.tail = node;
        } else {
            unsafe { *((old_head + 8) as *mut usize) = node };
        }
    }

    let mut cur = copy.tail;
    let mut found_prev = false;
    while cur != 0 {
        let prov_id = unsafe { *(cur as *const i32) };
        let prov = (provinces as isize + (prov_id as isize) * (PROVINCE_SIZE as isize)) as usize;
        if !found_prev && prov != current {
            r13 = prov;
            found_prev = true;
        }
        let geom = unsafe { *((prov + PROVINCE_GEOMETRY_OFFSET) as *const usize) };
        if unsafe { (*((geom + TERRAIN) as *const u8) & LAND) != 0 } && unsafe { fort_zoc(prov, country_handle) } == 0 {
            r12 = prov;
            break;
        }
        cur = unsafe { *((cur + 8) as *const usize) };
    }

    // Free only the heap-allocated nodes created by `copy_fn` (skip stack-prepended head).
    let stack_node = prepended.as_mut_ptr() as usize;
    let delete_fn: unsafe extern "C" fn(usize, usize) = unsafe { std::mem::transmute(m.at(DELETE.rva())) };
    let mut node = copy.head;
    while node != 0 {
        let next = unsafe { *((node + 0x10) as *const usize) };
        if node != stack_node {
            unsafe { delete_fn(node, 0x20) };
        }
        node = next;
    }
    (r12, r13)
}

resume!(FORT_PASS_RESUME: FORT_PASS_DONE_AT = FORT_PASS_DONE);
resume!(ROUTE_WALK_RESUME: ROUTE_WALK_DONE_AT = ROUTE_WALK_DONE);
resume!(SPEED_MOD_RESUME: SPEED_MOD_DONE_AT = SPEED_MOD_DONE);

#[unsafe(naked)]
extern "C" fn fort_pass_stub() {
    std::arch::naked_asm!(
        call_rust!(call fort_pass_for_stub -> eax),
        "jmp qword ptr [rip + {done}]",
        fort_pass_for_stub = sym fort_pass_for_stub,
        done = sym FORT_PASS_DONE_AT,
    );
}

#[unsafe(naked)]
extern "C" fn route_walk_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rax, [rsp + 0x40]",
            "mov qword ptr [rsp + 0x28], rax",
            "mov qword ptr [rsp + 0x20], r13",
            "mov r9, r12",
            "mov r8, rdi",
            "mov rdx, qword ptr [rbp]",
            "mov rcx, rbx",
            call route_walk_for_stub -> r12,
            "mov r13, qword ptr [rsp + 0x40]",
        ),
        "jmp qword ptr [rip + {done}]",
        route_walk_for_stub = sym route_walk_for_stub,
        done = sym ROUTE_WALK_DONE_AT,
    );
}

#[unsafe(naked)]
extern "C" fn speed_mod_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, qword ptr [rbp]",
            "lea rdx, [rbp + 0x78]",
            call speed_mod_for_stub -> rax,
        ),
        "jmp qword ptr [rip + {done}]",
        speed_mod_for_stub = sym speed_mod_for_stub,
        done = sym SPEED_MOD_DONE_AT,
    );
}

/// Checked, every cached fort-pass and speed check and every in-place route
/// walk is verified against the game's own computation.
pub const SPEED_UP: SpeedUp = SpeedUp::new("army-checks", "ai", points, true);

fn points() -> Vec<Point> {
    vec![
        Point::replace(ROUTE_QUERY, "4c 89 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55", "route query", route_query as RouteQuery, &ORIGINAL_ROUTE_QUERY),
        Point::replace(THREAT_LOOK, "44 88 4c 24 20 48 89 54 24 10 55 53 41 54 41 55", "threat map look", threat_look as ThreatLook, &ORIGINAL_THREAT_LOOK),
        Point::replace(ARMY_SEARCH, "4c 89 4c 24 20 89 54 24 10 48 89 4c 24 08 55 53", "army search", army_search as ArmySearch, &ORIGINAL_ARMY_SEARCH),
        Point::jump(
            FORT_PASS_CALL,
            "45 33 c0 ba e0 00 00 00 e8 4a b8 ff ff",
            "fort zone check asks whether army ignores forts",
            fort_pass_stub as *const () as usize,
            &FORT_PASS_RESUME,
        ),
        Point::keep(FORT_PASS_DONE, "84 c0 0f 85 57 06 00 00", "fort zone check branches on ignore-forts"),
        Point::keep(UNIT_HAS_MODIFIER, "48 89 5c 24 08 48 89 74 24 10 4c 89 4c 24 20 55", "unit boolean modifier check"),
        Point::jump(
            ROUTE_WALK_START,
            "48 8d 93 00 01 00 00 48 8d 4c 24 38 e8 55 98 47 ff",
            "fort zone check copies route history",
            route_walk_stub as *const () as usize,
            &ROUTE_WALK_RESUME,
        )
        .uses(&[PATH]),
        Point::keep(ROUTE_WALK_DONE, "49 8b 46 28 0f b6 88 2a 02 00 00 80 e1 01", "fort zone check after route history walk"),
        Point::keep(COPY_ROUTE_LIST, "48 89 5c 24 10 48 89 6c 24 18 48 89 4c 24 08 56", "route list copy constructor"),
        Point::keep(PROVINCE_FORT_ZOC, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "province fort zone of control"),
        Point::jump(
            SPEED_MOD_CALL,
            "45 33 c9 41 b8 fb 00 00 00 48 8d 54 24 70 48 8b cd e8 03 9e b9 00",
            "army movement speed reads movement speed modifier",
            speed_mod_stub as *const () as usize,
            &SPEED_MOD_RESUME,
        ),
        Point::keep(SPEED_MOD_DONE, "8b 18 81 c3 e8 03 00 00", "army movement speed adds base speed"),
        Point::keep(UNIT_MODIFIER_VALUE, "48 89 5c 24 08 44 88 4c 24 20 55 56 57 41 54 41", "unit numeric modifier lookup"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn scope_stack_caches_per_army_and_isolates_nested_searches() {
        let calls = Cell::new(0);
        let compute_fort = |army: usize| {
            calls.set(calls.get() + 1);
            army == 0x100
        };
        let compute_speed = |army: usize| {
            calls.set(calls.get() + 1);
            army as i32
        };

        // Outside any search scope, nothing is cached.
        assert_eq!(fort_pass_with(0x100, compute_fort), (true, false));
        assert_eq!(fort_pass_with(0x100, compute_fort), (true, false));
        assert_eq!(calls.get(), 2);

        {
            let _outer = search_scope();
            assert_eq!(fort_pass_with(0x100, compute_fort), (true, false));
            assert_eq!(fort_pass_with(0x100, compute_fort), (true, true));
            assert_eq!(speed_mod_with(0x100, compute_speed), (0x100, false));
            assert_eq!(speed_mod_with(0x100, compute_speed), (0x100, true));
            assert_eq!(calls.get(), 4);

            // Nested search gets an isolated scope and restores outer on drop.
            {
                let _inner = search_scope();
                assert_eq!(fort_pass_with(0x200, compute_fort), (false, false));
                assert_eq!(fort_pass_with(0x200, compute_fort), (false, true));
                assert_eq!(calls.get(), 5);
            }

            assert_eq!(fort_pass_with(0x100, compute_fort), (true, true));
            assert_eq!(speed_mod_with(0x100, compute_speed), (0x100, true));
            assert_eq!(calls.get(), 5);
        }

        // After outer scope drops, caching stops again.
        assert_eq!(fort_pass_with(0x100, compute_fort), (true, false));
        assert_eq!(calls.get(), 6);
    }

    #[test]
    fn route_walk_matches_prepend_and_backward_zoc_rules() {
        // Nodes: [10] <-> [20] <-> [30] (tail is 30).
        // Province pointer is id * 1000. Current province is 30_000.
        // 30 is current (skipped for r13) and in ZoC; 20 is land in ZoC (becomes r13);
        // 10 is sea (does not clear ZoC); prepended locked location 5 is land clear of ZoC (becomes r12).
        let nodes = [(10i32, 0usize), (20, 1), (30, 2)];
        let read_node = |idx: usize| nodes[idx - 1];
        let (r12, r13) = walk_route_history(3, Some(5), 30_000, 0xaaa, 0xbbb, read_node, |id| {
            let prov = (id as usize) * 1000;
            let clears_zoc = id == 5;
            (prov, clears_zoc)
        });
        assert_eq!(r13, 20_000);
        assert_eq!(r12, 5_000);

        // When route is empty and locked location is prepended, that single province is visited.
        let (r12, r13) = walk_route_history(0, Some(7), 99_000, 0xaaa, 0xbbb, read_node, |id| ((id as usize) * 1000, true));
        assert_eq!(r13, 7_000);
        assert_eq!(r12, 7_000);

        // Stops at first land province clearing ZoC without visiting earlier nodes.
        let visited = RefCell::new(Vec::new());
        let (r12, r13) = walk_route_history(3, Some(5), 99_000, 0xaaa, 0xbbb, read_node, |id| {
            visited.borrow_mut().push(id);
            ((id as usize) * 1000, id == 20)
        });
        assert_eq!(visited.borrow().as_slice(), &[30, 20]);
        assert_eq!(r13, 30_000);
        assert_eq!(r12, 20_000);
    }
}
