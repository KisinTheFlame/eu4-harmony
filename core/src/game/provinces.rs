//! Provinces: every province, a province's land neighbors, and one
//! province's snapshot: its name, development, terrain, fort, buildings,
//! autonomy, culture, religion, unrest, trade, state, cores and claims, what
//! is being built or done there, and what the player can recruit there
//! (docs/research/provinces.md).
use super::{commands, context, events, player::Eyes, span, thousandths, with_state, Ctx, GResult, InWorld, Session, Tag, World, WorldSession};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{building, classes, construction, country, geometry, globals, map, province as field, terrain, unit_type},
    objects::list,
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::provinces::args;
use eu4_harmony_remote::ProvinceId;
use eu4_harmony_remote::{
    provinces::{Construction, StateKind},
    CountryTag, Date,
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
/// In the player's view a province the player has not discovered has no
/// owner or controller, as the map shows it.
pub fn provinces(WorldSession { m, world, view, .. }: WorldSession) -> GResult<Vec<Province>> {
    let visible = events::visibility_current(m, &world);
    let provinces = with_state(|s| s.geography.provinces(m, &world, visible))?;
    discovered(m, &world, Eyes::of(view, world.player), provinces)
}

/// The provinces numbered in `ids` that there are, in their order, as
/// [`provinces`] has them.
pub fn named(WorldSession { m, world, view, .. }: WorldSession, ids: Vec<u32>) -> GResult<Vec<Province>> {
    let visible = events::visibility_current(m, &world);
    let provinces = with_state(|s| s.geography.named_provinces(m, &world, visible, &ids))?;
    discovered(m, &world, Eyes::of(view, world.player), provinces)
}

/// `provinces` with the owner and controller of those `eyes` see as terra
/// incognita left out.
fn discovered(m: &Memory, world: &World, eyes: Eyes, mut provinces: Vec<Province>) -> GResult<Vec<Province>> {
    if eyes.discovering().is_none() {
        return Ok(provinces);
    }
    for p in provinces.iter_mut().filter(|p| p.id != 0) {
        let address = with_state(|s| s.geography.province_address(m, world, p.id))?;
        if super::world::undiscovered(m, eyes, address)? {
            (p.owner, p.controller) = (None, None);
        }
    }
    Ok(provinces)
}

/// The passable land provinces next to a province; none for a province an
/// army cannot stand on.
pub fn neighbors(WorldSession { m, world, .. }: WorldSession, province: u32) -> GResult<Vec<Neighbor>> {
    with_state(|s| s.geography.neighbors(m, &world, province))
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

/// The unit kind a remote client names.
impl From<eu4_harmony_remote::recruiting::UnitKind> for UnitKind {
    fn from(kind: eu4_harmony_remote::recruiting::UnitKind) -> Self {
        use eu4_harmony_remote::recruiting::UnitKind as Named;
        match kind {
            Named::Infantry => Self::Infantry,
            Named::Cavalry => Self::Cavalry,
            Named::Artillery => Self::Artillery,
            Named::HeavyShip => Self::HeavyShip,
            Named::LightShip => Self::LightShip,
            Named::Galley => Self::Galley,
            Named::Transport => Self::Transport,
        }
    }
}

/// A unit kind as remote clients name it.
impl From<UnitKind> for eu4_harmony_remote::recruiting::UnitKind {
    fn from(kind: UnitKind) -> Self {
        match kind {
            UnitKind::Infantry => Self::Infantry,
            UnitKind::Cavalry => Self::Cavalry,
            UnitKind::Artillery => Self::Artillery,
            UnitKind::HeavyShip => Self::HeavyShip,
            UnitKind::LightShip => Self::LightShip,
            UnitKind::Galley => Self::Galley,
            UnitKind::Transport => Self::Transport,
        }
    }
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
#[derive(Clone, Debug, Default, PartialEq)]
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
    /// Local autonomy as the game shows it, thousandths of a percentage
    /// point.
    pub autonomy: i32,
    /// The keys of its culture, religion and trade good.
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub trade_good: Option<String>,
    /// Unrest as the game shows it, in thousandths; None in the player's
    /// view of another country's province, as the constructions and the
    /// missionary.
    pub unrest: Option<i32>,
    pub trade_node: Option<String>,
    /// 0 for none.
    pub center_of_trade: i32,
    pub trade_company: bool,
    /// None without an owner.
    pub state: Option<StateKind>,
    pub edict: Option<String>,
    pub cores: Vec<CountryTag>,
    pub claims: Vec<CountryTag>,
    pub constructions: Option<Vec<Construction>>,
    /// Thousandths of a percentage point; None when no missionary works
    /// there.
    pub missionary: Option<i32>,
    /// Thousandths of a settler; None for no colony.
    pub colony_size: Option<i32>,
}

/// A province's snapshot; [`Status::NotFound`] for a number that names no
/// province. In the player's view a province another country owns leaves
/// out its unrest, its constructions and its missionary, and one the player
/// has not discovered is refused as terra incognita. With no country to act
/// as, nothing can be recruited there.
pub fn province(WorldSession { m, world, view, .. }: WorldSession, args::Get { province: ProvinceId(id) }: args::Get) -> GResult<Snapshot> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    let address = with_state(|s| s.geography.province_address(m, &world, id))?;
    let eyes = Eyes::of(view, world.player);
    if super::world::undiscovered(m, eyes, address)? {
        return Err(eu4_harmony_checks::provinces::not_discovered(id).into());
    }
    let acting = match world.player.acting() {
        Some(_) => Some(context(m)?),
        None => None,
    };
    snapshot(m, &world, acting.as_ref(), address, eyes)
}

/// [`province`], for what acts as a country: failing with none to act as.
pub fn acting_province(session: Session, id: u32) -> GResult<Snapshot> {
    province(session.world()?, args::Get { province: ProvinceId(id) })
}

/// The game's getters of a province's base tax, production and manpower, as
/// triggers and the province window read them: (unused, out, province).
const BASE_TAX: GameFunction<3> = GameFunction::at(0x6de7a0);
const BASE_PRODUCTION: GameFunction<3> = GameFunction::at(0x6de830);
const BASE_MANPOWER: GameFunction<3> = GameFunction::at(0x6de8c0);
/// Whether a country can recruit a unit type in a province: (country,
/// province, unit type, special kind, any province, check resources,
/// reasons, ...), with the reasons appended to a `std::string`.
const CAN_RECRUIT: GameFunction<10> = GameFunction::at(0x346570);
/// What the recruit command checks after it: (province, unit type); ships
/// need the province's port.
const RECRUIT_PLACE: GameFunction<2> = GameFunction::at(0xa1fe50);
/// The game's local autonomy and unrest as it shows them: (unused, out,
/// province), as the scope values "local_autonomy" and "unrest".
const AUTONOMY: GameFunction<3> = GameFunction::at(0x6de950);
const UNREST: GameFunction<3> = GameFunction::at(0x6de660);

/// The most kinds of terrain or building believed.
const MOST_KINDS: usize = 10_000;

pub fn points() -> Vec<Point> {
    use crate::objects::campaign::countries;
    let getter = |function: GameFunction<3>, bytes: &'static str, what: &'static str, value: &'static [usize]| Point::keep(function, bytes, what).uses(value);
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
        // Local autonomy as the game shows it: the stored value between its bounds.
        Point::keep(
            AUTONOMY,
            "41 8b 80 28 04 00 00 89 02 41 8b 88 f0 2d 00 00 3b c1 0f 4f c8 89 0a 41 8b 80 f4 2d 00 00 3b c8 0f 4c c1 89 02 48 8b c2 c3",
            "local autonomy getter",
        ),
        // Unrest as the game shows it, from the province's own base.
        Point::keep(
            UNREST,
            "4c 89 4c 24 20 53 48 83 ec 20 41 8b 80 e4 2d 00 00 49 8b c8 4c 8d 44 24 48 89 44 24 48 48 8b da e8 9b 8c 32 00 48 8b c3 48 83 c4 20 5b c3",
            "unrest getter",
        ),
        // The province's save writer.
        Point::keep(0x9d782ausize, "49 8b 8f 40 04 00 00 48 8b 01 ff 50 40 84 c0 74 29 49 8b 87 40 04 00 00 8b 58 10", "province culture").uses(&[
            field::CULTURE,
            field::VALID,
            field::CULTURE_TOKEN,
        ]),
        Point::keep(0x9d7864usize, "49 8b 8f 48 04 00 00 48 8b 01 ff 50 40 84 c0 74 2c 49 8b 87 48 04 00 00 8b 98 d0 00 00 00", "province religion").uses(&[
            field::RELIGION,
            field::VALID,
            field::RELIGION_TOKEN,
        ]),
        Point::keep(0x9d7d15usize, "49 8b 87 58 04 00 00 8b 58 10", "province trade good").uses(&[field::TRADE_GOOD, field::TRADE_GOOD_TOKEN]),
        Point::keep(0x9d758eusize, "49 8b 87 e8 00 00 00 48 8b 98 a8 00 00 00 48 83 c3 10", "province trade node").uses(&[
            field::TRADE_NODE,
            field::TRADE_NODE_DEFINITION,
            field::TRADE_NODE_KEY,
        ]),
        Point::keep(0x9d8ea7usize, "49 8b 87 60 09 00 00 48 85 c0 74 15 48 8b 40 08 44 8b 40 14", "province center of trade").uses(&[
            field::CENTER_OF_TRADE,
            field::CENTER_OF_TRADE_DATA,
            field::CENTER_OF_TRADE_LEVEL,
        ]),
        Point::keep(0x9d75c8usize, "41 f6 87 58 09 00 00 04", "province in a trade company").uses(&[field::HIDDEN, field::TRADE_COMPANY_BIT as usize]),
        // is_city: the bit 4 of the byte.
        Point::keep(0x9d7914usize, "41 0f b6 9f 58 09 00 00 c0 eb 04 80 e3 01", "province is a city").uses(&[field::HIDDEN]),
        Point::keep(0x9d7962usize, "41 8b 9f f8 03 00 00", "province colony size").uses(&[field::COLONY_SIZE]),
        Point::keep(0x9d7c64usize, "41 8b 9f 94 08 00 00", "province missionary progress").uses(&[field::MISSIONARY_PROGRESS]),
        Point::keep(0x9d754eusize, "49 8d 97 90 04 00 00", "province cores").uses(&[field::CORES]),
        // Whether a claim is permanent walks the claims.
        Point::keep(
            0xa07294usize,
            "48 8b 81 c0 04 00 00 4c 8b c2 48 85 c0 74 23 4c 8b ca 49 c1 e9 20 66 0f 1f 44 00 00 48 8b 10 48 c1 ea 20 66 41 3b d1 74 10 48 8b 40 10",
            "province claims",
        )
        .uses(&[field::CLAIMS, list::NEXT]),
        // is_state, and has_state_edict through the state.
        Point::keep(0xce2eb2usize, "48 83 bc 0a e0 02 00 00 00", "province state").uses(&[field::STATE]),
        Point::keep(0xcf57f2usize, "48 8b 8c 0a e0 02 00 00 48 85 c9 74 c3 48 8b 49 28 48 85 c9 74 ba 48 8b 43 68 48 39 41 10", "province state edict")
            .uses(&[field::STATE, field::STATE_EDICT, field::EDICT_DEFINITION]),
        Point::keep(0xcf585eusize, "48 83 c2 10", "state edict key").uses(&[field::EDICT_KEY]),
        // The writer walks the constructions, writing each under its kind.
        Point::keep(
            0x9d8a5busize,
            "49 8b 9f d8 04 00 00 48 85 db 74 2f 66 0f 1f 84 00 00 00 00 00 48 8b 33 4c 8b 73 10 49 8b de 8b 56 08",
            "province constructions",
        )
        .uses(&[field::CONSTRUCTIONS, list::NEXT, construction::KIND]),
        Point::keep(0x24f9e8usize, "48 83 c1 20", "construction start").uses(&[construction::START]),
        Point::keep(0xa1bc53usize, "48 8d 4e 24", "construction end").uses(&[construction::END]),
        // The building construction's progress method answers its progress.
        Point::keep(0x24fff0usize, "8b 41 30 89 02 48 8b c2", "construction progress").uses(&[construction::PROGRESS]),
        Point::keep(0xcd1db1usize, "48 8b 4f 40", "construction country").uses(&[construction::COUNTRY]),
        Point::keep(0x25368dusize, "48 8b 87 80 00 00 00 8b 58 14", "building construction's building").uses(&[construction::MAKES, building::INDEX]),
        Point::keep(0x9d9977usize, "48 8b 8b 80 00 00 00", "military construction's unit type").uses(&[construction::MAKES]),
    ]
}

