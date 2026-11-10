//! Colonists: what the province view offers the player for colonizing a
//! province, expelling a minority into a new colony and promoting settlement
//! growth, and carrying each out through the game's own commands
//! (docs/research/envoys.md, "Colonists").
use super::{
    action,
    envoys::{self, quoted},
    with_state, Ctx, GResult, Session, Tag,
};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{construction, country, province as field},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::colonists::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    colonists::{ColonistOptions, Expulsion},
    ProvinceId,
};

/// Whether the country may send a colonist to the province:
/// `(country, province) -> bool`.
const MAY_SEND: GameFunction<2> = GameFunction::at(0x34e7f0);
/// Why not, as the province view's tooltip says: `(country, out, province,
/// _)`, constructing `out`.
const WHY_NOT: GameFunction<4> = GameFunction::at(0x34e890);
/// The province's colonist construction, or null: `(province)`.
const CONSTRUCTION: GameFunction<1> = GameFunction::at(0xa1ba30);
/// The province's construction of a kind, or null: `(province, kind)`.
const CONSTRUCTION_OF: GameFunction<2> = GameFunction::at(0xa1ba90);
/// The kind of a colonist promoting settlement growth.
const GROWTH: usize = 0x38a2;
/// The colonial upkeep a month of the country's colonies and of the provinces
/// a vector names, in thousandths of ducats before the country's factor:
/// `(country, i32* out, text or null, i32* factor in thousandths, vector of
/// provinces or null) -> out`.
const UPKEEP: GameFunction<5> = GameFunction::at(0x34b7a0);
/// The country's factor of the colonial upkeep, thousandths.
const UPKEEP_FACTOR: usize = 0xe14;
/// Whether the country may have a colonist promote settlement growth in the
/// province, and why not: `(country, province, reasons or null) -> bool`,
/// appending to `reasons`.
const MAY_GROW: GameFunction<3> = GameFunction::at(0x262060);
/// Whether the country may expel minorities at all: `(tag by value) ->
/// bool`; whether a province may take expelled minorities: `(province) ->
/// bool`; whether the colonize button lists a province of the player's to
/// expel from: `(province) -> bool`; and whether its minority may be
/// expelled now, and why not: `(province, reasons or null) -> bool`,
/// appending to `reasons`.
const MAY_EXPEL_ANY: GameFunction<1> = GameFunction::at(0x264ec0);
const TAKES_EXPELLED: GameFunction<1> = GameFunction::at(0x264fb0);
const LISTED: GameFunction<1> = GameFunction::at(0x264f50);
const MAY_EXPEL: GameFunction<2> = GameFunction::at(0x264ff0);
/// The most provinces believed.
const MOST: i32 = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_SEND, "40 53 48 83 ec 20 48 8b 81 80 14 00 00 48 8b da", "may send a colonist"),
        Point::keep(WHY_NOT, "44 88 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53 56 57", "why no colonist"),
        Point::keep(CONSTRUCTION, "48 89 5c 24 08 57 48 83 ec 20 48 8b 99 d8 04 00 00 48 85 db 74 23 66 66 0f 1f 84 00 00 00 00 00 48 8b 3b 48 8b 5b 10 48 8b cf 48 8b 07 ff 50 70 3c 0a", "colonist construction"),
        Point::keep(CONSTRUCTION_OF, "48 8b 89 d8 04 00 00 48 85 c9 74 15", "construction of a kind"),
        // The province view's settlement growth button asks for this kind.
        Point::keep(0x137a031usize, "ba a2 38 00 00", "settlement growth construction").uses(&[GROWTH]),
        // The cancel colonist command's check: the location, then the
        // construction's country.
        Point::keep(0x446850usize, "48 63 41 58 85 c0 0f 88 83 00 00 00 80 79 5c 00 75 74 4c 8b 41 50", "cancel colonist check"),
        Point::keep(0x4468d9usize, "48 8b 42 40", "colonist construction's country").uses(&[construction::COUNTRY]),
        Point::keep(UPKEEP, "4c 89 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53", "colonial upkeep"),
        // The budget's colonial upkeep: the sum times the country's factor.
        Point::keep(0xeb78bfusize, "48 63 9f 14 0e 00 00", "colonial upkeep factor").uses(&[UPKEEP_FACTOR]),
        Point::keep(MAY_GROW, "48 89 5c 24 08 48 89 74 24 10 55 57 41 56 48 8d 6c 24 b9 48 81 ec", "may promote settlement growth"),
        Point::keep(MAY_EXPEL_ANY, "40 53 48 83 ec 20 80 3d 67 b0 0d 02 00 48 8b d9 74 09", "may expel minorities"),
        Point::keep(TAKES_EXPELLED, "48 83 ec 28 48 8b 81 68 04 00 00 48 c1 e8 38 84 c0 75 18", "takes expelled minorities"),
        Point::keep(LISTED, "40 53 48 83 ec 20 f6 81 58 09 00 00 10 48 8b d9 74 3f", "lists a province to expel from"),
        Point::keep(MAY_EXPEL, "48 89 5c 24 10 48 89 7c 24 18 55 48 8b ec 48 83 ec 60 48 8b fa 48", "may expel the minority"),
        // The colonize button's list goes through the player's provinces and
        // keeps those of another culture or religion.
        Point::keep(0xe2e868usize, "49 8b 9c 24 38 13 00 00 48 89 5d 7f 49 63 84 24 44 13 00 00", "country provinces").uses(&[country::OWNED, country::OWNED_COUNT]),
        Point::keep(0xe2e95dusize, "4d 8b ae 40 04 00 00 4d 3b ac 24 28 12 00 00", "minority culture").uses(&[field::CULTURE, country::PRIMARY_CULTURE]),
        Point::keep(0xe2eab6usize, "4d 8b ae 48 04 00 00 4d 3b ac 24 30 0f 00 00", "minority religion").uses(&[field::RELIGION, country::STATE_RELIGION]),
    ]
}

