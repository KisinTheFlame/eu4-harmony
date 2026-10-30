//! When extensions run, and what they hear about.
//!
//! * The map manager's frame (also while paused) is the safe point in a game
//!   (`crate::safe_point`): the core dispatches every queued event and
//!   `on_frame` there, and remote control runs.
//! * Visibility is "current" only after a complete fog-of-war refresh with no
//!   daily update or external change since; `on_day` waits for that.
//! * Route changes and defeats of own armies are observed in the game's own
//!   functions and queued: a move or stop of the game's that left an own
//!   army's route as it was is not one. Changes made by the core itself are
//!   ignored.
//!
//! Hook callbacks only touch `BOUNDARY` and their thread's `ROUTES`; they
//! never take the runtime state, since some fire inside game functions that
//! capabilities call. The one exception is the end of the daily update, which
//! no capability runs: there `time` pauses the game at a date remote control
//! advances it to.
use super::{context, context_any, fail, orders, Ctx, GResult, Status, Tag};
use crate::{
    hooks::HookFrame,
    memory::Memory,
    objects::campaign::{battle, classes, list, side, unit, world},
    points::Point,
    Result,
};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Mutex;

const MANAGER_FRAME: usize = 0x8137c0;
const FOW_REFRESH: usize = 0x8275e0;
const VISIBILITY: usize = 0xa0b150;
const DAILY: usize = 0x758430;
const DEFEAT_A: usize = 0x22bea0;
const DEFEAT_B: usize = 0x22b5a0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Ordered(u64),
    Defeated(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Proof {
    session: u64,
    world: usize,
    date: i32,
    player: u64,
    revision: u64,
    thread: u32,
    fog: u8,
}

#[derive(Default)]
struct Boundary {
    session: u64,
    revision: u64,
    depth: u32,
    refresh_thread: u32,
    before: Option<(Ctx, u8, u64)>,
    proof: Option<Proof>,
    changes: Vec<Change>,
    identity: Option<(usize, u64)>,
}

impl Boundary {
    fn invalidate(&mut self) {
        self.revision += 1;
        self.proof = None;
    }
    fn rotate(&mut self) {
        self.session += 1;
        self.invalidate();
        self.changes.clear();
    }
}

static BOUNDARY: Mutex<Option<Boundary>> = Mutex::new(None);

fn boundary<T>(f: impl FnOnce(&mut Boundary) -> T) -> T {
    let mut guard = BOUNDARY.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Boundary::default))
}

pub fn load_begin() {
    boundary(Boundary::rotate);
}

fn fog_mode(m: &Memory, ctx: &Ctx) -> Result<u8> {
    m.u8(ctx.world + world::FOG)
}

extern "C" fn fow_before(_: *mut HookFrame) {
    crate::guarded("fog refresh", || {
        let Some(m) = crate::memory() else { return };
        boundary(|b| {
            b.depth += 1;
            if b.depth != 1 {
                if b.refresh_thread != crate::thread_id() {
                    b.invalidate();
                }
                return;
            }
            b.invalidate();
            b.refresh_thread = crate::thread_id();
            b.before = context_any(m).ok().and_then(|c| fog_mode(m, &c).ok().map(|fog| (c, fog, b.revision)));
        });
    });
}

extern "C" fn fow_after(frame: *mut HookFrame) {
    crate::guarded("fog refresh", || {
        let Some(m) = crate::memory() else { return };
        let fault = unsafe { (*frame).fault } != 0;
        boundary(|b| {
            b.depth = b.depth.saturating_sub(1);
            if b.depth != 0 {
                return;
            }
            let before = b.before.take();
            let thread = std::mem::take(&mut b.refresh_thread);
            if fault {
                b.invalidate();
                return;
            }
            let Some((before, fog, revision)) = before else { return };
            let Ok(after) = context_any(m) else { return };
            if before == after && revision == b.revision && fog_mode(m, &after).ok() == Some(fog) {
                b.proof = Some(Proof { session: b.session, world: after.world, date: after.date, player: after.player.0, revision: b.revision, thread, fog });
            }
        });
    });
}

extern "C" fn visibility_before(_: *mut HookFrame) {
    crate::guarded("visibility", || {
        boundary(|b| {
            if b.depth == 0 || b.refresh_thread != crate::thread_id() {
                b.invalidate();
            }
        })
    });
}

extern "C" fn daily_before(_: *mut HookFrame) {
    crate::guarded("daily update", || boundary(Boundary::invalidate));
}