fn snapshot(m: &Memory, ctx: &World, acting: Option<&Ctx>, address: usize, eyes: Eyes) -> GResult<Snapshot> {
    let geometry = m.ptr(address + field::GEOMETRY)?;
    let tag_at = |at: usize| -> GResult<Option<Tag>> { Ok(Some(Tag(m.u64(address + at)?)).filter(|t| t.valid())) };
    let mothballed = m.u8(address + field::MOTHBALL)? == field::MOTHBALLED;
    let owner = tag_at(field::OWNER)?;
    // The province view shows another country's province without these.
    let all = eyes.sees_inside(owner);
    let flags = m.u8(address + field::HIDDEN)?;
    let (state, edict) = state_of(m, address, owner)?;
    let constructions = constructions(m, ctx, address)?;
    let missionary = constructions.iter().any(|c| c.kind == "missionary_construction");
    let colony = m.i32(address + field::COLONY_SIZE)?;
    Ok(Snapshot {
        id: m.u32(address + field::ID)?,
        name: text_at(m, geometry + geometry::NAME)?,
        owner,
        controller: tag_at(field::CONTROLLER)?,
        development: Development {
            tax: base(m, BASE_TAX, address)?,
            production: base(m, BASE_PRODUCTION, address)?,
            manpower: base(m, BASE_MANPOWER, address)?,
        },
        terrain: terrain_of(m, geometry)?,
        fort: Fort { level: m.i32(address + if mothballed { field::FORT_LEVEL_MOTHBALLED } else { field::FORT_LEVEL })?, mothballed },
        buildings: buildings(m, address)?,
        recruit: match acting {
            Some(acting) => recruit(m, acting, address)?,
            None => Vec::new(),
        },
        autonomy: shown(m, AUTONOMY, address)?,
        culture: token_key(m, address, field::CULTURE, field::CULTURE_TOKEN, true)?,
        religion: token_key(m, address, field::RELIGION, field::RELIGION_TOKEN, true)?,
        trade_good: token_key(m, address, field::TRADE_GOOD, field::TRADE_GOOD_TOKEN, false)?,
        unrest: if all { Some(shown(m, UNREST, address)?) } else { None },
        trade_node: trade_node_of(m, address)?,
        center_of_trade: center_of_trade(m, address)?,
        trade_company: flags & field::TRADE_COMPANY_BIT != 0,
        state,
        edict,
        cores: vector_tags(m, ctx, address + field::CORES)?,
        claims: list_items(m, address + field::CLAIMS)?.into_iter().map(|tag| country_tag(m, ctx, Tag(tag as u64))).collect::<GResult<_>>()?,
        constructions: if all { Some(constructions) } else { None },
        missionary: if all && missionary { Some(m.i32(address + field::MISSIONARY_PROGRESS)?) } else { None },
        colony_size: if colony > 0 && flags & field::CITY_BIT == 0 { Some(colony) } else { None },
    })
}

