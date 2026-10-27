//! Reading provinces, armies and countries. Visibility rules follow the game:
//! units in fogged provinces are never read.
use super::{context, country, fail, special_countries, span, Ctx, GResult, Tag, ARMY_VT, FLEET_VT, MAP, MAX_PROVINCES, MAX_UNITS, PROVINCE_STRIDE};
use crate::{memory::Memory, points::Point};
use eu4_harmony_abi as abi;
use std::collections::BTreeSet;

const CAN_ENTER: usize = 0x1a2280;
const MILITARY_TAG: usize = 0x1a8d20;
const STRAIT_BLOCKED: usize = 0x96f650;
const CAN_ATTACK: usize = 0xd54a60;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CAN_ENTER, "48 89 5c 24 08 48 89 6c 24 10 56 57 41 56 48 83", "army access query"),
        Point::keep(MILITARY_TAG, "48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9", "army military tag"),
        Point::keep(STRAIT_BLOCKED, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48", "strait blockade query"),
        Point::keep(CAN_ATTACK, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 80", "attack permission query"),
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
fn le_u64(bytes: &[u8], offset: usize) -> GResult<u64> {
    Ok(u64::from_le_bytes(bytes.get(offset..offset + 8).ok_or_else(|| "truncated read".to_string())?.try_into().unwrap()))
}

fn decode_edges(bytes: &[u8], id: usize, total: usize, address: usize) -> GResult<Vec<Edge>> {
    if bytes.len() % 0x24 != 0 || bytes.len() / 0x24 > total {
        return Err("adjacency vector bound".to_string().into());
    }
    let mut edges = Vec::with_capacity(bytes.len() / 0x24);
    for (j, record) in bytes.chunks_exact(0x24).enumerate() {
        let to = le_u32(record, 0)? as i32;
        let from = le_u32(record, 4)? as i32;
        let kind = le_u32(record, 0x1c)? as i32;
        if from != id as i32 || to < 0 || to as usize >= total || to == from || !(0..=4).contains(&kind) {
            return Err("invalid adjacency record".to_string().into());
        }
        edges.push(Edge { to: to as u32, through: le_u32(record, 8)? as i32, kind, address: address + j * 0x24 });
    }
    Ok(edges)
}

fn read_node(m: &Memory, first: usize, id: usize, count: usize) -> GResult<Node> {
    let address = first + id * PROVINCE_STRIDE;
    let header = m.bytes(address + 0x20, 0x10)?;
    if le_u32(&header, 0)? != id as u32 {
        return Err("province ID does not match its index".to_string().into());
    }
    let geometry = le_u64(&header, 8)? as usize;
    if geometry == 0 {
        return Err("province without geometry".to_string().into());
    }
    let fields = m.bytes(geometry + 0x150, 0xe0)?;
    if le_u32(&fields, 0)? != id as u32 {
        return Err("geometry ID mismatch".to_string().into());
    }
    let adjacent = le_u64(&fields, 0x18)? as usize;
    let adjacent_end = le_u64(&fields, 0x20)? as usize;
    let bytes = adjacent_end.checked_sub(adjacent).ok_or_else(|| "reversed adjacency".to_string())?;
    let edges = if bytes == 0 { Vec::new() } else { decode_edges(&m.bytes(adjacent, bytes)?, id, count, adjacent)? };
    Ok(Node {
        id: id as u32,
        address,
        geometry,
        land: fields[0xd8] & 8 != 0,
        lake: fields[0xd8] & 0x10 != 0,
        impassable: fields[0xd9] & 8 != 0,
        adjacent,
        adjacent_end,
        edges,
    })
}

/// Re-reads a cached node's identity and adjacency so a changed map is never
/// served from the cache.
fn validate_node(m: &Memory, n: &Node, count: usize) -> GResult<()> {
    let fresh = read_node(m, n.address - n.id as usize * PROVINCE_STRIDE, n.id as usize, count)?;
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
    let map = m.ptr(m.at(MAP))?;
    let first = m.ptr(map + 0x6f0)?;
    let end = m.ptr(map + 0x6f8)?;
    let count = span(first, end, PROVINCE_STRIDE, MAX_PROVINCES)?;
    if count == 0 || m.ptr(ctx.world + 0x1ca8)? != first {
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
        let key = MapKey { world: ctx.world, map: m.ptr(m.at(MAP))?, first, count };
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
            let owner = Tag(m.u64(n.address + 0x468)?);
            let controller = Tag(m.u64(n.address + 0x470)?);
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
            .filter(|e| (0..=2).contains(&e.kind) && nodes.get(e.to as usize).is_some_and(Node::admissible))
            .map(|e| abi::Neighbor { to: e.to, flags: abi::NEIGHBOR_LAND | if e.kind == 1 { abi::NEIGHBOR_STRAIT } else { 0 } })
            .collect())
    }

    /// Whether the army may cross from `from` to the neighbor `to`; for
    /// straits this asks the game about blockades by the army's military tag.
    pub fn strait_open(&mut self, m: &Memory, ctx: &Ctx, army: usize, from: u32, to: u32) -> GResult<bool> {
        let node = self.node(m, ctx, from)?;
        let Some(edge) = node.edges.iter().find(|e| e.to == to && (0..=2).contains(&e.kind)) else {
            return fail(abi::NOT_FOUND, format!("{to} is not a land neighbor of {from}"));
        };
        if edge.kind != 1 {
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
    let fog = (m.u8(province + 0x958)?, m.u8(province + 0x959)?);
    Ok(fog.1 & 0x20 != 0 && fog.0 & 2 != 0)
}

fn check_army_vtable(m: &Memory, army: usize) -> GResult<()> {
    let vt = m.ptr(army)?;
    if vt != m.at(ARMY_VT) || m.ptr(vt + 0x120)? != m.at(CAN_ENTER) || m.ptr(vt + 0x138)? != m.at(MILITARY_TAG) {
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
    Ok(m.call(m.at(CAN_ENTER), &[army, province, 0, 0, 0, 0x2d2d2d, 0, 0, 0])? as u8 != 0)
}

pub fn can_attack(m: &Memory, army: usize, target: usize) -> GResult<bool> {
    check_army_vtable(m, army)?;
    Ok(m.call(m.at(CAN_ATTACK), &[army, target])? as u8 != 0)
}

/// Every own army and fleet: (id, address, land).
pub fn own_roster(m: &Memory, ctx: &Ctx) -> GResult<Vec<(u64, usize, bool)>> {
    let head = m.ptr(ctx.country + 0xd18)?;
    let mut node = head;
    let mut nodes = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut out = Vec::new();
    while node != 0 {
        if nodes.len() >= MAX_UNITS || !nodes.insert(node) {
            return Err("own unit list cycle".to_string().into());
        }
        let army = m.ptr(node)?;
        let vt = m.ptr(army)?;
        let id = m.u64(army + 0x20)?;
        if id == 0 || !ids.insert(id) || m.u64(army + 0xd0)? != ctx.player.0 {
            return Err("own unit identity or owner".to_string().into());
        }
        if vt != m.at(ARMY_VT) && vt != m.at(FLEET_VT) {
            return Err("unknown own unit class".to_string().into());
        }
        out.push((id, army, vt == m.at(ARMY_VT)));
        node = m.ptr(node + 0x10)?;
    }
    if m.ptr(ctx.country + 0xd18)? != head {
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
    let p = m.ptr(army + 0xe8)?;
    if p == 0 {
        return Ok(0);
    }
    Ok(m.u32(p + 0x20)?)
}

/// Native task attached to the army: 0 for none or the idle default task.
fn busy_mission(m: &Memory, army: usize) -> GResult<bool> {
    const IDLE_SINGLETON: usize = 0x2340020;
    const IDLE_VT: usize = 0x1d1e800;
    let mission = m.ptr(army + 0x1f8)?;
    if mission == 0 {
        return Ok(false);
    }
    if mission != m.ptr(m.at(IDLE_SINGLETON))? || m.ptr(mission)? != m.at(IDLE_VT) {
        return Ok(true);
    }
    let vt = m.ptr(mission)?;
    for (slot, rva) in [(0x50, 0x8f480), (0x78, 0x8f450), (0x80, 0x8f450)] {
        if m.ptr(vt + slot)? != m.at(rva) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Snapshot of a land army; `route_source` is filled in by the caller.
pub fn army_record(m: &Memory, specials: &[i32; 3], army: usize) -> GResult<abi::Army> {
    let land = m.ptr(army)? == m.at(ARMY_VT);
    let id = m.u64(army + 0x20)?;
    let owner = Tag(m.u64(army + 0xd0)?);
    if !land {
        return Ok(abi::Army { id, owner: owner.0, ..Default::default() });
    }
    let mut flags = abi::ARMY_LAND;
    let set = |flags: &mut u32, on: bool, bit: u32| {
        if on {
            *flags |= bit
        }
    };
    set(&mut flags, m.ptr(army + 0xc0)? != 0, abi::ARMY_IN_BATTLE);
    set(&mut flags, m.ptr(army + 0xc8)? != 0, abi::ARMY_SIEGING);
    set(&mut flags, m.u8(army + 0x203)? != 0 || m.u8(army + 0x204)? != 0, abi::ARMY_RETREATING);
    let exile = owner.kind() != 7 && !specials.contains(&(owner.index() as i32)) && m.u8(army + 0x207)? != 0;
    set(&mut flags, exile, abi::ARMY_EXILED);
    set(&mut flags, m.u8(army + 0x202)? != 0, abi::ARMY_MOVEMENT_LOCKED);
    set(&mut flags, m.ptr(army + 0x50)? != 0, abi::ARMY_ATTACHED);
    set(&mut flags, m.ptr(army + 0x58)? != 0, abi::ARMY_HAS_FOLLOWERS);
    let contexts = m.ptr(army + 0x98)? != 0 || m.ptr(army + 0xa0)? != 0 || m.ptr(army + 0xa8)? != m.ptr(army + 0xb0)?;
    set(&mut flags, contexts || busy_mission(m, army)?, abi::ARMY_BUSY);
    let regiments = span(m.ptr(army + 0x30)?, m.ptr(army + 0x38)?, 8, MAX_UNITS)? as u32;
    Ok(abi::Army { id, owner: owner.0, province: location(m, army)?, flags, regiments, ..Default::default() })
}

/// Land armies in a visible province, as (record, address).
pub fn province_armies(m: &Memory, ctx: &Ctx, province: usize) -> GResult<Vec<(abi::Army, usize)>> {
    if hidden(m, province)? {
        return Ok(Vec::new());
    }
    let specials = special_countries(m)?;
    let head = m.ptr(province + 0x350)?;
    let mut node = head;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    while node != 0 {
        if seen.len() >= MAX_UNITS || !seen.insert(node) {
            return Err("province unit list cycle".to_string().into());
        }
        let unit = m.ptr(node)?;
        let next = m.ptr(node + 0x10)?;
        let vt = m.ptr(unit)?;
        if vt == m.at(FLEET_VT) {
            node = next;
            continue;
        }
        if vt != m.at(ARMY_VT) || m.ptr(unit + 0xe8)? != province {
            return Err("unexpected unit in province list".to_string().into());
        }
        country(m, ctx, Tag(m.u64(unit + 0xd0)?))?;
        out.push((army_record(m, &specials, unit)?, unit));
        node = next;
    }
    if m.ptr(province + 0x350)? != head || hidden(m, province)? {
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
    let count = m.i32(c + 0x1554)?;
    if !(0..=32767).contains(&count) {
        return Err("hostility table bound".to_string().into());
    }
    let entries = m.ptr(c + 0x1548)?;
    for i in 0..count as usize {
        let index = m.i32(entries + i * 16)?;
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
