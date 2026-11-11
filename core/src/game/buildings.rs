//! Buildings: what the province view's building panel offers the player in a
//! province, building, cancelling and destroying a building there through the
//! game's own commands, and the event of a building finished
//! (docs/research/province-view.md).
use super::administration::checks as refuse;
use super::commands::{write, Block};
use super::{action, administration::MOST_CONSTRUCTIONS, context_any, events, game, thousandths, with_state, Ctx, GResult, Session, Tag};
use crate::containers;
use crate::{
    game_string,
    hooks::Call,
    memory::Memory,
    objects::{
        campaign::{building as fields, construction, country, province as field},
        list,
    },
    points::{GameFunction, Point},
};
use eu4_harmony_remote::buildings::{args, BuildingConstruction, BuildingOption, BuildingOptions};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::{Date, ProvinceId};

/// The building database, made by its getter: its buildings by index, a
/// vector of pointers at `+0x10`, index 0 the placeholder for none.
const DATABASE: usize = 0x242ba88;
const BY_INDEX: usize = 0x10;
/// Whether a country may build a building in a province: (country,
/// building, province, reasons, 0, 0, 0), as the build command asks it but
/// with the reasons; and its price, (country, out, building, province), the
/// out fixed-point ducats.
const CAN_BUILD: GameFunction<7> = GameFunction::at(0x32acb0);
const PRICE: GameFunction<4> = GameFunction::at(0x32a420);
/// Whether a country may cancel the building being built: (province,
/// building, tag, reasons).
const CAN_CANCEL: GameFunction<4> = GameFunction::at(0xa1b540);
/// A building construction's kind, and its fields besides a
/// [`construction`]'s: its builder's tag and its province.
const BUILDING_CONSTRUCTION: u32 = 0x28c7;
const BUILDER: usize = 0x88;
const PROVINCE: usize = 0x38;
/// The building construction's class, and its method that finishes it,
/// `(construction)`: the building is put in the province.
const CONSTRUCTION_CLASS: usize = 0x1c4b498;
const FINISH: usize = 0x253810;
/// The most buildings believed.
const MOST_BUILDINGS: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x1e539ausize, "48 8b 05 e7 66 24 02", "building database").uses(&[DATABASE]),
        // The build command's reader takes the building by its index.
        Point::keep(0x26936dusize, "48 8b 50 10 48 8b 40 18 48 2b c2 48 c1 f8 03 3b c8 7d 13 48 8b 04 ca", "buildings by index").uses(&[BY_INDEX]),
        Point::keep(CAN_BUILD, "48 89 5c 24 18 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54", "build check"),
        Point::keep(PRICE, "48 89 5c 24 18 56 57 41 56 48 83 ec 20 49 8b f1 4c 8b f2 4c", "building price"),
        Point::keep(CAN_CANCEL, "48 89 5c 24 08 48 89 6c 24 18 48 89 74 24 20 57 48 83 ec 60", "cancel building check"),
        // The cancel check reads the construction's building and builder.
        Point::keep(0xa1b576usize, "48 8b 80 80 00 00 00", "construction building").uses(&[construction::MAKES]),
        Point::keep(0xa1b663usize, "48 8b 81 88 00 00 00", "construction builder").uses(&[BUILDER]),
        // The building construction's constructor sets its class.
        Point::keep(0x253358usize, "48 8d 05 39 81 9f 01 48 89 01", "building construction class").uses(&[CONSTRUCTION_CLASS]),
        Point::method(CONSTRUCTION_CLASS, 0xb8, 0x24fff0, "construction progress method"),
        // Finishing it reads its province and building.
        Point::method(CONSTRUCTION_CLASS, 0x48, FINISH, "building construction finish"),
        Point::observe(FINISH, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24", "building finished", 1, Some(finished), None),
        Point::keep(0x25382fusize, "48 8b 41 38", "construction province").uses(&[PROVINCE]),
        Point::keep(0x253854usize, "48 8b 91 80 00 00 00", "finished building").uses(&[construction::MAKES]),
        // The command types.
        Point::method(0x1c4e490, 0x40, 0x2694b0, "build command check"),
        Point::method(0x1c4db60, 0x40, 0x271c30, "cancel building command check"),
        Point::method(0x1c4e0a0, 0x40, 0x26a3b0, "destroy building command check"),
    ]
}