/// One of the province's scope values the game computes as it shows them:
/// (unused, out, province), as [`base`].
fn shown(m: &Memory, getter: GameFunction<3>, province: usize) -> GResult<i32> {
    base(m, getter, province)
}

/// The key of the object at `province + at`, the name of its token at
/// `token`; None for none, or, when `valid`, for one its validity method
/// says is not.
fn token_key(m: &Memory, province: usize, at: usize, token: usize, valid: bool) -> GResult<Option<String>> {
    let object = m.ptr(province + at)?;
    if object == 0 || (valid && m.call(m.vslot(object, field::VALID)?, &[object])? as u8 == 0) {
        return Ok(None);
    }
    let token = m.u32(object + token)?;
    Ok(Some(commands::type_name(m, token).ok_or_else(|| format!("token {token:#x} has no name"))?))
}

/// The key of the province's trade node.
fn trade_node_of(m: &Memory, province: usize) -> GResult<Option<String>> {
    let node = m.ptr(province + field::TRADE_NODE)?;
    if node == 0 {
        return Ok(None);
    }
    let definition = m.ptr(node + field::TRADE_NODE_DEFINITION)?;
    if definition == 0 {
        return Err("a trade node without its definition".to_string().into());
    }
    Ok(Some(text_at(m, definition + field::TRADE_NODE_KEY)?))
}

