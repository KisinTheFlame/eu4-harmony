//! Native route queries and move/stop commands for own land armies, plus the
//! record of which extension installed which route.
use super::{fail, paused, span, world, Ctx, GResult, MAX_UNITS};
use crate::{
    memory::Memory,
    objects::campaign::{classes, globals, path, province, route_options, unit},
    points::Point,
};
use eu4_harmony_abi as abi;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const ROUTE_QUERY: usize = 0x96c520;
const FREE_NODE: usize = 0x17394f0;
pub const MOVE: usize = 0xd50340;
pub const STOP: usize = 0xd4fcb0;

/// `MOVE` and `STOP` are declared by `events`, which observes them.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(ROUTE_QUERY, "4c 89 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55", "route query"),
        Point::keep(FREE_NODE, "e9 2b 45 02 00 cc cc cc cc cc cc cc cc cc cc cc", "route node free"),
    ]
}

/// Set after a native exception or an unsafe cleanup; routes stay off for the session.
static POISONED: AtomicBool = AtomicBool::new(false);
/// Thread currently mutating a route on behalf of an extension; the event hooks
/// ignore route changes made while this is set.
static OWN_MUTATION: AtomicU32 = AtomicU32::new(0);

pub fn own_mutation_thread() -> u32 {
    OWN_MUTATION.load(Ordering::SeqCst)
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Path {
    pub ids: Vec<u32>,
    pub nodes: Vec<usize>,
}

/// Reads a native route (see [`path`]).
pub fn read_path(m: &Memory, header: usize) -> GResult<Path> {
    let count = m.i32(header + path::COUNT)?;
    if !(0..=50_000).contains(&count) || m.u8(header + path::CHANGING)? != 0 {
        return Err("invalid or pending path header".to_string().into());
    }
    let mut node = m.ptr(header + path::HEAD)?;
    let mut previous = 0;
    let mut seen = BTreeSet::new();
    let mut path = Path::default();
    for _ in 0..count {
        if node == 0 || !seen.insert(node) {
            return Err("path cycle or null link".to_string().into());
        }
        if m.ptr(node + path::PREVIOUS)? != previous || m.u8(node + path::REMOVED)? != 0 {
            return Err("path back link or tombstone".to_string().into());
        }
        let id = m.i32(node + path::PROVINCE)?;
        if id <= 0 {
            return Err("invalid path province".to_string().into());
        }
        path.ids.push(id as u32);
        path.nodes.push(node);
        previous = node;
        node = m.ptr(node + path::NEXT)?;
    }
    if node != 0 || m.ptr(header + path::TAIL)? != previous {
        return Err("path tail mismatch".to_string().into());
    }
    Ok(path)
}

/// State of an army that may receive an order, captured to detect changes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OrderState {
    origin: u32,
    path: Path,
    identity: Vec<usize>,
}

/// Refuses armies the game would not let us order reliably: paused game,
/// battle, siege, retreat, movement lock, attachments or a native task.
fn preflight(m: &Memory, ctx: &Ctx, army: usize) -> GResult<OrderState> {
    if m.ptr(army)? != m.at(classes::ARMY) {
        return fail(abi::INVALID_ARGUMENT, "not a land army");
    }
    if paused(m, ctx)? {
        return fail(abi::NOT_READY, "game is paused");
    }
    let record = world::army_record(m, &super::special_countries(m)?, army)?;
    for (flag, reason) in [
        (abi::ARMY_IN_BATTLE, "army is in battle"),
        (abi::ARMY_SIEGING, "army is sieging"),
        (abi::ARMY_RETREATING, "army is retreating"),
        (abi::ARMY_EXILED, "army is exiled"),
        (abi::ARMY_MOVEMENT_LOCKED, "army movement is locked"),
        (abi::ARMY_ATTACHED, "army is attached to another army"),
        (abi::ARMY_HAS_FOLLOWERS, "other armies are attached to this army"),
        (abi::ARMY_BUSY, "army has a native task"),
    ] {
        if record.flags & flag != 0 {
            return fail(abi::REJECTED, reason);
        }
    }
    if record.regiments == 0 {
        return fail(abi::REJECTED, "army has no regiments");
    }
    let origin_ptr = m.ptr(army + unit::LOCATION)?;
    let origin = m.u32(origin_ptr + province::ID)?;
    if origin == 0 {
        return Err("army has no location".to_string().into());
    }
    let begin = m.ptr(army + unit::REGIMENTS)?;
    let end = m.ptr(army + unit::REGIMENTS + 8)?;
    span(begin, end, 8, MAX_UNITS)?;
    Ok(OrderState {
        origin,
        path: read_path(m, army + unit::PATH)?,
        identity: vec![ctx.world, army, m.u64(army + unit::ID)? as usize, ctx.player.0 as usize, ctx.date as usize, origin_ptr, begin, end],
    })
}