/// A building finished: its province, the building's index, and the
/// province's owner, heard while the game finishes it, when someone listens.
fn finished(call: &mut Call) {
    if !events::listened(eu4_harmony_remote::events::Kind::BuildingFinished) {
        return;
    }
    let m = &call.memory();
    let construction = call.arg(0);
    let read = || -> crate::Result<Option<events::Change>> {
        if context_any(m).is_err() || m.ptr(construction)? != m.at(CONSTRUCTION_CLASS) {
            return Ok(None);
        }
        let province = m.ptr(construction + PROVINCE)?;
        let building = m.ptr(construction + construction::MAKES)?;
        let owner = Tag(m.u64(province + field::OWNER)?);
        let index = m.u32(building + fields::INDEX)?;
        let province = m.u32(province + field::ID)?;
        Ok((index != 0 && owner.valid()).then_some(events::Change::BuildingFinished { province, building: index, country: owner }))
    };
    match super::event_costs::BUILDINGS.time(read) {
        Ok(Some(change)) => events::raise(change),
        Ok(None) => {}
        Err(e) => log!("buildings: {e}"),
    }
}

/// The buildings the game has, by index, index 0 the placeholder.
fn database(m: &Memory) -> GResult<Vec<usize>> {
    let database = m.ptr(m.at(DATABASE))?;
    if database == 0 {
        return Ok(Vec::new());
    }
    let begin = m.ptr(database + BY_INDEX)?;
    let count = containers::span(begin, m.ptr(database + BY_INDEX + 8)?, 8, MOST_BUILDINGS)?;
    (0..count).map(|i| Ok(m.ptr(begin + i * 8)?)).collect()
}

fn key(m: &Memory, building: usize) -> GResult<String> {
    Ok(game_string::key(m, building + fields::NAME)?)
}

/// The key of the building with index `index`.
pub fn key_of(index: u32) -> GResult<String> {
    let m = game()?;
    let Some(&building) = database(m)?.get(index as usize) else {
        return Err(format!("no building {index}").into());
    };
    key(m, building)
}

/// The building whose key is `key`, and its index.
fn named(m: &Memory, wanted: &str) -> GResult<(usize, u32)> {
    for (index, &building) in database(m)?.iter().enumerate().skip(1) {
        if key(m, building)? == wanted {
            if m.u32(building + fields::INDEX)? as usize != index {
                return Err(format!("building {wanted} is not at its index").into());
            }
            return Ok((building, index as u32));
        }
    }
    Err(refuse::no_such_building(wanted))
}

/// Whether the building stands in the province.
fn stands(m: &Memory, province: usize, index: u32) -> GResult<bool> {
    let first = m.ptr(province + field::BUILDINGS)?;
    let count = containers::span(first, m.ptr(province + field::BUILDINGS + 8)?, field::BUILDING_SLOT, MOST_BUILDINGS)?;
    if index as usize >= count {
        return Ok(false);
    }
    let slot = m.ptr(first + index as usize * field::BUILDING_SLOT)?;
    Ok(slot != 0 && m.u32(slot + fields::INDEX)? == index)
}

/// The building being built in the province: the construction.
fn construction(m: &Memory, province: usize) -> GResult<Option<usize>> {
    for node in containers::chain(m, m.ptr(province + field::CONSTRUCTIONS)?, list::NEXT, MOST_CONSTRUCTIONS)? {
        let construction = m.ptr(node)?;
        if m.u32(construction + construction::KIND)? == BUILDING_CONSTRUCTION {
            return Ok(Some(construction));
        }
    }
    Ok(None)
}

fn province_address(m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    with_state(|s| s.geography.province_address(m, ctx, id))
}

/// Whether the player can build `building` in the province, the game's
/// reasons why not, and the price. The build command's check also wants the
/// price in the treasury.
fn can_build(m: &Memory, ctx: &Ctx, province: usize, building: usize) -> GResult<Choice> {
    let (may, text) = game_string::written(m, |out| CAN_BUILD.call(m, [ctx.country, building, province, out, 0, 0, 0]).map_err(super::Fail::from))?;
    let mut price = 0i32;
    PRICE.call(m, [ctx.country, &mut price as *mut i32 as usize, building, province])?;
    let cost = vec![Amount::new(Resource::Ducats, thousandths(price))];
    let treasury = m.i32(ctx.country + country::TREASURY)?;
    if may as u8 != 0 && price <= treasury {
        return Ok(Choice::can(cost));
    }
    let mut reasons = super::administration::reasons(&text);
    // The check leaves the price to the command's, which says nothing.
    if price > treasury {
        reasons.push(format!("the treasury has {} ducats, and this costs {}", thousandths(treasury), thousandths(price)));
    }
    Ok(Choice::cannot(reasons, cost))
}