fn center_of_trade(m: &Memory, province: usize) -> GResult<i32> {
    let center = m.ptr(province + field::CENTER_OF_TRADE)?;
    if center == 0 {
        return Ok(0);
    }
    Ok(m.i32(m.ptr(center + field::CENTER_OF_TRADE_DATA)? + field::CENTER_OF_TRADE_LEVEL)?)
}

/// Whether it is in its owner's state or a territory, and the state's
/// edict.
fn state_of(m: &Memory, province: usize, owner: Option<Tag>) -> GResult<(Option<StateKind>, Option<String>)> {
    if owner.is_none() {
        return Ok((None, None));
    }
    let state = m.ptr(province + field::STATE)?;
    if state == 0 {
        return Ok((Some(StateKind::Territory), None));
    }
    let edict = m.ptr(state + field::STATE_EDICT)?;
    let key = if edict == 0 { None } else { Some(text_at(m, m.ptr(edict + field::EDICT_DEFINITION)? + field::EDICT_KEY)?) };
    Ok((Some(StateKind::State), key))
}

/// The tags in a vector (begin, end) of tags at `vector`, each a country's.
fn vector_tags(m: &Memory, ctx: &impl InWorld, vector: usize) -> GResult<Vec<CountryTag>> {
    let begin = m.ptr(vector)?;
    let count = span(begin, m.ptr(vector + 8)?, 8, MOST_KINDS)?;
    (0..count).map(|i| country_tag(m, ctx, Tag(m.u64(begin + i * 8)?))).collect()
}

