//! Reading provinces, armies and countries. Visibility rules follow the game:
//! units in fogged provinces are never read.
use super::{context, country, fail, special_countries, span, Ctx, GResult, Tag, MAX_PROVINCES, MAX_UNITS};
use crate::{
    memory::Memory,
    objects::campaign::{adjacency, classes, country as country_fields, geometry, globals, list, map, province, route_options, tag, task, unit, world},
    points::Point,
};
use eu4_harmony_abi as abi;
use std::collections::BTreeSet;

const CAN_ENTER: usize = 0x1a2280;
const MILITARY_TAG: usize = 0x1a8d20;
const STRAIT_BLOCKED: usize = 0x96f650;
const CAN_ATTACK: usize = 0xd54a60;
/// What the idle task answers: no task kind, and false.
const NO_KIND: usize = 0x8f480;
const NO: usize = 0x8f450;
const IDLE_ANSWERS: [usize; 3] = [NO_KIND, NO, NO];

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CAN_ENTER, "48 89 5c 24 08 48 89 6c 24 10 56 57 41 56 48 83", "army access query"),
        Point::keep(MILITARY_TAG, "48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9", "army military tag"),
        Point::keep(STRAIT_BLOCKED, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48", "strait blockade query"),
        Point::keep(CAN_ATTACK, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 80", "attack permission query"),
        Point::keep(NO_KIND, "33 c0 c3", "no task kind"),
        Point::keep(NO, "32 c0 c3", "false"),
        Point::method(classes::ARMY, unit::CAN_ENTER, CAN_ENTER, "army access method"),
        Point::method(classes::ARMY, unit::MILITARY_TAG, MILITARY_TAG, "army military tag method"),
        Point::method(classes::IDLE_TASK, task::IDLE_METHODS[0], IDLE_ANSWERS[0], "idle task method"),
        Point::method(classes::IDLE_TASK, task::IDLE_METHODS[1], IDLE_ANSWERS[1], "idle task method"),
        Point::method(classes::IDLE_TASK, task::IDLE_METHODS[2], IDLE_ANSWERS[2], "idle task method"),
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edge {
    to: u32,
    through: i32,
    kind: i32,
    address: usize,
}

/// Immutable map data of one province. Ownership, units and fog are never cached.
#[derive(Clone, Debug)]
struct Node {
    id: u32,
    address: usize,
    geometry: usize,
    land: bool,
    lake: bool,
    impassable: bool,
    adjacent: usize,
    adjacent_end: usize,
    edges: Vec<Edge>,
}
impl Node {
    fn admissible(&self) -> bool {
        self.id > 0 && self.land && !self.lake && !self.impassable
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MapKey {
    world: usize,
    map: usize,
    first: usize,
    count: usize,
}

/// Static topology, rebuilt when the world or province array changes and
/// cleared on every session change.
#[derive(Default)]
pub struct Geography {
    entry: Option<(MapKey, Vec<Node>)>,
}

fn le_u32(bytes: &[u8], offset: usize) -> GResult<u32> {
    Ok(u32::from_le_bytes(bytes.get(offset..offset + 4).ok_or_else(|| "truncated read".to_string())?.try_into().unwrap()))
}

fn decode_edges(bytes: &[u8], id: usize, total: usize, address: usize) -> GResult<Vec<Edge>> {
    if bytes.len() % adjacency::SIZE != 0 || bytes.len() / adjacency::SIZE > total {
        return Err("adjacency vector bound".to_string().into());
    }
    let mut edges = Vec::with_capacity(bytes.len() / adjacency::SIZE);
    for (j, record) in bytes.chunks_exact(adjacency::SIZE).enumerate() {
        let to = le_u32(record, adjacency::TO)? as i32;
        let from = le_u32(record, adjacency::FROM)? as i32;
        let kind = le_u32(record, adjacency::KIND)? as i32;
        if from != id as i32 || to < 0 || to as usize >= total || to == from || !(adjacency::LAND..=adjacency::CANAL).contains(&kind) {
            return Err("invalid adjacency record".to_string().into());
        }
        let through = le_u32(record, adjacency::THROUGH)? as i32;
        edges.push(Edge { to: to as u32, through, kind, address: address + j * adjacency::SIZE });
    }
    Ok(edges)
}

/// A connection a land army can take: plain, across a strait or over a river.
fn by_land(kind: i32) -> bool {
    (adjacency::LAND..=adjacency::RIVER).contains(&kind)
}

fn read_node(m: &Memory, first: usize, id: usize, count: usize) -> GResult<Node> {
    let address = first + id * province::SIZE;
    if m.u32(address + province::ID)? != id as u32 {
        return Err("province ID does not match its index".to_string().into());
    }
    let geometry = m.ptr(address + province::GEOMETRY)?;
    if geometry == 0 {
        return Err("province without geometry".to_string().into());
    }
    if m.u32(geometry + geometry::ID)? != id as u32 {
        return Err("geometry ID mismatch".to_string().into());
    }
    let adjacent = m.ptr(geometry + geometry::ADJACENT)?;
    let adjacent_end = m.ptr(geometry + geometry::ADJACENT + 8)?;
    let bytes = adjacent_end.checked_sub(adjacent).ok_or_else(|| "reversed adjacency".to_string())?;
    let edges = if bytes == 0 { Vec::new() } else { decode_edges(&m.bytes(adjacent, bytes)?, id, count, adjacent)? };
    let terrain = m.u8(geometry + geometry::TERRAIN)?;
    Ok(Node {
        id: id as u32,
        address,
        geometry,
        land: terrain & geometry::LAND != 0,
        lake: terrain & geometry::LAKE != 0,
        impassable: m.u8(geometry + geometry::CLIMATE)? & geometry::IMPASSABLE != 0,
        adjacent,
        adjacent_end,
        edges,
    })
}

/// Re-reads a cached node's identity and adjacency so a changed map is never
/// served from the cache.
fn validate_node(m: &Memory, n: &Node, count: usize) -> GResult<()> {
    let fresh = read_node(m, n.address - n.id as usize * province::SIZE, n.id as usize, count)?;
    if fresh.geometry != n.geometry
        || fresh.adjacent != n.adjacent
        || fresh.adjacent_end != n.adjacent_end
        || fresh.land != n.land
        || fresh.lake != n.lake
        || fresh.impassable != n.impassable
        || fresh.edges != n.edges
    {
        return Err("cached map data changed".to_string().into());
    }
    Ok(())
}

/// First province and count, cross-checked between the map and the world.
fn province_array(m: &Memory, ctx: &Ctx) -> GResult<(usize, usize)> {
    let map = m.ptr(m.at(globals::MAP))?;
    let first = m.ptr(map + map::PROVINCES)?;
    let end = m.ptr(map + map::PROVINCES + 8)?;
    let count = span(first, end, province::SIZE, MAX_PROVINCES)?;
    if count == 0 || m.ptr(ctx.world + world::PROVINCES)? != first {
        return Err("world and map province arrays differ".to_string().into());
    }
    Ok((first, count))
}

impl Geography {
    pub fn clear(&mut self) {
        self.entry = None;
    }

    fn nodes(&mut self, m: &Memory, ctx: &Ctx) -> GResult<&[Node]> {
        let (first, count) = province_array(m, ctx)?;
        let key = MapKey { world: ctx.world, map: m.ptr(m.at(globals::MAP))?, first, count };
        if self.entry.as_ref().map(|(k, _)| *k) != Some(key) {
            self.entry = None;
            let nodes = (0..count).map(|id| read_node(m, first, id, count)).collect::<GResult<Vec<_>>>()?;
            self.entry = Some((key, nodes));
        }
        Ok(&self.entry.as_ref().unwrap().1)
    }

    fn node(&mut self, m: &Memory, ctx: &Ctx, id: u32) -> GResult<Node> {
        let nodes = self.nodes(m, ctx)?;
        let count = nodes.len();
        let Some(node) = nodes.get(id as usize).cloned() else {
            return fail(abi::NOT_FOUND, format!("no province {id}"));
        };
        if let Err(e) = validate_node(m, &node, count) {
            self.clear();
            return Err(e);
        }
        Ok(node)
    }

    /// Address of a province object.
    pub fn province_address(&mut self, m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
        Ok(self.node(m, ctx, id)?.address)
    }

    pub fn provinces(&mut self, m: &Memory, ctx: &Ctx, visibility_current: bool) -> GResult<Vec<abi::Province>> {
        let nodes = self.nodes(m, ctx)?.to_vec();
        let mut out = Vec::with_capacity(nodes.len());
        for n in &nodes {
            let mut flags = 0;
            if n.land {
                flags |= abi::PROVINCE_LAND;
            }
            if n.lake {
                flags |= abi::PROVINCE_LAKE;
            }
            if n.impassable {
                flags |= abi::PROVINCE_IMPASSABLE;
            }
            if n.id == 0 {
                out.push(abi::Province::default());
                continue;
            }
            if visibility_current && !hidden(m, n.address)? {
                flags |= abi::PROVINCE_VISIBLE;
            }
            let owner = Tag(m.u64(n.address + province::OWNER)?);
            let controller = Tag(m.u64(n.address + province::CONTROLLER)?);
            out.push(abi::Province {
                id: n.id,
                flags,
                owner: if owner.valid() { owner.0 } else { 0 },
                controller: if controller.valid() { controller.0 } else { 0 },
            });
        }
        Ok(out)
    }

    /// Land connections between passable land provinces.
    pub fn neighbors(&mut self, m: &Memory, ctx: &Ctx, id: u32) -> GResult<Vec<abi::Neighbor>> {
        let node = self.node(m, ctx, id)?;
        if !node.admissible() {
            return Ok(Vec::new());
        }
        let nodes = self.nodes(m, ctx)?;
        Ok(node
            .edges
            .iter()
            .filter(|e| by_land(e.kind) && nodes.get(e.to as usize).is_some_and(Node::admissible))
            .map(|e| abi::Neighbor { to: e.to, flags: abi::NEIGHBOR_LAND | if e.kind == adjacency::STRAIT { abi::NEIGHBOR_STRAIT } else { 0 } })
            .collect())
    }

    /// Whether the army may cross from `from` to the neighbor `to`; for
    /// straits this asks the game about blockades by the army's military tag.
    pub fn strait_open(&mut self, m: &Memory, ctx: &Ctx, army: usize, from: u32, to: u32) -> GResult<bool> {
        let node = self.node(m, ctx, from)?;
        let Some(edge) = node.edges.iter().find(|e| e.to == to && by_land(e.kind)) else {
            return fail(abi::NOT_FOUND, format!("{to} is not a land neighbor of {from}"));
        };
        if edge.kind != adjacency::STRAIT {
            return Ok(true);
        }
        if edge.through != 0 {
            let nodes = self.nodes(m, ctx)?;
            if edge.through < 0 || !nodes.get(edge.through as usize).is_some_and(|n| !n.land && !n.lake) {
                return Err("strait crosses an invalid province".to_string().into());
            }
        }
        let military = military_tag(m, ctx, army)?;
        Ok(m.call(m.at(STRAIT_BLOCKED), &[military.0 as usize, edge.address])? as u8 == 0)
    }

    pub fn clear_if_changed(&mut self, m: &Memory, ctx: &Ctx) {
        if self.nodes(m, ctx).is_err() {
            self.clear();
        }
    }
}

/// Fog of war hides this province.
pub fn hidden(m: &Memory, province: usize) -> GResult<bool> {
    let fogged = m.u8(province + province::FOGGED)? & province::FOGGED_BIT != 0;
    Ok(fogged && m.u8(province + province::HIDDEN)? & province::HIDDEN_BIT != 0)
}

fn check_army_vtable(m: &Memory, army: usize) -> GResult<()> {
    let vt = m.ptr(army)?;
    if vt != m.at(classes::ARMY) || m.ptr(vt + unit::CAN_ENTER)? != m.at(CAN_ENTER) || m.ptr(vt + unit::MILITARY_TAG)? != m.at(MILITARY_TAG) {
        return fail(abi::INVALID_ARGUMENT, "not a land army");
    }
    Ok(())
}

fn military_tag(m: &Memory, ctx: &Ctx, army: usize) -> GResult<Tag> {
    check_army_vtable(m, army)?;
    let mut out = 0u64;
    let ptr = &mut out as *mut u64 as usize;
    if m.call(m.at(MILITARY_TAG), &[army, ptr])? != ptr {
        return Err("military tag getter returned an unexpected pointer".to_string().into());
    }
    let tag = Tag(out);
    if !tag.normal() {
        return Err("army has no military country".to_string().into());
    }
    country(m, ctx, tag)?;
    Ok(tag)
}

pub fn can_enter(m: &Memory, army: usize, province: usize) -> GResult<bool> {
    check_army_vtable(m, army)?;
    Ok(m.call(m.at(CAN_ENTER), &[army, province, 0, 0, 0, route_options::NO_TAG as usize, 0, 0, 0])? as u8 != 0)
}

pub fn can_attack(m: &Memory, army: usize, target: usize) -> GResult<bool> {
    check_army_vtable(m, army)?;
    Ok(m.call(m.at(CAN_ATTACK), &[army, target])? as u8 != 0)
}

/// Every own army and fleet: (id, address, land).
pub fn own_roster(m: &Memory, ctx: &Ctx) -> GResult<Vec<(u64, usize, bool)>> {
    let head = m.ptr(ctx.country + country_fields::UNITS)?;
    let mut node = head;
    let mut nodes = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut out = Vec::new();
    while node != 0 {
        if nodes.len() >= MAX_UNITS || !nodes.insert(node) {
            return Err("own unit list cycle".to_string().into());
        }
        let army = m.ptr(node + list::ITEM)?;
        let vt = m.ptr(army)?;
        let id = m.u64(army + unit::ID)?;
        if id == 0 || !ids.insert(id) || m.u64(army + unit::OWNER)? != ctx.player.0 {
            return Err("own unit identity or owner".to_string().into());
        }
        if vt != m.at(classes::ARMY) && vt != m.at(classes::FLEET) {
            return Err("unknown own unit class".to_string().into());
        }
        out.push((id, army, vt == m.at(classes::ARMY)));
        node = m.ptr(node + list::NEXT)?;
    }
    if m.ptr(ctx.country + country_fields::UNITS)? != head {
        return Err("own unit list changed while reading".to_string().into());
    }
    Ok(out)
}

/// Address of an own land army.
pub fn own_land_army(m: &Memory, ctx: &Ctx, id: u64) -> GResult<usize> {
    match own_roster(m, ctx)?.into_iter().find(|(i, _, _)| *i == id) {
        Some((_, address, true)) => Ok(address),
        Some(_) => fail(abi::INVALID_ARGUMENT, "not a land army"),
        None => fail(abi::NOT_FOUND, format!("no own army {id}")),
    }
}

/// Province ID of a unit's location.
pub fn location(m: &Memory, army: usize) -> GResult<u32> {
    let p = m.ptr(army + unit::LOCATION)?;
    if p == 0 {
        return Ok(0);
    }
    Ok(m.u32(p + province::ID)?)
}

/// Native task attached to the army: 0 for none or the idle default task.
fn busy_mission(m: &Memory, army: usize) -> GResult<bool> {
    let mission = m.ptr(army + unit::TASK)?;
    if mission == 0 {
        return Ok(false);
    }
    if mission != m.ptr(m.at(globals::IDLE_TASK))? || m.ptr(mission)? != m.at(classes::IDLE_TASK) {
        return Ok(true);
    }
    let vt = m.ptr(mission)?;
    for (slot, rva) in task::IDLE_METHODS.into_iter().zip(IDLE_ANSWERS) {
        if m.ptr(vt + slot)? != m.at(rva) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Snapshot of a land army; `route_source` is filled in by the caller.
pub fn army_record(m: &Memory, specials: &[i32; 3], army: usize) -> GResult<abi::Army> {
    let land = m.ptr(army)? == m.at(classes::ARMY);
    let id = m.u64(army + unit::ID)?;
    let owner = Tag(m.u64(army + unit::OWNER)?);
    if !land {
        return Ok(abi::Army { id, owner: owner.0, ..Default::default() });
    }
    let mut flags = abi::ARMY_LAND;
    let set = |flags: &mut u32, on: bool, bit: u32| {
        if on {
            *flags |= bit
        }
    };
    set(&mut flags, m.ptr(army + unit::BATTLE)? != 0, abi::ARMY_IN_BATTLE);
    set(&mut flags, m.ptr(army + unit::SIEGE)? != 0, abi::ARMY_SIEGING);
    set(&mut flags, m.u8(army + unit::RETREATING[0])? != 0 || m.u8(army + unit::RETREATING[1])? != 0, abi::ARMY_RETREATING);
    let exile = owner.kind() != tag::REBELS && !specials.contains(&(owner.index() as i32)) && m.u8(army + unit::EXILED)? != 0;
    set(&mut flags, exile, abi::ARMY_EXILED);
    set(&mut flags, m.u8(army + unit::MOVEMENT_LOCKED)? != 0, abi::ARMY_MOVEMENT_LOCKED);
    set(&mut flags, m.ptr(army + unit::ATTACHED_TO)? != 0, abi::ARMY_ATTACHED);
    set(&mut flags, m.ptr(army + unit::FOLLOWERS)? != 0, abi::ARMY_HAS_FOLLOWERS);
    let busy = |at: usize| -> GResult<bool> { Ok(m.ptr(army + at)? != 0) };
    let vector = unit::CONTEXT_VECTOR;
    let contexts = busy(unit::CONTEXTS[0])? || busy(unit::CONTEXTS[1])? || m.ptr(army + vector)? != m.ptr(army + vector + 8)?;
    set(&mut flags, contexts || busy_mission(m, army)?, abi::ARMY_BUSY);
    let regiments = span(m.ptr(army + unit::REGIMENTS)?, m.ptr(army + unit::REGIMENTS + 8)?, 8, MAX_UNITS)? as u32;
    Ok(abi::Army { id, owner: owner.0, province: location(m, army)?, flags, regiments, ..Default::default() })
}

/// Land armies in a visible province, as (record, address).
pub fn province_armies(m: &Memory, ctx: &Ctx, province: usize) -> GResult<Vec<(abi::Army, usize)>> {
    if hidden(m, province)? {
        return Ok(Vec::new());
    }
    let specials = special_countries(m)?;
    let head = m.ptr(province + province::UNITS)?;
    let mut node = head;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    while node != 0 {
        if seen.len() >= MAX_UNITS || !seen.insert(node) {
            return Err("province unit list cycle".to_string().into());
        }
        let item = m.ptr(node + list::ITEM)?;
        let next = m.ptr(node + list::NEXT)?;
        let vt = m.ptr(item)?;
        if vt == m.at(classes::FLEET) {
            node = next;
            continue;
        }
        if vt != m.at(classes::ARMY) || m.ptr(item + unit::LOCATION)? != province {
            return Err("unexpected unit in province list".to_string().into());
        }
        country(m, ctx, Tag(m.u64(item + unit::OWNER)?))?;
        out.push((army_record(m, &specials, item)?, item));
        node = next;
    }
    if m.ptr(province + province::UNITS)? != head || hidden(m, province)? {
        return Err("province units changed while reading".to_string().into());
    }
    Ok(out)
}

/// At war with each other, or either is a hostile special country (rebels).
pub fn hostile(m: &Memory, ctx: &Ctx, a: Tag, b: Tag) -> GResult<bool> {
    if a == b {
        return Ok(false);
    }
    let specials = special_countries(m)?;
    if specials.contains(&(a.index() as i32)) || specials.contains(&(b.index() as i32)) {
        return Ok(true);
    }
    let c = country(m, ctx, a)?;
    country(m, ctx, b)?;
    let count = m.i32(c + country_fields::HOSTILE_COUNT)?;
    if !(0..=32767).contains(&count) {
        return Err("hostility table bound".to_string().into());
    }
    let entries = m.ptr(c + country_fields::HOSTILE)?;
    for i in 0..count as usize {
        let index = m.i32(entries + i * country_fields::HOSTILE_SIZE)?;
        if !(0..=32767).contains(&index) {
            return Err("invalid hostility entry".to_string().into());
        }
        if index == b.index() as i32 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Current campaign for read capabilities.
pub fn ctx(m: &Memory) -> GResult<Ctx> {
    context(m)
}
