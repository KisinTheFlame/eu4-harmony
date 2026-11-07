//! Route queries, moving and stopping own land armies with the game's move
//! command (ADR commands-run-at-once), and the record of which extension set
//! which route.
use super::{armies::RouteSource, commands::Commands, events::Change, fail, paused, span, world, Caller, Ctx, GResult, Session, Status, MAX_UNITS};
use crate::{
    memory::Memory,
    objects::{
        campaign::{classes, geometry, globals, province, route_options, unit},
        list,
    },
    points::{GameFunction, Point},
};
use eu4_harmony_checks::armies as checks;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

pub const ROUTE_QUERY: GameFunction<8> = GameFunction::at(0x96c520);
pub const MOVE: usize = 0xd50340;
pub const STOP: usize = 0xd4fcb0;

/// `MOVE` and `STOP` are declared by `events`, which observes them.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(ROUTE_QUERY, "4c 89 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55", "route query"),
        Point::keep(TRANSPORTS, "48 8b c4 4c 89 40 18 48 89 50 10 53 55 56 57 41 54 41 55 41 56 41 57", "fleets that may carry an army"),
        Point::keep(MAY_CARRY, "48 89 5c 24 18 48 89 74 24 20 57 48 83 ec 20 41 8b f0 48 8b da 48 8b f9", "fleet may take an army aboard"),
        Point::keep(SHIP_ROOM, "48 89 5c 24 18 48 89 74 24 20 55 57 41 54 41 56 41 57 48 8d 6c 24 c9 48 81 ec e0 00 00 00", "regiments a ship carries"),
    ]
}

/// Whether routes are off, and for how long ([`ROUTES_ON`], [`OFF_FOR_THE_WORLD`],
/// [`OFF_FOR_THE_PROCESS`]): the longer of the two once both were set.
static POISONED: AtomicU8 = AtomicU8::new(ROUTES_ON);
const ROUTES_ON: u8 = 0;
/// A route's cleanup was unsafe: the temporary route shared nodes with the
/// army's own, or could not be read or freed, so it was left as it was. What
/// that leaves lies in the armies of the world it was asked in, which a load
/// or a world reset replaces: routes come back then
/// ([`world_replaced`]), though not at a change of country, which keeps them.
const OFF_FOR_THE_WORLD: u8 = 1;
/// The game's route query faulted. What it left half done may lie wherever
/// it reaches, the game's heap and caches outside the world among it, and
/// nothing shows that a load rebuilds that: routes stay off until the game
/// starts again.
const OFF_FOR_THE_PROCESS: u8 = 2;

fn poison(how_long: u8) {
    POISONED.fetch_max(how_long, Ordering::AcqRel);
}

/// The game's world was replaced (a save began to be read, or the world was
/// reset): routes off for the world they failed in come back.
pub fn world_replaced() {
    let _ = POISONED.compare_exchange(OFF_FOR_THE_WORLD, ROUTES_ON, Ordering::AcqRel, Ordering::Acquire);
}

/// Why routes are off, if they are.
fn routes_off() -> Option<&'static str> {
    match POISONED.load(Ordering::Acquire) {
        ROUTES_ON => None,
        OFF_FOR_THE_WORLD => {
            Some("routes are off until another save is loaded or the game returns to the main menu: an earlier route could not be cleaned up safely")
        }
        _ => Some("routes are off until the game starts again: the game's route query faulted earlier"),
    }
}
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