/// Route installed by an extension: valid while the army's current path is
/// an unconsumed suffix of it.
#[derive(Clone, Debug)]
struct Receipt {
    extension: usize,
    origin: u32,
    path: Path,
}

fn owned_suffix(origin: u32, original: &Path, current_origin: u32, current: &Path) -> bool {
    if current.ids.len() > original.ids.len() {
        return false;
    }
    let consumed = original.ids.len() - current.ids.len();
    let expected_origin = if consumed == 0 { origin } else { original.ids[consumed - 1] };
    current_origin == expected_origin && original.ids[consumed..] == current.ids && original.nodes[consumed..] == current.nodes
}

#[derive(Default)]
pub struct Orders {
    receipts: BTreeMap<u64, Receipt>,
}

struct OwnGuard;
impl OwnGuard {
    fn new() -> Self {
        OWN_MUTATION.store(crate::thread_id(), Ordering::SeqCst);
        OwnGuard
    }
}
impl Drop for OwnGuard {
    fn drop(&mut self) {
        OWN_MUTATION.store(0, Ordering::SeqCst);
    }
}

impl Orders {
    pub fn clear(&mut self) {
        self.receipts.clear();
    }

    /// Forgets the receipt when the route was changed by someone else.
    pub fn revoke(&mut self, army: u64) {
        self.receipts.remove(&army);
    }

    pub fn route_source(&self, m: &Memory, army: usize, id: u64, caller: usize) -> GResult<u32> {
        let path = read_path(m, army + unit::PATH)?;
        if path.ids.is_empty() {
            return Ok(abi::ROUTE_NONE);
        }
        let origin = world::location(m, army)?;
        Ok(match self.receipts.get(&id) {
            Some(r) if owned_suffix(r.origin, &r.path, origin, &path) => {
                if r.extension == caller {
                    abi::ROUTE_OWN
                } else {
                    abi::ROUTE_OTHER_EXTENSION
                }
            }
            _ => abi::ROUTE_EXTERNAL,
        })
    }

    /// Runs the game's route query into a temporary list, hands the result to
    /// `consume` and frees the list. The army's own route is never touched.
    fn with_route<T>(
        &mut self,
        m: &Memory,
        ctx: &Ctx,
        geography: &mut world::Geography,
        army: usize,
        goal: u32,
        consume: impl FnOnce(&Path, usize) -> GResult<T>,
    ) -> GResult<Option<T>> {
        if POISONED.load(Ordering::Acquire) {
            return fail(abi::UNAVAILABLE, "routes disabled after an earlier native failure");
        }
        let before = preflight(m, ctx, army)?;
        let origin = m.ptr(army + unit::LOCATION)?;
        let goal_address = geography.province_address(m, ctx, goal)?;
        let mut policy = m.at(globals::ROUTE_POLICY);
        // Options as the native "replace route" branch builds them.
        let mut options = [0u64; route_options::SIZE / 8];
        let options_ptr = options.as_mut_ptr() as usize;
        m.write(options_ptr + route_options::ONE_HOP, &[1])?;
        m.write(options_ptr + route_options::LIMIT, &(-1i32).to_le_bytes())?;
        m.write(options_ptr + route_options::TAG, &route_options::NO_TAG.to_le_bytes())?;
        let mut header = [0u64; path::SIZE / 8];
        let h = header.as_mut_ptr() as usize;
        let queried = m.call(m.at(ROUTE_QUERY), &[&mut policy as *mut usize as usize, army, origin, goal_address, h, options_ptr, 0, 0]);
        let result = (|| {
            let found = match queried {
                Ok(v) => v as u8 != 0,
                Err(e) => {
                    POISONED.store(true, Ordering::Release);
                    return Err(format!("native route query failed: {e}").into());
                }
            };
            let path = read_path(m, h)?;
            if preflight(m, ctx, army)? != before {
                return Err("route query changed the army".to_string().into());
            }
            if !found {
                return Ok(None);
            }
            let bad_endpoint = if goal == before.origin {
                !path.ids.is_empty()
            } else {
                path.ids.last() != Some(&goal) || path.ids.first() == Some(&before.origin)
            };
            if bad_endpoint {
                return Err("route endpoints do not match the query".to_string().into());
            }
            consume(&path, h).map(Some)
        })();
        // Free the temporary list, after making sure it shares no node with the army's route.
        let cleaned = (|| -> GResult<()> {
            let current = read_path(m, army + unit::PATH)?;
            let protected: BTreeSet<_> = before.path.nodes.iter().chain(current.nodes.iter()).copied().collect();
            let temp = read_path(m, h)?;
            if temp.nodes.iter().any(|n| protected.contains(n)) {
                return Err("temporary route shares nodes with the army route".to_string().into());
            }
            for node in temp.nodes {
                m.call(m.at(FREE_NODE), &[node, path::NODE_SIZE])?;
            }
            Ok(())
        })();
        if let Err(e) = cleaned {
            POISONED.store(true, Ordering::Release);
            return Err(format!("unsafe route cleanup: {}", e.message).into());
        }
        result
    }

