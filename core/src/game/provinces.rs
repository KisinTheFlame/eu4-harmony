//! Provinces: every province, a province's land neighbors, and one
//! province's snapshot: its name, development, terrain, fort, buildings and
//! what the player can recruit there (docs/research/provinces.md).
use super::{context, events, fail, game, span, with_state, Caller, Ctx, GResult, Status, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{building, classes, country, geometry, globals, map, province as field, terrain, unit_type},
    points::Point,
    unicode::game_text,
};

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

/// Every province, indexed by ID; index 0 is a placeholder with nothing set.
pub fn provinces(_: Caller) -> GResult<Vec<Province>> {
    let m = game()?;
    let ctx = context(m)?;
    let visible = events::visibility_current(m, &ctx);
    with_state(|s| s.geography.provinces(m, &ctx, visible))
}

/// The passable land provinces next to a province; none for a province an
/// army cannot stand on.
pub fn neighbors(_: Caller, province: u32) -> GResult<Vec<Neighbor>> {
    let m = game()?;
    let ctx = context(m)?;
    with_state(|s| s.geography.neighbors(m, &ctx, province))
}

// ---------- the snapshot ----------

/// Base tax, production and manpower, in thousandths, as the game's own
/// getters give them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Development {
    pub tax: i32,
    pub production: i32,
    pub manpower: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fort {
    /// The fort level that defends it now; 0 for none.
    pub level: i32,
    pub mothballed: bool,
}

/// A kind of regiment or ship a country recruits; the game's own order of
/// kinds, which a country's unit types are kept by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnitKind {
    Infantry,
    Cavalry,
    Artillery,
    HeavyShip,
    LightShip,
    Galley,
    Transport,
}

impl UnitKind {
    pub const ALL: [UnitKind; country::UNIT_KINDS] =
        [Self::Infantry, Self::Cavalry, Self::Artillery, Self::HeavyShip, Self::LightShip, Self::Galley, Self::Transport];

    /// Its index among a country's unit types.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The game's key for it, as in a country's history (`heavy_ship`).
    pub fn key(self) -> &'static str {
        ["infantry", "cavalry", "artillery", "heavy_ship", "light_ship", "galley", "transport"][self.index()]
    }
}

/// Whether the player can recruit one regiment or ship of a kind in a
/// province now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recruit {
    pub kind: UnitKind,
    /// The key of the unit type the player recruits of this kind, such as
    /// `western_medieval_infantry`; None when the player has none.
    pub unit_type: Option<String>,
    pub allowed: bool,
    /// Why not, in the game's words, from its localisation for the language
    /// in the game's settings (not the player language), as plain text
    /// ([`game_text::plain`]); empty when it is allowed or the game gives no
    /// reason.
    pub reasons: Vec<String>,
}

/// All of one province at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub id: u32,
    /// Its name as the game shows it, as Unicode (read from game text).
    pub name: String,
    pub owner: Option<Tag>,
    pub controller: Option<Tag>,
    pub development: Development,
    /// The key of its terrain, such as `farmlands`.
    pub terrain: Option<String>,
    pub fort: Fort,
    /// The keys of its buildings, such as `fort_15th`, in the game's order of
    /// buildings.
    pub buildings: Vec<String>,
    /// One for each [`UnitKind`], in their order.
    pub recruit: Vec<Recruit>,
}

/// A province's snapshot; [`Status::NotFound`] for a number that names no
/// province.
pub fn province(_: Caller, id: u32) -> GResult<Snapshot> {
    let m = game()?;
    let ctx = context(m)?;
    if id == 0 {
        return fail(Status::NotFound, "no province 0");
    }
    let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
    snapshot(m, &ctx, address)
}

/// The game's getters of a province's base tax, production and manpower, as
/// triggers and the province window read them: (unused, out, province).
const BASE_TAX: usize = 0x6de7a0;
const BASE_PRODUCTION: usize = 0x6de830;
const BASE_MANPOWER: usize = 0x6de8c0;
/// Whether a country can recruit a unit type in a province: (country,
/// province, unit type, special kind, any province, check resources,
/// reasons, ...), with the reasons appended to a `std::string`.
const CAN_RECRUIT: usize = 0x346570;
/// What the recruit command checks after it: (province, unit type); ships
/// need the province's port.
const RECRUIT_PLACE: usize = 0xa1fe50;

/// The most kinds of terrain or building believed.
const MOST_KINDS: usize = 10_000;