struct Province {
    address: usize,
}

fn province(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Province> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    Ok(Province { address: with_state(|s| s.geography.province_address(m, ctx, id))? })
}

/// What the province view offers the player in province `id`.
pub fn options(Session { m, ctx, .. }: Session, args::Options { province: ProvinceId(id) }: args::Options) -> GResult<Box<ColonistOptions>> {
    read(m, &ctx, &province(m, &ctx, id)?, id).map(Box::new)
}

/// The colonial upkeep a month the player's colonies cost, in thousandths of
/// ducats, with the province at `extra` counted too when it is not 0.
fn upkeep(m: &Memory, ctx: &Ctx, extra: usize) -> GResult<i64> {
    let (mut out, factor) = (0i32, 1000i32);
    let provinces = [extra];
    let range = provinces.as_ptr_range();
    let vector = [range.start as usize, range.end as usize, range.end as usize];
    let list = if extra == 0 { 0 } else { vector.as_ptr() as usize };
    UPKEEP.call(m, [ctx.country, &mut out as *mut i32 as usize, 0, &factor as *const i32 as usize, list])?;
    Ok(i64::from(out) * i64::from(m.i32(ctx.country + UPKEEP_FACTOR)?) / 1000)
}

/// What a colonist the player sends to the province costs: the colonist, and
/// the colonial upkeep a month its work there adds.
fn cost(m: &Memory, ctx: &Ctx, p: &Province) -> GResult<Vec<Amount>> {
    let added = upkeep(m, ctx, p.address)? - upkeep(m, ctx, 0)?;
    Ok(vec![Amount::new(Resource::Colonist, 1.0), Amount::new(Resource::MonthlyDucats, added as f64 / 1000.0)])
}