/// What the building panel offers the player in a province now, and what is
/// being built there; in the player's view not in another country's
/// province, as the province view does not show it.
pub fn options(Session { m, ctx, view, .. }: Session, args::Options { province: asked }: args::Options) -> GResult<BuildingOptions> {
    let ProvinceId(id) = asked;
    let province = province_address(m, &ctx, id)?;
    let owner = Some(Tag(m.u64(province + field::OWNER)?)).filter(|t| t.valid());
    let owned = owner == Some(ctx.player);
    let seen = super::player::Eyes::acting(view, ctx.player).sees_inside(owner);
    let mut offers = Vec::new();
    let constructing = match construction(m, province)?.filter(|_| seen) {
        None => None,
        Some(construction) => {
            let building = m.ptr(construction + construction::MAKES)?;
            let builder = Tag(m.u64(construction + BUILDER)?);
            let (may, text) = game_string::written(m, |out| CAN_CANCEL.call(m, [province, building, ctx.player.0 as usize, out]).map_err(super::Fail::from))?;
            let choice = if may as u8 != 0 { Choice::can(Vec::new()) } else { Choice::cannot(super::administration::reasons(&text), Vec::new()) };
            let key = key(m, building)?;
            offers.push(offer("cancel", key.clone(), choice));
            Some(BuildingConstruction {
                building: key,
                progress: thousandths(m.i32(construction + construction::PROGRESS)?),
                finished: Date(m.i32(construction + construction::END)?),
                builder: crate::catalogue::tag(builder),
            })
        }
    };
    for (index, &building) in database(m)?.iter().enumerate().skip(1) {
        let key = key(m, building)?;
        if stands(m, province, index as u32)? {
            let choice = if owned { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
            offers.push(offer("destroy", key, choice));
        } else {
            offers.push(offer("build", key, can_build(m, &ctx, province, building)?));
        }
    }
    Ok(BuildingOptions { province: asked, constructing, options: offers })
}

/// One thing the building panel offers: the capability that does it,
/// `build`, `cancel` or `destroy`, for a building.
fn offer(action: &str, building: String, choice: Choice) -> BuildingOption {
    BuildingOption { action: action.into(), building, choice }
}

/// The building commands: a building by its index.
fn build_text(tag: Tag, province: u32, index: u32) -> String {
    write("constructbuilding", Block::new().word("building", index).word("province", province).tag("builder", tag.letters()))
}

fn cancel_text(tag: Tag, province: u32, index: u32) -> String {
    write("cancel_building", Block::new().tag("country", tag.letters()).word("province", province).word("building", index))
}

fn destroy_text(tag: Tag, province: u32, index: u32) -> String {
    write("destroybuilding", Block::new().word("building", index).tag("country", tag.letters()).word("province", province))
}

/// Starts building `key` in the province for the player.
pub fn build(Session { m, ctx, commands, .. }: Session, args::Build { province: ProvinceId(id), building: key }: args::Build) -> GResult<()> {
    let province = province_address(m, &ctx, id)?;
    let (building, index) = named(m, &key)?;
    let choice = can_build(m, &ctx, province, building)?;
    if !choice.can {
        return Err(action::not_allowed("build it there", &choice.reasons));
    }
    action::post(commands, &build_text(ctx.player, id, index), "build it")
}

/// Cancels the building the player is building in the province.
pub fn cancel(Session { m, ctx, commands, .. }: Session, args::Cancel { province: ProvinceId(id) }: args::Cancel) -> GResult<()> {
    let province = province_address(m, &ctx, id)?;
    let Some(construction) = construction(m, province)? else {
        return Err(refuse::not_doing("building in", id));
    };
    let building = m.ptr(construction + construction::MAKES)?;
    let (may, text) = game_string::written(m, |out| CAN_CANCEL.call(m, [province, building, ctx.player.0 as usize, out]).map_err(super::Fail::from))?;
    if may as u8 == 0 {
        return Err(action::not_allowed("cancel it there", &super::administration::reasons(&text)));
    }
    action::post(commands, &cancel_text(ctx.player, id, m.u32(building + fields::INDEX)?), "cancel it")
}

/// Destroys the building `key` in one of the player's provinces.
pub fn destroy(Session { m, ctx, commands, .. }: Session, args::Destroy { province: ProvinceId(id), building: key }: args::Destroy) -> GResult<()> {
    let province = province_address(m, &ctx, id)?;
    let (_, index) = named(m, &key)?;
    if Tag(m.u64(province + field::OWNER)?) != ctx.player {
        return Err(refuse::not_owned(id));
    }
    if !stands(m, province, index)? {
        return Err(refuse::building_absent(&key, id));
    }
    action::post(commands, &destroy_text(ctx.player, id, index), "destroy it")
}

crate::catalogue::bind! {
    eu4_harmony_remote::buildings: BuildingRequest => BuildingAnswer;
    Options => options;
    Build => build;
    Cancel => cancel;
    Destroy => destroy;
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAS: Tag = Tag(0x0100_0001_0053_4143);

    #[test]
    fn the_commands_name_the_building_by_its_index() {
        assert_eq!(build_text(CAS, 217, 12), r#"constructbuilding={ building=12 province=217 builder="CAS" }"#);
        assert_eq!(cancel_text(CAS, 217, 12), r#"cancel_building={ country="CAS" province=217 building=12 }"#);
        assert_eq!(destroy_text(CAS, 217, 12), r#"destroybuilding={ building=12 country="CAS" province=217 }"#);
    }

    use crate::catalogue::fake::{byz, tur};
    use crate::game::{
        buffers::{self, write_reason, Image, Object, Provinces},
        commands::record,
        Caller, Fail, Status,
    };

    /// The build check: any building but a temple, as the game writes why.
    extern "C" fn can_build(_: usize, building: usize, _: usize, out: usize) -> usize {
        if game_string::key(&Memory { base: 0 }, building + fields::NAME).unwrap() == "temple" {
            unsafe { write_reason(out, b"- needs a tech") };
            return 0;
        }
        1
    }

    /// Every building's price: 100 ducats.
    extern "C" fn price(_: usize, out: *mut i32) -> usize {
        unsafe { *out = 100_000 };
        0
    }

    /// The Ottomans with `treasury` thousandths of ducats, and the building
    /// database of the marketplace, the temple and the workshop (indices 1
    /// to 3). Province 1 is theirs, with a marketplace, and a workshop half
    /// built by them; province 2 is Byzantium's, with a marketplace; province
    /// 3 is theirs, with nothing.
    struct Ottomans {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn ottomans(treasury: i32) -> Ottomans {
        let image = Image::new();
        image
            .strings()
            .function(CAN_BUILD.rva(), can_build as *const ())
            .function(PRICE.rva(), price as *const ())
            .function(CAN_CANCEL.rva(), buffers::yes as *const ());
        let building = |index: u32, key: &str| Object::new(fields::NAME + 0x20).u32(fields::INDEX, index).string(fields::NAME, key);
        let (none, marketplace, temple, workshop) = (building(0, ""), building(1, "marketplace"), building(2, "temple"), building(3, "workshop"));
        let database = Object::new(BY_INDEX + 16).vector(BY_INDEX, &[none.at() as u64, marketplace.at() as u64, temple.at() as u64, workshop.at() as u64]);
        image.u64(DATABASE, database.at() as u64);
        let slots = Object::new(4 * field::BUILDING_SLOT).u64(field::BUILDING_SLOT, marketplace.at() as u64);
        let (begin, end) = (slots.at() as u64, (slots.at() + 4 * field::BUILDING_SLOT) as u64);
        let construction = Object::new(BUILDER + 8)
            .u32(construction::KIND, BUILDING_CONSTRUCTION)
            .u64(construction::MAKES, workshop.at() as u64)
            .i32(construction::PROGRESS, 500)
            .i32(construction::END, 7)
            .u64(BUILDER, tur().0);
        let node = Object::new(list::NODE_SIZE).u64(list::ITEM, construction.at() as u64);
        let provinces = Provinces::new(&image, 4);
        provinces.owner(1, tur()).u64(1, field::BUILDINGS, begin).u64(1, field::BUILDINGS + 8, end).u64(1, field::CONSTRUCTIONS, node.at() as u64);
        provinces.owner(2, byz()).u64(2, field::BUILDINGS, begin).u64(2, field::BUILDINGS + 8, end);
        provinces.owner(3, tur());
        let player = Object::country(tur(), country::TREASURY + 4).i32(country::TREASURY, treasury);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), provinces.world(), tur(), player.at(), countries.at(), Caller::Remote);
        let objects = vec![none, marketplace, temple, workshop, database, slots, construction, node, player, countries];
        Ottomans { _image: image, _provinces: provinces, _objects: objects, session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_panel_offers_cancelling_destroying_and_building_with_the_game_s_reasons() {
        let player = ottomans(500_000);
        let BuildingOptions { constructing, options: offers, .. } = options(player.session, args::Options { province: ProvinceId(1) }).unwrap();
        assert_eq!(
            constructing,
            Some(BuildingConstruction { building: "workshop".into(), progress: 0.5, finished: Date(7), builder: crate::catalogue::tag(tur()) })
        );
        let summary: Vec<_> = offers.iter().map(|o| (o.action.as_str(), o.building.as_str(), o.choice.can)).collect();
        assert_eq!(summary, [("cancel", "workshop", true), ("destroy", "marketplace", true), ("build", "temple", false), ("build", "workshop", true)]);
        assert_eq!(
            (offers[2].choice.reasons.clone(), offers[2].choice.cost.clone()),
            (vec!["needs a tech".to_string()], vec![Amount::new(Resource::Ducats, 100.0)])
        );
        // Another country's building is not the player's to destroy.
        let theirs = options(player.session, args::Options { province: ProvinceId(2) }).unwrap().options;
        assert_eq!((theirs[0].action.as_str(), theirs[0].choice.can), ("destroy", false));
    }

    #[test]
    fn building_cancelling_and_destroying_post_the_panel_s_commands_by_index() {
        let player = ottomans(500_000);
        build(player.session, args::Build { province: ProvinceId(3), building: "workshop".into() }).unwrap();
        cancel(player.session, args::Cancel { province: ProvinceId(1) }).unwrap();
        destroy(player.session, args::Destroy { province: ProvinceId(1), building: "marketplace".into() }).unwrap();
        assert_eq!(
            record::posted(),
            [
                r#"constructbuilding={ building=3 province=3 builder="TUR" }"#,
                r#"cancel_building={ country="TUR" province=1 building=3 }"#,
                r#"destroybuilding={ building=1 country="TUR" province=1 }"#,
            ]
        );
    }

    #[test]
    fn what_the_player_cannot_build_cancel_or_destroy_is_refused() {
        let player = ottomans(500_000);
        let session = player.session;
        assert_eq!(refused(build(session, args::Build { province: ProvinceId(3), building: "temple".into() })), (Status::Rejected, "needs a tech".into()));
        assert_eq!(
            refused(build(session, args::Build { province: ProvinceId(3), building: "palace".into() })),
            (Status::InvalidArgument, "no building palace".into())
        );
        assert_eq!(refused(cancel(session, args::Cancel { province: ProvinceId(3) })), (Status::Rejected, "the player is not building in province 3".into()));
        assert_eq!(
            refused(destroy(session, args::Destroy { province: ProvinceId(2), building: "marketplace".into() })),
            (Status::Rejected, "the player does not own province 2".into())
        );
        assert_eq!(
            refused(destroy(session, args::Destroy { province: ProvinceId(1), building: "temple".into() })),
            (Status::Rejected, "there is no temple in province 1".into())
        );
        assert_eq!(refused(build(session, args::Build { province: ProvinceId(0), building: "workshop".into() })).0, Status::NotFound);
        // A test has one world of provinces at a time.
        drop(player);
        let poor = ottomans(50_000);
        assert_eq!(
            refused(build(poor.session, args::Build { province: ProvinceId(3), building: "workshop".into() })),
            (Status::Rejected, "the treasury has 50 ducats, and this costs 100".into())
        );
        assert!(record::posted().is_empty());
    }
}
