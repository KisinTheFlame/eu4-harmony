//! Reading the map's provinces and the countries: the provinces' graph,
//! fog of war and discovery, and who is at war with whom. One army or fleet is
//! read by [`super::army`].
use super::{
    country, fail,
    player::Eyes,
    provinces::{Neighbor, Province},
    special_countries, Ctx, GResult, InWorld, Status, Tag, MAX_PROVINCES,
};
use crate::{
    containers,
    memory::Memory,
    objects::campaign::{adjacency, countries as country_list, country as country_fields, geometry, globals, map, province, world},
    points::{GameFunction, Point},
};
const STRAIT_BLOCKED: GameFunction<2> = GameFunction::at(0x96f650);

pub fn points() -> Vec<Point> {
    vec![Point::keep(STRAIT_BLOCKED, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48", "strait blockade query")]
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
    if !bytes.len().is_multiple_of(adjacency::SIZE) || bytes.len() / adjacency::SIZE > total {
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

/// First province and count, cross-checked between the map and the world
/// `world`.
fn province_array(m: &Memory, world: usize) -> GResult<(usize, usize)> {
    let map = m.ptr(m.at(globals::MAP))?;
    let first = m.ptr(map + map::PROVINCES)?;
    let end = m.ptr(map + map::PROVINCES + 8)?;
    let count = containers::span(first, end, province::SIZE, MAX_PROVINCES)?;
    if count == 0 || m.ptr(world + world::PROVINCES)? != first {
        return Err("world and map province arrays differ".to_string().into());
    }
    Ok((first, count))
}

/// How many provinces the country `owner` owns in the world `world`, which
/// need not have a player: the main menu's too.
pub fn owned_provinces(m: &Memory, world: usize, owner: Tag) -> GResult<usize> {
    let (first, count) = province_array(m, world)?;
    let mut owned = 0;
    for index in 0..count {
        if m.u64(first + index * province::SIZE + province::OWNER)? == owner.0 {
            owned += 1;
        }
    }
    Ok(owned)
}

impl Geography {
    pub fn clear(&mut self) {
        self.entry = None;
    }

    fn nodes(&mut self, m: &Memory, ctx: &impl InWorld) -> GResult<&[Node]> {
        let (first, count) = province_array(m, ctx.world())?;
        let key = MapKey { world: ctx.world(), map: m.ptr(m.at(globals::MAP))?, first, count };
        if self.entry.as_ref().map(|(k, _)| *k) != Some(key) {
            self.entry = None;
            let nodes = (0..count).map(|id| read_node(m, first, id, count)).collect::<GResult<Vec<_>>>()?;
            self.entry = Some((key, nodes));
        }
        Ok(&self.entry.as_ref().unwrap().1)
    }

    fn node(&mut self, m: &Memory, ctx: &impl InWorld, id: u32) -> GResult<Node> {
        let nodes = self.nodes(m, ctx)?;
        let count = nodes.len();
        let Some(node) = nodes.get(id as usize).cloned() else {
            return Err(eu4_harmony_checks::provinces::not_found(id).into());
        };
        if let Err(e) = validate_node(m, &node, count) {
            self.clear();
            return Err(e);
        }
        Ok(node)
    }

    /// Address of a province object.
    pub fn province_address(&mut self, m: &Memory, ctx: &impl InWorld, id: u32) -> GResult<usize> {
        Ok(self.node(m, ctx, id)?.address)
    }

    pub fn provinces(&mut self, m: &Memory, ctx: &impl InWorld, visibility_current: bool) -> GResult<Vec<Province>> {
        self.nodes(m, ctx)?.iter().map(|n| if n.id == 0 { Ok(Province::default()) } else { province_now(m, n, visibility_current) }).collect()
    }

    /// Every province's owner, by number; none for the placeholder 0 and a
    /// province no country owns. Only the owner is read.
    pub fn owners(&mut self, m: &Memory, ctx: &impl InWorld) -> GResult<Vec<Option<Tag>>> {
        self.nodes(m, ctx)?
            .iter()
            .map(|n| if n.id == 0 { Ok(None) } else { Ok(Some(Tag(m.u64(n.address + province::OWNER)?)).filter(|t| t.valid())) })
            .collect()
    }

    /// The provinces numbered in `ids` that there are, in their order; only
    /// those are read.
    pub fn named_provinces(&mut self, m: &Memory, ctx: &impl InWorld, visibility_current: bool, ids: &[u32]) -> GResult<Vec<Province>> {
        let nodes = self.nodes(m, ctx)?;
        ids.iter().filter_map(|id| nodes.get(*id as usize).filter(|n| n.id != 0)).map(|n| province_now(m, n, visibility_current)).collect()
    }

    /// Every province's address, land, sea and lake.
    pub fn addresses(&mut self, m: &Memory, ctx: &impl InWorld) -> GResult<Vec<usize>> {
        Ok(self.nodes(m, ctx)?.iter().filter(|n| n.id != 0).map(|n| n.address).collect())
    }

    /// Every province armies can stand on, as (number, address).
    pub fn land_provinces(&mut self, m: &Memory, ctx: &impl InWorld) -> GResult<Vec<(u32, usize)>> {
        Ok(self.nodes(m, ctx)?.iter().filter(|n| n.admissible()).map(|n| (n.id, n.address)).collect())
    }

    /// Land connections between passable land provinces.
    pub fn neighbors(&mut self, m: &Memory, ctx: &impl InWorld, id: u32) -> GResult<Vec<Neighbor>> {
        let node = self.node(m, ctx, id)?;
        if !node.admissible() {
            return Ok(Vec::new());
        }
        let nodes = self.nodes(m, ctx)?;
        Ok(node
            .edges
            .iter()
            .filter(|e| by_land(e.kind) && nodes.get(e.to as usize).is_some_and(Node::admissible))
            .map(|e| Neighbor { to: e.to, strait: e.kind == adjacency::STRAIT })
            .collect())
    }

    /// Whether the army may cross from `from` to the neighbor `to`; for
    /// straits this asks the game about blockades by the army's military tag.
    pub fn strait_open(&mut self, m: &Memory, ctx: &Ctx, army: usize, from: u32, to: u32) -> GResult<bool> {
        let node = self.node(m, ctx, from)?;
        let Some(edge) = node.edges.iter().find(|e| e.to == to && by_land(e.kind)) else {
            return fail(Status::NotFound, format!("{to} is not a land neighbor of {from}"));
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
        let military = super::army::military_tag(m, ctx, army)?;
        Ok(STRAIT_BLOCKED.call(m, [military.0 as usize, edge.address])? as u8 == 0)
    }

    pub fn clear_if_changed(&mut self, m: &Memory, ctx: &impl InWorld) {
        if self.nodes(m, ctx).is_err() {
            self.clear();
        }
    }
}

/// A province's owner, controller and fog now, with its cached map data.
fn province_now(m: &Memory, n: &Node, visibility_current: bool) -> GResult<Province> {
    let tag_at = |at: usize| -> GResult<Option<Tag>> { Ok(Some(Tag(m.u64(n.address + at)?)).filter(|t| t.valid())) };
    Ok(Province {
        id: n.id,
        land: n.land,
        lake: n.lake,
        impassable: n.impassable,
        visible: visibility_current && !hidden(m, n.address)?,
        owner: tag_at(province::OWNER)?,
        controller: tag_at(province::CONTROLLER)?,
    })
}

/// `eyes` see this province as terra incognita: the country whose
/// discoveries they go by has not discovered it.
pub fn undiscovered(m: &Memory, eyes: Eyes, province: usize) -> GResult<bool> {
    let Some(country) = eyes.discovering() else { return Ok(false) };
    if m.u8(province + province::FOGGED)? & province::UNDISCOVERED_BIT == 0 {
        return Ok(false);
    }
    let index = country.index() as u16 as usize;
    let word = m.u32(m.ptr(province + province::DISCOVERED_BY)? + index / 32 * 4)?;
    Ok(word >> (index % 32) & 1 == 0)
}

/// Fog of war hides this province.
pub fn hidden(m: &Memory, province: usize) -> GResult<bool> {
    let fogged = m.u8(province + province::FOGGED)? & province::FOGGED_BIT != 0;
    Ok(fogged && m.u8(province + province::HIDDEN)? & province::HIDDEN_BIT != 0)
}

/// Every country's tag, by tag index; empty slots and invalid tags are
/// skipped.
pub fn countries(m: &Memory, ctx: &impl InWorld) -> GResult<Vec<Tag>> {
    let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
    if m.ptr(manager + country_list::LIST)? != ctx.countries() {
        return Err("country array moved".to_string().into());
    }
    let count = m.i32(manager + country_list::COUNT)?;
    if !(0..=32767).contains(&count) {
        return Err("country array bound".to_string().into());
    }
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let country = m.ptr(ctx.countries() + index * 8)?;
        if country == 0 {
            continue;
        }
        let tag = Tag(m.u64(country + country_fields::TAG)?);
        if !tag.valid() {
            continue;
        }
        if tag.index() as usize != index {
            return Err(format!("country {index} has the tag of country {}", tag.index()).into());
        }
        out.push(tag);
    }
    Ok(out)
}

/// At war with each other, or either is a hostile special country (rebels).
pub fn hostile(m: &Memory, ctx: &impl InWorld, a: Tag, b: Tag) -> GResult<bool> {
    if a == b {
        return Ok(false);
    }
    let specials = special_countries(m)?;
    if specials.contains(&(a.index() as i32)) || specials.contains(&(b.index() as i32)) {
        return Ok(true);
    }
    at_war(m, ctx, a, b)
}

/// In a war with each other: a special country, hostile to every country,
/// is in none.
pub fn at_war(m: &Memory, ctx: &impl InWorld, a: Tag, b: Tag) -> GResult<bool> {
    if a == b {
        return Ok(false);
    }
    let specials = special_countries(m)?;
    if specials.contains(&(a.index() as i32)) || specials.contains(&(b.index() as i32)) {
        return Ok(false);
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
