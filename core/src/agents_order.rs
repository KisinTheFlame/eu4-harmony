//! A developer build's test switch `agents-order`: the order in which an AI
//! country's top agent has its agents do their day, so that whether one
//! agent's day depends on what another's did before it can be tried
//! (docs/research/late-game.md, "Whether the AI's agents depend on each
//! other").
//!
//! Each day, a country's task in the AI's parallel stage steps its top agent
//! (`0x14b370`), which, after its own think, has each agent of two lists do
//! its day (vtable slot `+0x58`), one after another: its children (`+0x40`)
//! and its armies' agents (`+0xc50`). Each list is the game's doubly linked
//! list: the first node at `+0` and the last at `+8`, a node's agent at `+0`,
//! the node before it at `+8` and the one after it at `+0x10`. With the
//! switch, the two walks are the framework's: `forward` walks each list from
//! its first node as the game does, and `reversed` from its last back. A game started
//! with `-seed`, where each agent draws from a generator of its own, then
//! plays the same days in either order if no agent reads what another of
//! its country wrote that day.
use crate::called_by_game;
use crate::{dev::AgentsOrder, points::Point};

/// In the top agent's step `0x14b370`, once it has thought: `mov rbx, [rdi +
/// 0x40]; test rbx, rbx; je`, the walk of its children (`rdi`, the top agent;
/// `rbp`, the day's argument it hands on).
const WALKS: usize = 0x14b622;
/// What follows the walk of its armies' agents: `mov edx, esi; mov rcx, rdi;
/// call`.
const WALKED: usize = 0x14b675;
const CHILDREN: usize = 0x40;
const ARMIES: usize = 0xc50;
/// A list's last node, and a node's node before it and after it.
const LAST: usize = 8;
const PREVIOUS: usize = 8;
const NEXT: usize = 0x10;
/// An agent's day, in its vtable.
const DAY: usize = 0x58;

fn order() -> AgentsOrder {
    crate::dev::options().test_switches.agents_order
}

/// Has the agents of the list at `list` do their day, from its first node on
/// (`step` [`NEXT`]) or from its last back (`step` [`PREVIOUS`]): each node's
/// neighbor read before its agent's day, as the game's walk reads it.
unsafe fn walk_list(list: usize, from: usize, step: usize, day: usize) {
    let read = |at: usize| unsafe { *(at as *const usize) };
    let mut node = read(list + from);
    while node != 0 {
        let agent = read(node);
        node = read(node + step);
        let day_of: unsafe extern "C" fn(usize, usize) = unsafe { std::mem::transmute(read(read(agent) + DAY)) };
        unsafe { day_of(agent, day) };
    }
}

/// The two walks, in the switch's order.
#[called_by_game]
unsafe extern "C" fn walk(top: usize, day: usize) {
    let (from, step) = if order() == AgentsOrder::Reversed { (LAST, PREVIOUS) } else { (0, NEXT) };
    for list in [CHILDREN, ARMIES] {
        unsafe { walk_list(top + list, from, step, day) };
    }
}

crate::resume!(WALKED_RESUME: WALKED_AT = WALKED);

/// Replaces the two walks: `rdi` is the top agent, `rbp` the day's
/// argument; the registers the step uses after them (`rdi`, `rsi`, `rbp`)
/// stay as they were.
#[unsafe(naked)]
extern "C" fn walks_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, rdi", "mov rdx, [rbp]", call walk),
        "jmp qword ptr [rip + {walked}]",
        walk = sym walk,
        walked = sym WALKED_AT,
    );
}

/// The points, when the switch asks for the framework's walks.
pub fn points() -> Vec<Point> {
    let order = order();
    if order == AgentsOrder::Game {
        return Vec::new();
    }
    log!("test switch agents-order: an AI country's top agent has its agents do their day {order:?}");
    vec![
        Point::jump(WALKS, "48 8b 5f 40 48 85 db 74 1a", "top agent's walk of its agents", walks_stub as *const () as usize, &WALKED_RESUME).uses(&[CHILDREN]),
        Point::keep(0x14b645usize, "48 8b 9f 50 0c 00 00", "top agent's walk of its armies' agents").uses(&[ARMIES]),
        Point::keep(WALKED, "8b d6 48 8b cf", "top agent's step after its walks"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    static WALKED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

    /// An agent's day that notes the agent.
    unsafe extern "C" fn noted(agent: usize, _: usize) {
        WALKED.lock().unwrap().push(agent);
    }

    #[test]
    fn a_list_is_walked_from_either_end() {
        // An agent is its vtable's address, whose slot +0x58 notes it; three
        // nodes of 0x20 bytes, linked both ways.
        let table = [noted as *const () as usize; 12];
        let agents = [table.as_ptr() as usize; 3];
        let mut nodes = [[0usize; 4]; 3];
        let base = nodes.as_mut_ptr() as usize;
        for (i, node) in nodes.iter_mut().enumerate() {
            node[0] = agents.as_ptr() as usize + i * 8;
            node[1] = if i > 0 { base + (i - 1) * 0x20 } else { 0 };
            node[2] = if i < 2 { base + (i + 1) * 0x20 } else { 0 };
        }
        let list = [base, base + 0x40];
        let at = |i: usize| agents.as_ptr() as usize + i * 8;
        unsafe { walk_list(list.as_ptr() as usize, 0, NEXT, 0) };
        assert_eq!(std::mem::take(&mut *WALKED.lock().unwrap()), [at(0), at(1), at(2)]);
        unsafe { walk_list(list.as_ptr() as usize, LAST, PREVIOUS, 0) };
        assert_eq!(std::mem::take(&mut *WALKED.lock().unwrap()), [at(2), at(1), at(0)]);
        let empty = [0usize; 2];
        unsafe { walk_list(empty.as_ptr() as usize, 0, NEXT, 0) };
        assert!(WALKED.lock().unwrap().is_empty());
    }
}