    pub fn find_route(&mut self, m: &Memory, ctx: &Ctx, geography: &mut world::Geography, army: usize, goal: u32) -> GResult<Vec<u32>> {
        match self.with_route(m, ctx, geography, army, goal, |path, _| Ok(path.ids.clone()))? {
            Some(ids) => Ok(ids),
            None => fail(abi::NOT_FOUND, format!("no route to {goal}")),
        }
    }

    pub fn move_army(
        &mut self,
        m: &Memory,
        ctx: &Ctx,
        geography: &mut world::Geography,
        army: usize,
        id: u64,
        goal: u32,
        extension: usize,
    ) -> GResult<()> {
        let before = preflight(m, ctx, army)?;
        let installed = self.with_route(m, ctx, geography, army, goal, |route, header| {
            let _own = OwnGuard::new();
            if !route.ids.is_empty() {
                m.call(m.at(MOVE), &[army, header, 1, 0])?;
            } else if !before.path.ids.is_empty() {
                m.call(m.at(STOP), &[army, 0, 0])?;
            }
            let after = read_path(m, army + unit::PATH)?;
            if after.ids != route.ids {
                return Err("installed route differs from the queried route".to_string().into());
            }
            Ok(after)
        })?;
        let Some(path) = installed else {
            return fail(abi::NOT_FOUND, format!("no route to {goal}"));
        };
        if path.ids.is_empty() {
            self.receipts.remove(&id);
        } else {
            self.receipts.insert(id, Receipt { extension, origin: before.origin, path });
        }
        Ok(())
    }

    pub fn stop_army(&mut self, m: &Memory, ctx: &Ctx, army: usize, id: u64) -> GResult<()> {
        let before = preflight(m, ctx, army)?;
        if !before.path.ids.is_empty() {
            let _own = OwnGuard::new();
            m.call(m.at(STOP), &[army, 0, 0])?;
        }
        if !read_path(m, army + unit::PATH)?.ids.is_empty() {
            return Err("army still has a route after stop".to_string().into());
        }
        self.receipts.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(ids: &[u32], nodes: &[usize]) -> Path {
        Path { ids: ids.into(), nodes: nodes.into() }
    }

    #[test]
    fn suffix_ownership_requires_origin_and_exact_nodes() {
        let p = path(&[2, 3, 4], &[20, 30, 40]);
        assert!(owned_suffix(1, &p, 1, &p));
        assert!(owned_suffix(1, &p, 2, &path(&[3, 4], &[30, 40])));
        assert!(!owned_suffix(1, &p, 1, &path(&[3, 4], &[30, 40])));
        assert!(!owned_suffix(1, &p, 2, &path(&[3, 4], &[300, 400])));
        assert!(owned_suffix(1, &path(&[2, 3], &[20, 30]), 3, &path(&[], &[])));
        assert!(!owned_suffix(1, &path(&[2], &[20]), 1, &path(&[2, 3], &[20, 30])));
    }

    #[test]
    fn reads_linked_path_and_rejects_corruption() {
        let m = Memory { base: 0 };
        let mut header = [0u64; 3];
        let mut first = [0u64; 4];
        let mut second = [0u64; 4];
        let h = header.as_mut_ptr() as usize;
        let a = first.as_mut_ptr() as usize;
        let b = second.as_mut_ptr() as usize;
        let put = |p: usize, v: u64| m.write(p, &v.to_le_bytes()).unwrap();
        put(h, a as u64);
        put(h + 8, b as u64);
        put(h + 16, 2);
        put(a, 2);
        put(a + 16, b as u64);
        put(b, 3);
        put(b + 8, a as u64);
        assert_eq!(read_path(&m, h).unwrap(), path(&[2, 3], &[a, b]));
        put(b + 8, 0);
        assert!(read_path(&m, h).is_err());
        put(b + 8, a as u64);
        m.write(b + 0x18, &[1]).unwrap();
        assert!(read_path(&m, h).is_err());
    }
}