/// The items of a [`list`] at `at`, in its order.
fn list_items(m: &Memory, at: usize) -> GResult<Vec<usize>> {
    let mut out = Vec::new();
    let mut node = m.ptr(at + list::HEAD)?;
    while node != 0 {
        if out.len() >= MOST_KINDS {
            return Err("province list bound".to_string().into());
        }
        out.push(m.ptr(node + list::ITEM)?);
        node = m.ptr(node + list::NEXT)?;
    }
    Ok(out)
}

/// A tag the game keeps, checked to be a country's.
fn country_tag(m: &Memory, ctx: &impl InWorld, tag: Tag) -> GResult<CountryTag> {
    super::country(m, ctx, tag)?;
    Ok(CountryTag(tag.letters()))
}

/// The constructions in the province, each with its kind's key.
fn constructions(m: &Memory, ctx: &impl InWorld, province: usize) -> GResult<Vec<Construction>> {
    let mut out = Vec::new();
    for c in list_items(m, province + field::CONSTRUCTIONS)? {
        let token = m.u32(c + construction::KIND)?;
        let kind = commands::type_name(m, token).ok_or_else(|| format!("construction token {token:#x} has no name"))?;
        let country = Tag(m.u64(c + construction::COUNTRY)?);
        let makes = match kind.as_str() {
            "building_construction" => Some(text_at(m, m.ptr(c + construction::MAKES)? + building::NAME)?),
            "military_construction" => {
                let made = m.ptr(c + construction::MAKES)?;
                if made != 0 && m.ptr(made)? == m.at(classes::UNIT_TYPE) {
                    Some(text_at(m, made + unit_type::NAME)?)
                } else {
                    None
                }
            }
            _ => None,
        };
        out.push(Construction {
            kind,
            country: if country.valid() { Some(country_tag(m, ctx, country)?) } else { None },
            progress: thousandths(m.i32(c + construction::PROGRESS)?),
            start: Date(m.i32(c + construction::START)?),
            end: Date(m.i32(c + construction::END)?),
            makes,
        });
    }
    Ok(out)
}

/// The game text of the `std::string` at `string`, as Unicode: a name or a
/// key.
fn text_at(m: &Memory, string: usize) -> GResult<String> {
    Ok(game_string::key(m, string)?)
}

/// One of the base development getters.
fn base(m: &Memory, getter: GameFunction<3>, province: usize) -> GResult<i32> {
    let mut out = 0i32;
    let ptr = &mut out as *mut i32 as usize;
    if getter.call(m, [0, ptr, province])? != ptr {
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
    let (checked, text) = game_string::written(m, |reasons| CAN_RECRUIT.call(m, [ctx.country, province, unit_type, 0, 0, 1, reasons, 0, 0, 0]))?;
    let allowed = checked as u8 != 0 && RECRUIT_PLACE.call(m, [province, unit_type])? as u8 != 0;
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
        let mut geometry = [0u8; geometry::NAME + game_string::OBJECT];
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