/// Reads a native route (see [`list`]).
pub fn read_path(m: &Memory, header: usize) -> GResult<Path> {
    let count = m.i32(header + list::COUNT)?;
    if !(0..=50_000).contains(&count) || m.u8(header + list::CHANGING)? != 0 {
        return Err("invalid or pending path header".to_string().into());
    }
    let mut node = m.ptr(header + list::HEAD)?;
    let mut previous = 0;
    let mut seen = BTreeSet::new();
    let mut path = Path::default();
    for _ in 0..count {
        if node == 0 || !seen.insert(node) {
            return Err("path cycle or null link".to_string().into());
        }
        if m.ptr(node + list::PREVIOUS)? != previous || m.u8(node + list::REMOVED)? != 0 {
            return Err("path back link or tombstone".to_string().into());
        }
        let id = m.i32(node + list::ITEM)?;
        if id <= 0 {
            return Err("invalid path province".to_string().into());
        }
        path.ids.push(id as u32);
        path.nodes.push(node);
        previous = node;
        node = m.ptr(node + list::NEXT)?;
    }
    if node != 0 || m.ptr(header + list::TAIL)? != previous {
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

/// Refuses armies the game would not let us order reliably: battle, siege,
/// retreat, movement lock, attachments or a native task; and, for an
/// extension, a paused game. An extension's routes and orders wait for the
/// game to run, as regional-combat's always have; remote control's are the
/// player's, which the player gives while the game is paused too (a paused
/// right-click on the map queries the route and submits it as these do,
/// docs/research/army-orders.md).
fn preflight(m: &Memory, ctx: &Ctx, army: usize, caller: Caller) -> GResult<OrderState> {
    let class = m.ptr(army)?;
    if class != m.at(classes::ARMY) && class != m.at(classes::FLEET) {
        return Err(checks::not_land().into());
    }
    if waits_for_time(caller) && paused(m, ctx)? {
        return Err(checks::paused().into());
    }
    // A fleet's own state is the game's move command's to check.
    if class == m.at(classes::ARMY) {
        checks::orderable(&world::army_record(m, &super::special_countries(m)?, army)?.condition())?;
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

/// Whether the caller's routes and orders wait for the game to run
/// ([`preflight`]).
fn waits_for_time(caller: Caller) -> bool {
    matches!(caller, Caller::Extension(_))
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

    pub fn route_source(&self, m: &Memory, army: usize, id: u64, caller: Caller) -> GResult<RouteSource> {
        let path = read_path(m, army + unit::PATH)?;
        if path.ids.is_empty() {
            return Ok(RouteSource::None);
        }
        let origin = world::location(m, army)?;
        Ok(self.source(id, origin, &path, caller))
    }

    /// Who set the army `id`'s route `path`, which it follows from `origin`,
    /// as `caller` sees it.
    fn source(&self, id: u64, origin: u32, path: &Path, caller: Caller) -> RouteSource {
        if path.ids.is_empty() {
            return RouteSource::None;
        }
        match self.receipts.get(&id) {
            Some(r) if owned_suffix(r.origin, &r.path, origin, path) => {
                if caller == Caller::Extension(r.extension) {
                    RouteSource::Own
                } else {
                    RouteSource::OtherExtension
                }
            }
            _ => RouteSource::External,
        }
    }

    /// Records an order that was carried out, which left the army `id` on
    /// `path` from `origin`. An extension's route stays its own until someone
    /// else changes it. A remote client's order is the player's: the army is
    /// taken back from any extension, and the player's order is returned, to
    /// be raised once.
    fn settle(&mut self, caller: Caller, id: u64, origin: u32, path: Path) -> Option<Change> {
        match caller {
            Caller::Extension(extension) if !path.ids.is_empty() => {
                self.receipts.insert(id, Receipt { extension, origin, path });
                None
            }
            Caller::Extension(_) => {
                self.receipts.remove(&id);
                None
            }
            Caller::Remote => {
                self.receipts.remove(&id);
                Some(Change::Ordered(id))
            }
        }
    }

    /// Runs the game's route query into a temporary list, hands the result to
    /// `consume` and frees the list. The army's own route is never touched.
    #[expect(clippy::too_many_arguments, reason = "the game, its campaign and map, the army, its goal and caller, and what to do with the route")]
    fn with_route<T>(
        &mut self,
        m: &Memory,
        ctx: &Ctx,
        geography: &mut world::Geography,
        army: usize,
        goal: u32,
        caller: Caller,
        consume: impl FnOnce(&Path, usize) -> GResult<T>,
    ) -> GResult<Option<T>> {
        if let Some(why) = routes_off() {
            return fail(Status::Unavailable, why);
        }
        let before = preflight(m, ctx, army, caller)?;
        let origin = m.ptr(army + unit::LOCATION)?;
        let goal_address = geography.province_address(m, ctx, goal)?;
        let mut policy = m.at(globals::ROUTE_POLICY);
        // Options as the native "replace route" branch builds them.
        let mut options = [0u64; route_options::SIZE / 8];
        let options_ptr = options.as_mut_ptr() as usize;
        m.write(options_ptr + route_options::ONE_HOP, &[1])?;
        m.write(options_ptr + route_options::LIMIT, &(-1i32).to_le_bytes())?;
        m.write(options_ptr + route_options::TAG, &route_options::NO_TAG.to_le_bytes())?;
        let mut header = [0u64; list::SIZE / 8];
        let h = header.as_mut_ptr() as usize;
        let queried = ROUTE_QUERY.call(m, [&mut policy as *mut usize as usize, army, origin, goal_address, h, options_ptr, 0, 0]);
        let result = (|| {
            let found = match queried {
                Ok(v) => v as u8 != 0,
                Err(e) => {
                    poison(OFF_FOR_THE_PROCESS);
                    return Err(format!("native route query failed: {e}").into());
                }
            };
            let path = read_path(m, h)?;
            if preflight(m, ctx, army, caller)? != before {
                return Err("route query changed the army".to_string().into());
            }
            if !found {
                return Ok(None);
            }
            let bad_endpoint =
                if goal == before.origin { !path.ids.is_empty() } else { path.ids.last() != Some(&goal) || path.ids.first() == Some(&before.origin) };
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
                crate::game_heap::delete(m, node, list::NODE_SIZE)?;
            }
            Ok(())
        })();
        if let Err(e) = cleaned {
            poison(OFF_FOR_THE_WORLD);
            return Err(format!("unsafe route cleanup: {}", e.message).into());
        }
        result
    }

    pub fn find_route(&mut self, m: &Memory, ctx: &Ctx, geography: &mut world::Geography, army: usize, goal: u32, caller: Caller) -> GResult<Vec<u32>> {
        match self.with_route(m, ctx, geography, army, goal, caller, |path, _| Ok(path.ids.clone()))? {
            Some(ids) => Ok(ids),
            None => Err(checks::no_route(goal).into()),
        }
    }

    /// Moves the army to `goal` with the game's move command, which takes
    /// the game's route there, as the player's right click on the map does;
    /// refused when the game finds none. To the province it is in, it stops
    /// the army, as the right click there does. Returns the player's order
    /// when the caller acts as the player.
    pub fn move_army(&mut self, session: &Session, geography: &mut world::Geography, army: usize, id: u64, goal: u32) -> GResult<Option<Change>> {
        let (m, ctx, caller) = (session.m, &session.ctx, session.caller);
        let before = preflight(m, ctx, army, caller)?;
        if goal == before.origin {
            return self.stop_army(session, army, id);
        }
        // The route query the command makes, asked first: the command does
        // nothing, and says nothing, when it finds no route.
        if self.with_route(m, ctx, geography, army, goal, caller, |_, _| Ok(()))?.is_none() {
            let sea = geography.province_address(m, ctx, goal)?;
            return Err(boarding_refused(m, ctx, army, sea, goal)?.unwrap_or_else(|| checks::no_route(goal)).into());
        }
        order(session.commands, &move_text(m.u64(army + unit::ID)?, before.origin, goal))?;
        let after = read_path(m, army + unit::PATH)?;
        if after.ids.last() != Some(&goal) {
            return Err("the game's move command left the army without a route to its goal".to_string().into());
        }
        Ok(self.settle(caller, id, before.origin, after))
    }

    /// Stops the army where it is, with the game's command that cancels its
    /// movement, as the player's right click on the province it is in does.
    /// Returns the player's order when the caller acts as the player.
    pub fn stop_army(&mut self, session: &Session, army: usize, id: u64) -> GResult<Option<Change>> {
        let (m, caller) = (session.m, session.caller);
        let before = preflight(m, &session.ctx, army, caller)?;
        if !before.path.ids.is_empty() {
            order(session.commands, &stop_text(m.u64(army + unit::ID)?))?;
        }
        if !read_path(m, army + unit::PATH)?.ids.is_empty() {
            return Err("army still has a route after stop".to_string().into());
        }
        Ok(self.settle(caller, id, before.origin, Path::default()))
    }
}

/// `(fleet, army, regiments)`: whether the fleet may take the army aboard,
/// with room for `regiments` (all the army's when negative) beside those
/// aboard already: the route query's test of a step onto the sea, made for
/// each fleet there (docs/research/units.md).
const MAY_CARRY: GameFunction<3> = GameFunction::at(0x90acf0);
/// `(ship) → i32`: the regiments a ship carries.
const SHIP_ROOM: GameFunction<1> = GameFunction::at(0xae1a40);

/// Why a land army finds no route onto the sea zone at `sea`, when that is
/// the boarding: the player has no fleet there, or none with room for the
/// whole army, which a step onto the sea needs. None when a fleet there may
/// take it, or `sea` is land, or the unit a fleet.
fn boarding_refused(m: &Memory, ctx: &Ctx, army: usize, sea: usize, goal: u32) -> GResult<Option<eu4_harmony_checks::Refusal>> {
    if m.ptr(army)? != m.at(classes::ARMY) || m.u8(m.ptr(sea + province::GEOMETRY)? + geometry::TERRAIN)? & geometry::LAND != 0 {
        return Ok(None);
    }
    let mut fleets = Vec::new();
    let mut node = m.ptr(sea + province::UNITS)?;
    while node != 0 && fleets.len() < MAX_UNITS {
        let unit = m.ptr(node + list::ITEM)?;
        if m.ptr(unit)? == m.at(classes::FLEET) && m.u64(unit + unit::OWNER)? == ctx.player.0 {
            fleets.push(unit);
        }
        node = m.ptr(node + list::NEXT)?;
    }
    let mut most = 0;
    for &fleet in &fleets {
        if MAY_CARRY.call(m, [fleet, army, -1i32 as u32 as usize])? as u8 != 0 {
            return Ok(None);
        }
        let (begin, end) = (m.ptr(fleet + unit::REGIMENTS)?, m.ptr(fleet + unit::REGIMENTS + 8)?);
        let mut room = 0i64;
        for i in 0..span(begin, end, 8, MAX_UNITS)? {
            room += SHIP_ROOM.call(m, [m.ptr(begin + i * 8)?])? as i32 as i64;
        }
        for aboard in world::aboard(m, fleet)? {
            room -= world::regiments(m, aboard)? as i64;
        }
        most = most.max(room.max(0) as u32);
    }
    Ok(Some(match fleets.is_empty() {
        true => checks::no_fleet_to_board(goal),
        false => checks::no_room_aboard(goal, world::regiments(m, army)?, most),
    }))
}

/// `(country, std::vector*, army)`: the fleets of the country's that may
/// carry the army, as the right click's automatic transport gathers them.
pub const TRANSPORTS: GameFunction<3> = GameFunction::at(0x342170);

/// Has the player's fleets carry an own land army to `goal`, as the right
/// click's automatic transport does once the player accepts its dialog: the
/// fleets that may carry it, and the game's transport command for them.
pub fn transport(session: &Session, army: usize, id: u64, goal: u32) -> GResult<()> {
    let (m, ctx) = (session.m, &session.ctx);
    let mut vector = [0u64; 3];
    let at = vector.as_mut_ptr() as usize;
    let called = TRANSPORTS.call(m, [ctx.country, at, army]);
    let (begin, end, capacity) = (vector[0] as usize, vector[1] as usize, vector[2] as usize);
    let fleets = (|| -> GResult<Vec<u64>> {
        called?;
        let count = span(begin, end, 8, MAX_UNITS)?;
        let mut ids = Vec::new();
        for i in 0..count {
            let fleet = m.ptr(begin + i * 8)?;
            if fleet != 0 && m.ptr(fleet)? == m.at(classes::FLEET) {
                ids.push(m.u64(fleet + unit::ID)?);
            }
        }
        Ok(ids)
    })();
    // The vector's buffer is the game's: freed as its own small blocks are.
    if begin != 0 && capacity > begin && capacity - begin < 0x1000 {
        crate::game_heap::delete(m, begin, capacity - begin)?;
    }
    let fleets = fleets?;
    if fleets.is_empty() {
        return Err(checks::no_transports().into());
    }
    order(session.commands, &transport_text(id, &fleets, goal))
}

/// The automatic transport command: the army, the fleets that carry it, and
/// where to.
fn transport_text(army: u64, fleets: &[u64], goal: u32) -> String {
    let navies: Vec<String> = fleets.iter().map(|f| format!("navy={}", unit_text(*f))).collect();
    format!("start_auto_fleet_transport={{ army={} {} province_id={goal} }}", unit_text(army), navies.join(" "))
}

/// The game's move command for the army with the ID `id`, in `origin`, to
/// `goal`, as the player's right click on another province makes it: the
/// army's route replaced, and its fleet's own orders cancelled
/// (docs/research/army-orders.md).
fn move_text(id: u64, origin: u32, goal: u32) -> String {
    format!("move_order={{ unit={} province={{ {goal} }} location={origin} clear=yes cancel_autofleet=yes }}", unit_text(id))
}

/// The game's command that cancels the movement of the army with the ID
/// `id`, as the player's right click on the province it is in makes it.
fn stop_text(id: u64) -> String {
    format!("cancel_movement={{ unit={} cancel_autofleet=yes reason=1 }}", unit_text(id))
}

/// An army's ID, as the game writes it: its number and its type.
fn unit_text(id: u64) -> String {
    format!("{{ id={} type={} }}", (id >> 32) as i32, id as u32 as i32)
}

/// Has the game carry out the army's command `text` through `commands` at
/// once, the route changes it makes the framework's own.
fn order(commands: Commands, text: &str) -> GResult<()> {
    let posted = commands.post_with(text, |run| {
        let _own = OwnGuard::new();
        run()
    });
    posted.map_err(|refused| super::action::refused(refused, "order the army there"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_off_for_their_world_come_back_with_another_and_those_off_for_the_process_do_not() {
        // No other test turns routes off.
        assert_eq!(routes_off(), None);
        poison(OFF_FOR_THE_WORLD);
        assert!(routes_off().unwrap().contains("until another save is loaded"));
        world_replaced();
        assert_eq!(routes_off(), None);
        poison(OFF_FOR_THE_PROCESS);
        poison(OFF_FOR_THE_WORLD);
        world_replaced();
        assert!(routes_off().unwrap().contains("until the game starts again"));
        POISONED.store(ROUTES_ON, Ordering::Release);
    }

    fn path(ids: &[u32], nodes: &[usize]) -> Path {
        Path { ids: ids.into(), nodes: nodes.into() }
    }

    #[test]
    fn an_order_names_the_army_by_its_number_and_type() {
        assert_eq!(move_text(12 << 32 | 54, 183, 185), "move_order={ unit={ id=12 type=54 } province={ 185 } location=183 clear=yes cancel_autofleet=yes }");
        assert_eq!(stop_text(12 << 32 | 54), "cancel_movement={ unit={ id=12 type=54 } cancel_autofleet=yes reason=1 }");
        assert_eq!(
            transport_text(12 << 32 | 54, &[20 << 32 | 54, 21 << 32 | 54], 236),
            "start_auto_fleet_transport={ army={ id=12 type=54 } navy={ id=20 type=54 } navy={ id=21 type=54 } province_id=236 }"
        );
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
    fn only_an_extension_s_orders_wait_for_a_paused_game() {
        assert!(waits_for_time(Caller::Extension(0)));
        assert!(!waits_for_time(Caller::Remote));
    }

    #[test]
    fn a_remote_order_is_the_players() {
        let mut orders = Orders::default();
        let route = path(&[2, 3], &[20, 30]);
        let first = Caller::Extension(0);

        assert_eq!(orders.settle(first, 7, 1, route.clone()), None);
        assert_eq!(orders.source(7, 1, &route, first), RouteSource::Own);
        assert_eq!(orders.source(7, 1, &route, Caller::Extension(1)), RouteSource::OtherExtension);
        assert_eq!(orders.source(7, 1, &route, Caller::Remote), RouteSource::OtherExtension);

        // A remote client moves the army: it is taken back from the extension,
        // and one player's order comes out.
        let remote = path(&[4], &[40]);
        assert_eq!(orders.settle(Caller::Remote, 7, 1, remote.clone()), Some(Change::Ordered(7)));
        assert!(orders.receipts.is_empty());
        for caller in [first, Caller::Extension(1), Caller::Remote] {
            assert_eq!(orders.source(7, 1, &remote, caller), RouteSource::External);
        }

        // Stopping is an order too, whether or not anyone owned the army.
        assert_eq!(orders.settle(first, 7, 1, route.clone()), None);
        assert_eq!(orders.settle(Caller::Remote, 7, 1, Path::default()), Some(Change::Ordered(7)));
        assert_eq!(orders.settle(Caller::Remote, 8, 1, Path::default()), Some(Change::Ordered(8)));
        assert!(orders.receipts.is_empty());
        assert_eq!(orders.source(7, 1, &Path::default(), first), RouteSource::None);

        // An extension stopping its army gives the route up without an order.
        assert_eq!(orders.settle(first, 7, 1, route.clone()), None);
        assert_eq!(orders.settle(first, 7, 1, Path::default()), None);
        assert!(orders.receipts.is_empty());
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