pub fn points() -> Vec<Point> {
    use crate::objects::campaign::countries;
    let getter = |rva: usize, bytes: &'static str, what: &'static str, value: &'static [usize]| Point::keep(rva, bytes, what).uses(value);
    vec![
        getter(
            BASE_TAX,
            "4c 8b ca 49 8b 88 68 04 00 00 48 8b 05 1f f1 c5 01 48 c1 e9 20 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 14 c8 \
             48 8b 8a 78 9a 00 00 48 85 c9 74 26 41 8b 40 20 39 82 b8 14 00 00 75 1a b8 d3 4d 62 10 f7 69 1c c1 fa 06 8b c2 \
             c1 e8 1f 03 d0 69 c2 e8 03 00 00 eb 02 33 c0 41 8b 90 e4 03 00 00 03 d0 41 89 11 49 8b c1 c3",
            "base tax getter",
            &[field::OWNER, globals::COUNTRY_MANAGER, countries::LIST, field::ID, field::BASE_TAX],
        ),
        getter(
            BASE_PRODUCTION,
            "4c 8b ca 49 8b 88 68 04 00 00 48 8b 05 8f f0 c5 01 48 c1 e9 20 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 14 c8 \
             48 8b 8a 78 9a 00 00 48 85 c9 74 26 41 8b 40 20 39 82 b8 14 00 00 75 1a b8 d3 4d 62 10 f7 69 20 c1 fa 06 8b c2 \
             c1 e8 1f 03 d0 69 c2 e8 03 00 00 eb 02 33 c0 41 8b 90 e8 03 00 00 03 d0 41 89 11 49 8b c1 c3",
            "base production getter",
            &[field::OWNER, globals::COUNTRY_MANAGER, countries::LIST, field::ID, field::BASE_PRODUCTION],
        ),
        getter(
            BASE_MANPOWER,
            "4c 8b ca 49 8b 88 68 04 00 00 48 8b 05 ff ef c5 01 48 c1 e9 20 48 0f bf c9 48 8b 80 18 01 00 00 48 8b 14 c8 \
             48 8b 8a 78 9a 00 00 48 85 c9 74 26 41 8b 40 20 39 82 b8 14 00 00 75 1a b8 d3 4d 62 10 f7 69 24 c1 fa 06 8b c2 \
             c1 e8 1f 03 d0 69 c2 e8 03 00 00 eb 02 33 c0 41 8b 90 e0 03 00 00 03 d0 41 89 11 49 8b c1 c3",
            "base manpower getter",
            &[field::OWNER, globals::COUNTRY_MANAGER, countries::LIST, field::ID, field::BASE_MANPOWER],
        ),
        Point::keep(CAN_RECRUIT, "44 89 4c 24 20 4c 89 44 24 18 48 89 54 24 10 55 53 56 57 41 54 41 55 41 56 41 57", "recruit check"),
        // It reads the unit type's kind: regiments may go anywhere.
        Point::keep(RECRUIT_PLACE, "40 57 48 83 ec 20 44 8b 42 10 4c 8b c9 45 85 c0 0f 84 f2 00 00 00", "recruit place check").uses(&[unit_type::KIND]),
    ]
}

fn snapshot(m: &Memory, ctx: &Ctx, address: usize) -> GResult<Snapshot> {
    let geometry = m.ptr(address + field::GEOMETRY)?;
    let tag_at = |at: usize| -> GResult<Option<Tag>> { Ok(Some(Tag(m.u64(address + at)?)).filter(|t| t.valid())) };
    let mothballed = m.u8(address + field::MOTHBALL)? == field::MOTHBALLED;
    Ok(Snapshot {
        id: m.u32(address + field::ID)?,
        name: text_at(m, geometry + geometry::NAME)?,
        owner: tag_at(field::OWNER)?,
        controller: tag_at(field::CONTROLLER)?,
        development: Development { tax: base(m, BASE_TAX, address)?, production: base(m, BASE_PRODUCTION, address)?, manpower: base(m, BASE_MANPOWER, address)? },
        terrain: terrain_of(m, geometry)?,
        fort: Fort { level: m.i32(address + if mothballed { field::FORT_LEVEL_MOTHBALLED } else { field::FORT_LEVEL })?, mothballed },
        buildings: buildings(m, address)?,
        recruit: recruit(m, ctx, address)?,
    })
}

/// The game text of the `std::string` at `string`, as Unicode: a name or a
/// key.
fn text_at(m: &Memory, string: usize) -> GResult<String> {
    Ok(game_text::to_utf8(&game_string::read(m, string)?))
}

/// One of the base development getters.
fn base(m: &Memory, getter: usize, province: usize) -> GResult<i32> {
    let mut out = 0i32;
    let ptr = &mut out as *mut i32 as usize;
    if m.call(m.at(getter), &[0, ptr, province])? != ptr {
        return Err("development getter returned an unexpected pointer".to_string().into());
    }
    Ok(out)
}

/// The key of the province's terrain, checked to be one of the map's.
fn terrain_of(m: &Memory, geometry: usize) -> GResult<Option<String>> {
    let terrain = m.ptr(geometry + geometry::TERRAIN_TYPE)?;
    if terrain == 0 {
        return Ok(None);
    }
    let map = m.ptr(m.at(globals::MAP))?;
    let first = m.ptr(map + map::TERRAINS)?;
    let count = span(first, m.ptr(map + map::TERRAINS + 8)?, 8, MOST_KINDS)?;
    if !(0..count).any(|i| m.ptr(first + i * 8).ok() == Some(terrain)) {
        return Err("the province's terrain is not one of the map's".to_string().into());
    }
    Ok(Some(text_at(m, terrain + terrain::NAME)?))
}