/// The day is over: time stops here when remote control advances it to this
/// date (`time`).
extern "C" fn daily_after(frame: *mut HookFrame) {
    crate::guarded("daily update", || {
        if let (Some(m), 0) = (crate::memory(), unsafe { (*frame).fault }) {
            super::time::day_passed(m);
        }
    });
}

fn record(change: Change) {
    boundary(|b| {
        b.invalidate();
        if b.changes.len() < 10_000 {
            b.changes.push(change);
        }
    });
}

/// Raises a change the game's own functions did not report, such as the
/// player's order a remote client gave; extensions hear it at the next safe
/// frame, as if a hook had seen it.
pub fn raise(change: Change) {
    record(change);
}

/// Own land army at `address`, if it is one.
fn own_army_id(m: &Memory, address: usize) -> Option<u64> {
    let ctx = context_any(m).ok()?;
    if address == 0 || m.ptr(address).ok()? != m.at(classes::ARMY) || m.u64(address + unit::OWNER).ok()? != ctx.player.0 {
        return None;
    }
    m.u64(address + unit::ID).ok()
}

/// The own army a move or stop of the game's is for, and its route before
/// the call, when it could be read.
type Watched = (u64, Option<orders::Path>);

thread_local! {
    /// For each move or stop of the game's still running, innermost last:
    /// its observer slot, and what it is watched for, or nothing when it is
    /// not.
    static ROUTES: RefCell<Vec<(usize, Option<Watched>)>> = const { RefCell::new(Vec::new()) };
}

/// The game moves or stops an army: an own army's route is kept, to see
/// afterwards whether the call changed it.
extern "C" fn route_before(frame: *mut HookFrame) {
    crate::guarded("route change", || {
        let h = unsafe { &*frame };
        let watched = (|| {
            let m = crate::memory()?;
            if orders::own_mutation_thread() == crate::thread_id() || crate::saves::load_in_progress() {
                return None;
            }
            let id = own_army_id(m, h.args[0])?;
            Some((id, orders::read_path(m, h.args[0] + unit::PATH).ok()))
        })();
        ROUTES.with(|r| r.borrow_mut().push((h.slot, watched)));
    });
}

/// An order is heard when the call left the army on another route, not when
/// the game takes it: the player's order is carried out here, a frame after
/// the game took it, and one the game takes but cannot carry out (a move onto
/// the sea with no fleet there) changes nothing (docs/research/army-orders.md).
extern "C" fn route_after(frame: *mut HookFrame) {
    crate::guarded("route change", || {
        let h = unsafe { &*frame };
        let entry = ROUTES.with(|r| {
            let mut r = r.borrow_mut();
            if r.last().is_some_and(|(slot, _)| *slot == h.slot) {
                r.pop()
            } else {
                None
            }
        });
        let Some((_, Some((id, before)))) = entry else { return };
        let after = (h.fault == 0).then(crate::memory).flatten().and_then(|m| orders::read_path(m, h.args[0] + unit::PATH).ok());
        if route_changed(before.as_ref(), after.as_ref()) {
            record(Change::Ordered(id));
        }
    });
}

/// Whether a move or stop changed the army's route: its provinces or its
/// nodes, which the game makes anew each time it sets a route, the same
/// provinces too. A route that could not be read counts as changed.
fn route_changed(before: Option<&orders::Path>, after: Option<&orders::Path>) -> bool {
    !matches!((before, after), (Some(b), Some(a)) if b == a)
}

/// Battle resolution: args are (battle, winner side, loser side, side index).
extern "C" fn defeat_before(frame: *mut HookFrame) {
    crate::guarded("battle defeat", || {
        let Some(m) = crate::memory() else { return };
        if crate::saves::load_in_progress() {
            return;
        }
        let a = unsafe { (*frame).args };
        let result = (|| -> Result<Vec<u64>> {
            let ctx = context_any(m).map_err(|e| e.message)?;
            let index = a[3] & 255;
            if index > 1 || m.u8(a[0] + battle::SETTLED)? != 0 || m.ptr(a[0] + battle::SIDES[index])? != a[1] || m.ptr(a[0] + battle::SIDES[1 - index])? != a[2]
            {
                return Ok(Vec::new());
            }
            let count = m.i32(a[2] + side::COUNT)?;
            if !(0..=1024).contains(&count) {
                return Err("defeated side roster bound".into());
            }
            let mut node = m.ptr(a[2] + side::UNITS)?;
            let mut seen = BTreeSet::new();
            let mut ids = Vec::new();
            while node != 0 {
                if seen.len() >= count as usize || !seen.insert(node) {
                    return Err("defeated side roster cycle".into());
                }
                let army = m.ptr(node + list::ITEM)?;
                if m.ptr(army)? != m.at(classes::ARMY) {
                    return Ok(Vec::new());
                }
                if Tag(m.u64(army + unit::OWNER)?) == ctx.player {
                    ids.push(m.u64(army + unit::ID)?);
                }
                node = m.ptr(node + list::NEXT)?;
            }
            Ok(ids)
        })();
        match result {
            Ok(ids) => ids.into_iter().for_each(|id| record(Change::Defeated(id))),
            Err(e) => log!("events: {e}"),
        }
    });
}