fn read(m: &Memory, ctx: &Ctx, p: &Province, id: u32) -> GResult<ColonistOptions> {
    let construction = CONSTRUCTION.call(m, [p.address])?;
    let working = construction != 0 && Tag(m.u64(construction + construction::COUNTRY)?) == ctx.player;
    let size = m.i32(p.address + field::COLONY_SIZE)?;
    let colony = size > 0 && m.u8(p.address + field::HIDDEN)? & field::CITY_BIT == 0;
    let cost = cost(m, ctx, p)?;
    let send = if MAY_SEND.call(m, [ctx.country, p.address])? as u8 != 0 {
        Choice::can(cost.clone())
    } else {
        let (_, text) = game_string::written(m, |out| WHY_NOT.call(m, [ctx.country, out, p.address, 0]))?;
        Choice::cannot(envoys::lines(&text), cost.clone())
    };
    let recall = if working { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    let growth = CONSTRUCTION_OF.call(m, [p.address, GROWTH])?;
    let growing = growth != 0 && Tag(m.u64(growth + construction::COUNTRY)?) == ctx.player;
    let (may, text) = game_string::written(m, |out| MAY_GROW.call(m, [ctx.country, p.address, out]))?;
    let start_growth = if may as u8 != 0 { Choice::can(cost.clone()) } else { Choice::cannot(envoys::lines(&text), cost.clone()) };
    let stop_growth = if growing { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    Ok(ColonistOptions {
        province: ProvinceId(id),
        working,
        size: colony.then_some(size as f64 / 1000.0),
        expel_from: expulsions(m, ctx, p, &send)?,
        send,
        recall,
        growing,
        start_growth,
        stop_growth,
    })
}

/// The player's provinces the colonize button lists for expelling their
/// minority into the province, each with whether the player can now: as
/// sending a colonist there, and the province's own rules.
fn expulsions(m: &Memory, ctx: &Ctx, p: &Province, send: &Choice) -> GResult<Vec<Expulsion>> {
    if MAY_EXPEL_ANY.call(m, [ctx.player.0 as usize])? as u8 == 0 || TAKES_EXPELLED.call(m, [p.address])? as u8 == 0 {
        return Ok(Vec::new());
    }
    let count = m.i32(ctx.country + country::OWNED_COUNT)?;
    if !(0..MOST).contains(&count) {
        return Err(format!("{count} provinces").into());
    }
    let (ids, mut out) = (m.ptr(ctx.country + country::OWNED)?, Vec::new());
    for i in 0..count as usize {
        let from = m.u32(ids + i * 4)?;
        let address = with_state(|s| s.geography.province_address(m, ctx, from))?;
        let minority = m.ptr(address + field::CULTURE)? != m.ptr(ctx.country + country::PRIMARY_CULTURE)?
            || m.ptr(address + field::RELIGION)? != m.ptr(ctx.country + country::STATE_RELIGION)?;
        if !minority || LISTED.call(m, [address])? as u8 == 0 {
            continue;
        }
        let choice = if !send.can {
            send.clone()
        } else {
            let (may, text) = game_string::written(m, |out| MAY_EXPEL.call(m, [address, out]))?;
            if may as u8 != 0 {
                send.clone()
            } else {
                Choice::cannot(envoys::lines(&text), send.cost.clone())
            }
        };
        out.push(Expulsion { from: ProvinceId(from), choice });
    }
    Ok(out)
}

/// What a colonist the player sends to the province does there.
enum Work {
    Colonize,
    Recall,
    Expel { from: u32 },
    StartGrowth,
    StopGrowth,
}

/// The command each of the province view's colonist buttons posts.
fn written(player: Tag, province: u32, work: Work) -> String {
    let country = quoted(player);
    match work {
        Work::Colonize => format!("send_colonist={{ country={country} location={province} }}"),
        Work::Recall => format!("cancel_colonist={{ country={country} location={province} }}"),
        Work::Expel { from } => format!("send_colonist={{ country={country} location={province} expulsion_province={from} }}"),
        Work::StartGrowth => format!("send_colonist={{ country={country} location={province} settlement_growth_construction=yes }}"),
        Work::StopGrowth => format!("cancel_colonist={{ country={country} location={province} settlement_growth_construction=yes }}"),
    }
}

/// Sends one of the player's colonists to province `id`.
pub fn send(Session { m, ctx, commands, .. }: Session, args::Send { province: ProvinceId(id) }: args::Send) -> GResult<()> {
    let options = read(m, &ctx, &province(m, &ctx, id)?, id)?;
    if !options.send.can {
        return Err(action::not_allowed("send a colonist there", &options.send.reasons));
    }
    action::post(commands, &written(ctx.player, id, Work::Colonize), "send a colonist there")
}

/// Recalls the player's colonist from province `id`.
pub fn recall(Session { m, ctx, commands, .. }: Session, args::Recall { province: ProvinceId(id) }: args::Recall) -> GResult<()> {
    if !read(m, &ctx, &province(m, &ctx, id)?, id)?.working {
        return Err(envoys::checks::none_there("colonist"));
    }
    action::post(commands, &written(ctx.player, id, Work::Recall), "recall the colonist")
}

/// Sends one of the player's colonists to found a colony in province
/// `province` with the minority expelled from the player's province `from`.
pub fn expel(
    Session { m, ctx, commands, .. }: Session,
    args::ExpelMinorities { province: ProvinceId(province), from: ProvinceId(from) }: args::ExpelMinorities,
) -> GResult<()> {
    let options = read(m, &ctx, &self::province(m, &ctx, province)?, province)?;
    let Some(offered) = options.expel_from.iter().find(|e| e.from.0 == from) else {
        return Err(envoys::checks::no_minority(from));
    };
    if !offered.choice.can {
        return Err(action::not_allowed("expel that minority there", &offered.choice.reasons));
    }
    action::post(commands, &written(ctx.player, province, Work::Expel { from }), "expel that minority there")
}

/// Has one of the player's colonists promote settlement growth in province
/// `id`, or, `start` false, stops the one there.
pub fn change_growth(Session { m, ctx, commands, .. }: Session, id: u32, start: bool) -> GResult<()> {
    let options = read(m, &ctx, &province(m, &ctx, id)?, id)?;
    if !start {
        // The command's own check takes any province.
        if !options.growing {
            return Err(envoys::checks::none_there("colonist promoting settlement growth"));
        }
        return action::post(commands, &written(ctx.player, id, Work::StopGrowth), "stop promoting settlement growth there");
    }
    if !options.start_growth.can {
        return Err(action::not_allowed("promote settlement growth there", &options.start_growth.reasons));
    }
    action::post(commands, &written(ctx.player, id, Work::StartGrowth), "promote settlement growth there")
}

crate::catalogue::bind! {
    eu4_harmony_remote::colonists: ColonistRequest => ColonistAnswer;
    Options => options;
    Send => send;
    Recall => recall;
    ExpelMinorities => expel;
    StartSettlementGrowth { province } => session change_growth(province, true);
    StopSettlementGrowth { province } => session change_growth(province, false);
}

#[cfg(test)]
mod tests {
    use crate::game::As;
    use super::*;

    #[test]
    fn the_commands_name_the_country_the_province_and_the_work() {
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        assert_eq!(written(fra, 1000, Work::Colonize), r#"send_colonist={ country="FRA" location=1000 }"#);
        assert_eq!(written(fra, 1000, Work::Recall), r#"cancel_colonist={ country="FRA" location=1000 }"#);
        assert_eq!(written(fra, 1000, Work::Expel { from: 183 }), r#"send_colonist={ country="FRA" location=1000 expulsion_province=183 }"#);
        assert_eq!(written(fra, 183, Work::StartGrowth), r#"send_colonist={ country="FRA" location=183 settlement_growth_construction=yes }"#);
        assert_eq!(written(fra, 183, Work::StopGrowth), r#"cancel_colonist={ country="FRA" location=183 settlement_growth_construction=yes }"#);
    }

    use crate::catalogue::fake::tur;
    use crate::game::{
        buffers::{self, write_reason, Image, Object, Provinces},
        commands::record,
        Caller, Fail, Status,
    };
    use std::cell::Cell;

    /// What the game's checks answer in this test's thread, and where the
    /// player's colonists work.
    #[derive(Clone, Copy)]
    struct Game {
        send: bool,
        grow: bool,
        expel_any: bool,
        expel: bool,
        /// The province where a colonist of the player's colonizes, and its
        /// construction.
        working: (usize, usize),
        /// The province where one promotes settlement growth, and its
        /// construction.
        growing: (usize, usize),
    }

    const LETS: Game = Game { send: true, grow: true, expel_any: true, expel: true, working: (0, 0), growing: (0, 0) };

    thread_local! {
        static GAME: Cell<Game> = const { Cell::new(LETS) };
    }

    fn game() -> Game {
        GAME.with(Cell::get)
    }

    extern "C" fn construction(province: usize) -> usize {
        let (at, construction) = game().working;
        if province == at {
            construction
        } else {
            0
        }
    }

    extern "C" fn construction_of(province: usize, kind: usize) -> usize {
        let (at, construction) = game().growing;
        if province == at && kind == GROWTH {
            construction
        } else {
            0
        }
    }

    extern "C" fn may_send() -> usize {
        game().send.into()
    }

    extern "C" fn why_not(_: usize, out: usize) -> usize {
        unsafe { write_reason(out, b"- no colonists") };
        0
    }

    /// The upkeep: a ducat a month, two with the province the colonist goes
    /// to counted.
    extern "C" fn upkeep(_: usize, out: *mut i32, _: usize, _: usize, list: usize) -> usize {
        unsafe { *out = if list == 0 { 1000 } else { 2000 } };
        0
    }

    extern "C" fn may_grow(_: usize, _: usize, reasons: usize) -> usize {
        if game().grow {
            return 1;
        }
        unsafe { write_reason(reasons, b"- not a colony") };
        0
    }

    extern "C" fn may_expel_any() -> usize {
        game().expel_any.into()
    }

    extern "C" fn may_expel(_: usize, reasons: usize) -> usize {
        if game().expel {
            return 1;
        }
        unsafe { write_reason(reasons, b"- unrest") };
        0
    }

    /// The Ottomans, owning province 2, of their culture and religion, and 3,
    /// of another culture; province 1 is empty land.
    struct Ottomans {
        _image: Image,
        provinces: Provinces,
        objects: Vec<Object>,
        session: Session,
    }

    fn ottomans(game: Game) -> Ottomans {
        let image = Image::new();
        image
            .strings()
            .function(CONSTRUCTION.rva(), construction as *const ())
            .function(CONSTRUCTION_OF.rva(), construction_of as *const ())
            .function(MAY_SEND.rva(), may_send as *const ())
            .function(WHY_NOT.rva(), why_not as *const ())
            .function(UPKEEP.rva(), upkeep as *const ())
            .function(MAY_GROW.rva(), may_grow as *const ())
            .function(MAY_EXPEL_ANY.rva(), may_expel_any as *const ())
            .function(TAKES_EXPELLED.rva(), buffers::yes as *const ())
            .function(LISTED.rva(), buffers::yes as *const ())
            .function(MAY_EXPEL.rva(), may_expel as *const ());
        let provinces = Provinces::new(&image, 4);
        let (turkish, sunni, greek) = (Object::new(8), Object::new(8), Object::new(8));
        provinces.owner(2, tur()).u64(2, field::CULTURE, turkish.at() as u64).u64(2, field::RELIGION, sunni.at() as u64);
        provinces.owner(3, tur()).u64(3, field::CULTURE, greek.at() as u64).u64(3, field::RELIGION, sunni.at() as u64);
        let owned = Object::new(8).u32(0, 2).u32(4, 3);
        let player = Object::country(tur(), country::OWNED_COUNT + 4)
            .points_to(country::OWNED, owned)
            .i32(country::OWNED_COUNT, 2)
            .u64(country::PRIMARY_CULTURE, turkish.at() as u64)
            .u64(country::STATE_RELIGION, sunni.at() as u64)
            .i32(UPKEEP_FACTOR, 1000);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), provinces.world(), tur(), player.at(), countries.at(), Caller::Remote(As::Player));
        GAME.with(|g| g.set(game));
        Ottomans { _image: image, provinces, objects: vec![turkish, sunni, greek, player, countries], session }
    }

    impl Ottomans {
        /// A colonist of the player's working in province `id`, colonizing or
        /// promoting settlement growth.
        fn working(mut self, id: u32, growth: bool) -> Self {
            let work = Object::new(construction::COUNTRY + 8).u64(construction::COUNTRY, tur().0);
            let place = (self.provinces.province(id), work.at());
            GAME.with(|g| g.set(if growth { Game { growing: place, ..g.get() } } else { Game { working: place, ..g.get() } }));
            self.objects.push(work);
            self
        }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_view_offers_a_colonist_for_its_upkeep_and_the_minorities_to_expel() {
        let player = ottomans(LETS);
        let options = options(player.session, args::Options { province: ProvinceId(1) }).unwrap();
        assert_eq!((options.working, options.growing, options.size), (false, false, None));
        assert_eq!(options.send.cost, [Amount::new(Resource::Colonist, 1.0), Amount::new(Resource::MonthlyDucats, 1.0)]);
        assert!(options.send.can && options.start_growth.can && !options.recall.can && !options.stop_growth.can);
        assert_eq!(options.expel_from.iter().map(|e| (e.from.0, e.choice.can)).collect::<Vec<_>>(), [(3, true)]);
        // A test has one world of provinces at a time.
        drop(player);
        let no_expelling = ottomans(Game { expel_any: false, ..LETS });
        assert!(super::options(no_expelling.session, args::Options { province: ProvinceId(1) }).unwrap().expel_from.is_empty());
    }

    #[test]
    fn the_buttons_post_their_commands() {
        let ottomans = ottomans(LETS).working(2, false).working(3, true);
        let session = ottomans.session;
        send(session, args::Send { province: ProvinceId(1) }).unwrap();
        recall(session, args::Recall { province: ProvinceId(2) }).unwrap();
        expel(session, args::ExpelMinorities { province: ProvinceId(1), from: ProvinceId(3) }).unwrap();
        change_growth(session, 2, true).unwrap();
        change_growth(session, 3, false).unwrap();
        assert_eq!(
            record::posted(),
            [
                r#"send_colonist={ country="TUR" location=1 }"#,
                r#"cancel_colonist={ country="TUR" location=2 }"#,
                r#"send_colonist={ country="TUR" location=1 expulsion_province=3 }"#,
                r#"send_colonist={ country="TUR" location=2 settlement_growth_construction=yes }"#,
                r#"cancel_colonist={ country="TUR" location=3 settlement_growth_construction=yes }"#,
            ]
        );
    }

    #[test]
    fn work_the_player_has_not_there_and_no_minority_are_refused() {
        let player = ottomans(LETS);
        let session = player.session;
        assert_eq!(refused(recall(session, args::Recall { province: ProvinceId(1) })), (Status::Rejected, "the player has no colonist there".into()));
        assert_eq!(refused(change_growth(session, 2, false)), (Status::Rejected, "the player has no colonist promoting settlement growth there".into()));
        assert_eq!(
            refused(expel(session, args::ExpelMinorities { province: ProvinceId(1), from: ProvinceId(2) })),
            (Status::Rejected, "the colonize button lists no minority to expel from province 2".into())
        );
        assert_eq!(refused(send(session, args::Send { province: ProvinceId(0) })).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }

    #[test]
    fn the_game_s_reasons_refuse_sending_expelling_and_growth() {
        let no_colonists = ottomans(Game { send: false, ..LETS });
        assert_eq!(refused(send(no_colonists.session, args::Send { province: ProvinceId(1) })), (Status::Rejected, "no colonists".into()));
        // Expelling sends a colonist, so it is refused as sending one is.
        let expelling = args::ExpelMinorities { province: ProvinceId(1), from: ProvinceId(3) };
        assert_eq!(refused(expel(no_colonists.session, expelling.clone())), (Status::Rejected, "no colonists".into()));
        drop(no_colonists);
        let unrest = ottomans(Game { expel: false, ..LETS });
        assert_eq!(refused(expel(unrest.session, expelling)), (Status::Rejected, "unrest".into()));
        drop(unrest);
        let no_growth = ottomans(Game { grow: false, ..LETS });
        assert_eq!(refused(change_growth(no_growth.session, 2, true)), (Status::Rejected, "not a colony".into()));
        assert!(record::posted().is_empty());
    }
}