/// The keys of the buildings in the province: the slots holding a building,
/// each at its building's index.
fn buildings(m: &Memory, province: usize) -> GResult<Vec<String>> {
    let first = m.ptr(province + field::BUILDINGS)?;
    let count = span(first, m.ptr(province + field::BUILDINGS + 8)?, field::BUILDING_SLOT, MOST_KINDS)?;
    let mut out = Vec::new();
    for slot in 0..count {
        let building = m.ptr(first + slot * field::BUILDING_SLOT)?;
        if building == 0 {
            continue;
        }
        match m.u32(building + building::INDEX)? as usize {
            0 => continue,
            index if index != slot => return Err(format!("building {index} is in slot {slot}").into()),
            _ => out.push(text_at(m, building + building::NAME)?),
        }
    }
    Ok(out)
}

/// The unit type a country recruits of a kind; None for the placeholder
/// that stands for none.
pub fn unit_type(m: &Memory, country_address: usize, kind: UnitKind) -> GResult<Option<usize>> {
    let unit_type = m.ptr(country_address + country::UNIT_TYPES + kind.index() * 8)?;
    if unit_type == 0 || m.ptr(unit_type)? != m.at(classes::UNIT_TYPE) {
        return Ok(None);
    }
    if m.u32(unit_type + unit_type::KIND)? as usize != kind.index() {
        return Err(format!("the {kind:?} unit type is of another kind").into());
    }
    Ok(Some(unit_type))
}

/// Whether the player can recruit one of a unit type in a province now, and
/// the game's reasons why not, as lines of plain text
/// ([`game_text::plain`]): what the game's recruit command checks.
pub fn can_recruit(m: &Memory, ctx: &Ctx, province: usize, unit_type: usize) -> GResult<(bool, Vec<String>)> {
    // A regular unit (special kind 0), in a province of the country's own,
    // with its money, manpower and sailors checked.
    let (checked, text) = game_string::written(m, |reasons| m.call(m.at(CAN_RECRUIT), &[ctx.country, province, unit_type, 0, 0, 1, reasons, 0, 0, 0]))?;
    let allowed = checked as u8 != 0 && m.call(m.at(RECRUIT_PLACE), &[province, unit_type])? as u8 != 0;
    let reasons = game_text::plain(&text).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
    Ok((allowed, reasons))
}

fn recruit(m: &Memory, ctx: &Ctx, province: usize) -> GResult<Vec<Recruit>> {
    UnitKind::ALL
        .into_iter()
        .map(|kind| {
            let Some(unit_type) = unit_type(m, ctx.country, kind)? else {
                return Ok(Recruit { kind, unit_type: None, allowed: false, reasons: Vec::new() });
            };
            let (allowed, reasons) = can_recruit(m, ctx, province, unit_type)?;
            let name = text_at(m, unit_type + unit_type::NAME)?;
            Ok(Recruit { kind, unit_type: Some(name), allowed, reasons: if allowed { Vec::new() } else { reasons } })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `std::string` holding `text` on the heap, as the game keeps a name
    /// longer than 15 bytes.
    fn on_heap(text: &[u8]) -> [u8; game_string::OBJECT] {
        let mut string = [0u8; game_string::OBJECT];
        string[..8].copy_from_slice(&(text.as_ptr() as usize).to_le_bytes());
        string[game_string::SIZE..game_string::SIZE + 8].copy_from_slice(&text.len().to_le_bytes());
        string[game_string::CAPACITY..game_string::CAPACITY + 8].copy_from_slice(&(text.len() + 1).to_le_bytes());
        string
    }

    #[test]
    fn a_mixed_name_reads_as_its_characters() {
        // Windows-1252 bytes for what 1252 has (Î, ·), UTF-8 for the rest.
        let text = b"\xcele-de-France \xb7 \xe5\xb7\xb4\xe9\xbb\x8e".to_vec();
        let mut geometry = vec![0u8; geometry::NAME + game_string::OBJECT];
        geometry[geometry::NAME..].copy_from_slice(&on_heap(&text));
        let m = Memory { base: 0 };
        assert_eq!(text_at(&m, geometry.as_ptr() as usize + geometry::NAME).unwrap(), "Île-de-France · 巴黎");
    }

    #[test]
    fn unit_kinds_are_in_the_games_order() {
        assert_eq!(UnitKind::ALL.map(UnitKind::index), [0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(UnitKind::Galley.index(), 5);
        assert_eq!(UnitKind::ALL.map(UnitKind::key)[3..], ["heavy_ship", "light_ship", "galley", "transport"]);
    }
}