extern "C" fn manager_after(frame: *mut HookFrame) {
    crate::guarded("frame", || {
        if unsafe { (*frame).fault } == 0 {
            crate::safe_point::game_frame();
        }
    });
}

/// The frame, fog, daily and command functions that drive events.
pub fn points() -> Vec<Point> {
    vec![
        Point::observe(FOW_REFRESH, "48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57", "fog of war refresh", 0, Some(fow_before), Some(fow_after)),
        Point::observe(VISIBILITY, "48 89 5c 24 10 57 48 83 ec 20 4c 8b 41 28 0f b6", "visibility update", 2, Some(visibility_before), None),
        Point::observe(DAILY, "48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48", "daily update", 1, Some(daily_before), Some(daily_after)),
        Point::observe(orders::MOVE, "48 89 5c 24 20 44 88 44 24 18 48 89 54 24 10 55", "army move", 4, Some(route_before), Some(route_after)),
        Point::observe(orders::STOP, "48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57", "army stop", 3, Some(route_before), Some(route_after)),
        Point::observe(DEFEAT_A, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 57", "battle defeat", 4, Some(defeat_before), None),
        Point::observe(DEFEAT_B, "44 88 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55", "battle defeat", 4, Some(defeat_before), None),
        Point::observe(MANAGER_FRAME, "48 8b c4 88 50 10 55 53 56 57 41 54 41 55 41 56", "map manager frame", 2, None, Some(manager_after)),
    ]
}

/// What a frame needs to know before dispatching.
pub struct FrameState {
    pub ctx: Ctx,
    pub session: u64,
    pub visibility_current: bool,
    pub changes: Vec<Change>,
}

/// Called at the start of a safe frame: tracks the session and collects
/// changes. Fails when the frame is not one: while a save loads, off the main
/// thread, or when the campaign cannot be read.
pub fn begin_frame(m: &Memory) -> GResult<FrameState> {
    if crate::saves::load_in_progress() {
        return fail(Status::NotReady, "a save is being loaded");
    }
    if crate::thread_id() != crate::main_thread() {
        return fail(Status::WrongContext, "off the game's main thread");
    }
    let ctx = context(m)?;
    let fog = fog_mode(m, &ctx)?;
    Ok(boundary(|b| {
        let identity = (ctx.world, ctx.player.0);
        if b.identity != Some(identity) {
            b.identity = Some(identity);
            b.rotate();
        }
        FrameState { ctx, session: b.session, visibility_current: proof_valid(b, &ctx, fog), changes: std::mem::take(&mut b.changes) }
    }))
}

fn proof_valid(b: &Boundary, ctx: &Ctx, fog: u8) -> bool {
    b.proof.is_some_and(|p| {
        b.depth == 0
            && p.session == b.session
            && p.revision == b.revision
            && p.thread == crate::thread_id()
            && p.world == ctx.world
            && p.date == ctx.date
            && p.player == ctx.player.0
            && p.fog == fog
    })
}

/// Whether visibility reflects the current state, for capability calls.
pub fn visibility_current(m: &Memory, ctx: &Ctx) -> bool {
    let Ok(fog) = fog_mode(m, ctx) else { return false };
    boundary(|b| proof_valid(b, ctx, fog))
}

pub fn session() -> u64 {
    boundary(|b| b.session)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(ids: &[u32], nodes: &[usize]) -> orders::Path {
        orders::Path { ids: ids.into(), nodes: nodes.into() }
    }

    #[test]
    fn an_order_is_heard_only_when_the_route_changed() {
        let route = path(&[2754, 219], &[20, 30]);
        // The game called the function and left the route as it was.
        assert!(!route_changed(Some(&route), Some(&route.clone())));
        // The same provinces through new nodes: the route was set again.
        assert!(route_changed(Some(&route), Some(&path(&[2754, 219], &[40, 50]))));
        assert!(route_changed(Some(&route), Some(&path(&[2754], &[20]))));
        assert!(route_changed(Some(&path(&[], &[])), Some(&route)));
        // A route that could not be read counts as changed.
        assert!(route_changed(None, Some(&route)));
        assert!(route_changed(Some(&route), None));
    }
}
