//! Reading the map's provinces and the countries: the provinces' graph,
//! fog of war and discovery, and who is at war with whom. One army or fleet is
//! read by [`super::army`].
use super::{country, fail, player::Eyes, special_countries, Ctx, GResult, InWorld, Status, Tag};
use crate::native::{
    memory::Memory,
    objects::campaign::{adjacency, countries as countries_fields, country as country_fields, geometry, globals, province},
    points::{GameFunction, Point},
};
const STRAIT_BLOCKED: GameFunction<2> = GameFunction::at(0x96f650);
/// The game's getters of a province's base tax, production and manpower, as
/// triggers and the province window read them: (unused, out, province).
const BASE_TAX: GameFunction<3> = GameFunction::at(0x6de7a0);
const BASE_PRODUCTION: GameFunction<3> = GameFunction::at(0x6de830);
const BASE_MANPOWER: GameFunction<3> = GameFunction::at(0x6de8c0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Province {
    pub id: u32,
    pub land: bool,
    pub lake: bool,
    pub impassable: bool,
    /// Not under fog of war; false while visibility is not current.
    pub visible: bool,
    pub owner: Option<Tag>,
    pub controller: Option<Tag>,
}

/// A province reachable over land, possibly across a strait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Neighbor {
    pub to: u32,
    pub strait: bool,
}

pub fn points() -> Vec<Point> {
    let getter = |function: GameFunction<3>, bytes: &'static str, what: &'static str, value: &'static [usize]| Point::keep(function, bytes, what).uses(value);
    vec![
        Point::keep(STRAIT_BLOCKED, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48", "strait blockade query"),
        getter(
            BASE_TAX,
            "4c 8b ca 49 8b 88 68 04 00 00 48 8b 05 1f f1 c5 01 48 c1 e9 20 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 14 c8 \
             48 8b 8a 78 9a 00 00 48 85 c9 74 26 41 8b 40 20 39 82 b8 14 00 00 75 1a b8 d3 4d 62 10 f7 69 1c c1 fa 06 8b c2 \
             c1 e8 1f 03 d0 69 c2 e8 03 00 00 eb 02 33 c0 41 8b 90 e4 03 00 00 03 d0 41 89 11 49 8b c1 c3",
            "base tax getter",
            &[province::OWNER, globals::COUNTRY_MANAGER, countries_fields::LIST, province::ID, province::BASE_TAX],
        ),
        getter(
            BASE_PRODUCTION,
            "4c 8b ca 49 8b 88 68 04 00 00 48 8b 05 8f f0 c5 01 48 c1 e9 20 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 14 c8 \
             48 8b 8a 78 9a 00 00 48 85 c9 74 26 41 8b 40 20 39 82 b8 14 00 00 75 1a b8 d3 4d 62 10 f7 69 20 c1 fa 06 8b c2 \
             c1 e8 1f 03 d0 69 c2 e8 03 00 00 eb 02 33 c0 41 8b 90 e8 03 00 00 03 d0 41 89 11 49 8b c1 c3",
            "base production getter",
            &[province::OWNER, globals::COUNTRY_MANAGER, countries_fields::LIST, province::ID, province::BASE_PRODUCTION],
        ),
        getter(
            BASE_MANPOWER,
            "4c 8b ca 49 8b 88 68 04 00 00 48 8b 05 ff ef c5 01 48 c1 e9 20 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 14 c8 \
             48 8b 8a 78 9a 00 00 48 85 c9 74 26 41 8b 40 20 39 82 b8 14 00 00 75 1a b8 d3 4d 62 10 f7 69 24 c1 fa 06 8b c2 \
             c1 e8 1f 03 d0 69 c2 e8 03 00 00 eb 02 33 c0 41 8b 90 e0 03 00 00 03 d0 41 89 11 49 8b c1 c3",
            "base manpower getter",
            &[province::OWNER, globals::COUNTRY_MANAGER, countries_fields::LIST, province::ID, province::BASE_MANPOWER],
        ),
    ]
}

/// One of the base development getters.
fn base_development(m: &Memory, getter: GameFunction<3>, province: usize) -> GResult<i32> {
    let mut out = 0i32;
    let ptr = &mut out as *mut i32 as usize;
    if getter.call(m, [0, ptr, province])? != ptr {
        return Err("development getter returned an unexpected pointer".to_string().into());
    }
    Ok(out)
}

/// Base tax, production and manpower of a province, in the game's fixed point.
pub(super) fn development_parts(m: &Memory, province: usize) -> GResult<(i32, i32, i32)> {
    Ok((base_development(m, BASE_TAX, province)?, base_development(m, BASE_PRODUCTION, province)?, base_development(m, BASE_MANPOWER, province)?))
}

/// Total development in the game's fixed point, including tribal development.
pub(super) fn development(m: &Memory, province: usize) -> GResult<i32> {
    let (tax, production, manpower) = development_parts(m, province)?;
    tax.checked_add(production).and_then(|sum| sum.checked_add(manpower)).ok_or_else(|| "province development overflow".to_string().into())
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

pub(crate) use crate::world::{owned_provinces, province_array, provinces_owned_by};

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
            return Err(eu4_harmony_remote::checks::provinces::not_found(id).into());
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

    /// Whether a country fighting under `military` may cross from `from` to
    /// the neighbor `to`; for straits this asks the game about blockades.
    pub fn strait_open(&mut self, m: &Memory, ctx: &Ctx, military: Tag, from: u32, to: u32) -> GResult<bool> {
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
        Ok(STRAIT_BLOCKED.call(m, [military.0 as usize, edge.address])? as u8 == 0)
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

pub use crate::world::countries;

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
