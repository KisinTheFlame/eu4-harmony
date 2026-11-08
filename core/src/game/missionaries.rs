//! Missionaries: what the province view's missionary button offers the
//! player in a province, sending and recalling one through the game's own
//! command, and the number of missionaries the game sends by itself
//! (docs/research/envoys.md, "Missionaries").
use super::{
    action,
    envoys::{self, quoted, Kind},
    with_state, Ctx, GResult, Session, Tag,
};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::province as province_fields,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::missionaries::args;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    missionaries::{AutomaticMissionaries, MissionaryOptions},
    ProvinceId,
};

/// Whether the country may send a missionary to the province:
/// `(country, province) -> bool`.
const MAY_SEND: GameFunction<2> = GameFunction::at(0x3546b0);
/// Why not, as the province view's tooltip says: `(country, out, province)`,
/// writing nothing when every rule holds.
const WHY_NOT: GameFunction<3> = GameFunction::at(0x354a00);
/// The yearly upkeep of a missionary the country sends there, before its
/// multiplier: `(country, out i32*, province) -> out`.
const UPKEEP: GameFunction<3> = GameFunction::at(0x34c730);
/// The province's missionary construction, or null: `(province)`.
const CONSTRUCTION: GameFunction<1> = GameFunction::at(0xa1a360);
/// The missionary construction's country.
const SENDER: usize = 0x80;
/// The country's multiplier of the upkeep, thousandths.
const UPKEEP_FACTOR: usize = 0xedc;
/// The province's conversion progress, thousandths of a percent.
const PROGRESS: usize = 0x894;
/// How many of the country's missionaries the game sends by itself.
const AUTOMATIC: usize = 0x14a0;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_SEND, "4c 8b dc 49 89 5b 08 49 89 6b 10 49 89 73 18 49 89 7b 20", "may send a missionary"),
        Point::keep(WHY_NOT, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 20 55 41 54 41 55 41 56 41 57 48 8d 6c 24 c9 48 81 ec", "why no missionary"),
        Point::keep(UPKEEP, "40 53 56 57 41 56 48 83 ec 28 49 8b f0 4c 8b f2 48 8b d9", "missionary upkeep"),
        // The province's constructions, walked for the missionary's.
        Point::keep(0xa1a320usize, "40 53 48 83 ec 20 48 8b 99 d8 04 00 00 48 85 db 74 16 48 8b 0b 48 8b 5b 10 48 8b 01 ff 50 70 3c 05", "has a missionary"),
        Point::keep(CONSTRUCTION, "48 89 5c 24 08 57 48 83 ec 20 48 8b 99 d8 04 00 00 48 85 db 74 23 66 66 0f 1f 84 00 00 00 00 00 48 8b 3b 48 8b 5b 10 48 8b cf 48 8b 07 ff 50 70 3c 05", "missionary construction"),
        // The missionary command's check compares the construction's country
        // with the sender's.
        Point::keep(0xaab0c5usize, "48 8b 88 80 00 00 00", "missionary's country").uses(&[SENDER]),
        // The send missionary tooltip's upkeep a month.
        Point::keep(0x1206e80usize, "48 63 8e dc 0e 00 00", "missionary upkeep factor").uses(&[UPKEEP_FACTOR]),
        Point::keep(0x13797e3usize, "49 8b 45 20 48 63 88 94 08 00 00", "missionary progress").uses(&[PROGRESS]),
        // The automatic missionaries command's execute.
        Point::keep(0x2bf79busize, "41 8b 91 a0 14 00 00", "automatic missionaries").uses(&[AUTOMATIC]),
    ]
}

struct Province {
    address: usize,
    owner: Tag,
}

fn province(m: &Memory, ctx: &Ctx, id: u32) -> GResult<Province> {
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    let address = with_state(|s| s.geography.province_address(m, ctx, id))?;
    Ok(Province { address, owner: Tag(m.u64(address + province_fields::OWNER)?) })
}

/// What the province view offers the player in province `id`; in the
/// player's view another's missionary's progress in another country's
/// province is left out, as `province.get` leaves it out.
pub fn options(Session { m, ctx, view, .. }: Session, args::Options { province: ProvinceId(id) }: args::Options) -> GResult<MissionaryOptions> {
    let p = province(m, &ctx, id)?;
    let (working, progress, send, recall) = read(m, &ctx, &p)?;
    let owner = Some(p.owner).filter(|t| t.valid());
    let progress = progress.filter(|_| working || super::player::Eyes::acting(view, ctx.player).sees_inside(owner));
    Ok(MissionaryOptions { province: ProvinceId(id), working, progress, send, recall })
}

/// Whether the player's missionary works there, the conversion's progress,
/// and the send and recall choices.
fn read(m: &Memory, ctx: &Ctx, p: &Province) -> GResult<(bool, Option<f64>, Choice, Choice)> {
    let construction = CONSTRUCTION.call(m, [p.address])?;
    let working = construction != 0 && m.u64(construction + SENDER)? == ctx.player.0;
    let progress = (construction != 0).then(|| m.i32(p.address + PROGRESS).map(|v| v as f64 / 1000.0)).transpose()?;
    let (_, text) = game_string::written(m, |out| WHY_NOT.call(m, [ctx.country, out, p.address]))?;
    let reasons = envoys::lines(&text);
    let may = MAY_SEND.call(m, [ctx.country, p.address])? as u8 != 0;
    let cost = vec![Amount::new(Resource::Missionary, 1.0), Amount::new(Resource::MonthlyDucats, monthly_upkeep(m, ctx, p.address)?)];
    let send = if construction == 0 && may && reasons.is_empty() { Choice::can(cost) } else { Choice::cannot(reasons, cost) };
    let recall = if working { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    Ok((working, progress, send, recall))
}

/// The upkeep a month of a missionary the player sends to the province, in
/// ducats, as the send missionary tooltip shows it.
fn monthly_upkeep(m: &Memory, ctx: &Ctx, province: usize) -> GResult<f64> {
    let mut yearly = 0i32;
    UPKEEP.call(m, [ctx.country, &mut yearly as *mut i32 as usize, province])?;
    let factor = m.i32(ctx.country + UPKEEP_FACTOR)? as i64;
    Ok((yearly as i64 * factor / 1000) as f64 / 1000.0 / 12.0)
}

/// The command the province view's button posts: sending the player's
/// missionary to the province, or recalling it.
fn written(owner: Tag, player: Tag, province: u32, recall: bool) -> String {
    format!("send_missionary={{ country={} owned_by={} location={province}{} }}", quoted(owner), quoted(player), if recall { " cancel=yes" } else { "" })
}

/// Sends one of the player's missionaries to province `id`.
pub fn send(Session { m, ctx, commands, .. }: Session, args::Send { province: ProvinceId(id) }: args::Send) -> GResult<()> {
    let p = province(m, &ctx, id)?;
    let (_, _, send, _) = read(m, &ctx, &p)?;
    if !send.can {
        return Err(action::not_allowed("send a missionary there", &send.reasons));
    }
    action::post(commands, &written(p.owner, ctx.player, id, false), "send a missionary there")
}

/// Recalls the player's missionary from province `id`.
pub fn recall(Session { m, ctx, commands, .. }: Session, args::Recall { province: ProvinceId(id) }: args::Recall) -> GResult<()> {
    let p = province(m, &ctx, id)?;
    if !read(m, &ctx, &p)?.0 {
        return Err(envoys::checks::none_there("missionary"));
    }
    action::post(commands, &written(p.owner, ctx.player, id, true), "recall the missionary")
}

/// The player's missionaries and how many of them the game sends by itself.
pub fn automatic(Session { m, ctx, .. }: Session) -> GResult<AutomaticMissionaries> {
    let (missionaries, _) = envoys::count(m, ctx.country, Kind::Missionary)?;
    let automatic = m.i32(ctx.country + AUTOMATIC)?.max(0) as u32;
    let choice = |can: bool| if can { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    Ok(AutomaticMissionaries { missionaries, automatic, raise: choice(automatic < missionaries), lower: choice(automatic > 0) })
}

/// Has the game send one more (`raise`) or one fewer of the player's
/// missionaries by itself, as the religion view's buttons do.
pub fn set_automatic(session: Session, raise: bool) -> GResult<()> {
    let now = automatic(session)?;
    if raise && now.automatic >= now.missionaries {
        return Err(envoys::checks::all_automatic(now.missionaries));
    }
    if !raise && now.automatic == 0 {
        return Err(envoys::checks::none_automatic());
    }
    let text = format!("automate_missionary_command={{ country={}{} }}", quoted(session.ctx.player), if raise { " value=yes" } else { "" });
    action::post(session.commands, &text, if raise { "have the game send one more missionary" } else { "have the game send one fewer missionary" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_names_the_owner_the_sender_and_the_province() {
        let (fra, eng) = (Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0")), Tag(u64::from_le_bytes(*b"ENG\0\0\0\0\0")));
        assert_eq!(written(eng, fra, 236, false), r#"send_missionary={ country="ENG" owned_by="FRA" location=236 }"#);
        assert_eq!(written(eng, fra, 236, true), r#"send_missionary={ country="ENG" owned_by="FRA" location=236 cancel=yes }"#);
    }

    use crate::game::{
        buffers::{self, answers_no, answers_yes, write_inline, Image, Object, Provinces},
        commands::record,
        envoys::POOLS,
        Caller, Fail, Status,
    };

    /// A province of the test's: its missionary construction, and whether
    /// the game's tooltip says the player may not send one there; read by
    /// the game's functions below, in bytes the province leaves unused.
    const BUILDING: usize = 0x10;
    const BARRED: usize = 0x18;

    extern "C" fn construction(province: usize) -> usize {
        unsafe { *((province + BUILDING) as *const usize) }
    }

    extern "C" fn why_not(_: usize, out: usize, province: usize) -> usize {
        if unsafe { *((province + BARRED) as *const u8) } != 0 {
            write_inline(out, "- not heretic");
        }
        0
    }

    /// France, with two missionaries of which the game sends `automatic` by
    /// itself, in a world of English provinces 1 to 3: France's missionary
    /// works in 2, the tooltip bars 3.
    struct France {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn france(automatic: i32) -> France {
        let image = Image::new();
        image
            .function(CONSTRUCTION.rva(), construction as *const ())
            .function(WHY_NOT.rva(), why_not as *const ())
            .function(MAY_SEND.rva(), answers_yes as *const ())
            .function(UPKEEP.rva(), answers_no as *const ())
            .function(game_string::DESTROY_AT.rva(), answers_no as *const ());
        let (fra, eng) = (Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0")), Tag(u64::from_le_bytes(*b"ENG\0\0\0\0\0")));
        let provinces = Provinces::new(&image, 4);
        for id in 1..4 {
            provinces.u64(id, province_fields::OWNER, eng.0);
        }
        let missionary = Object::new(SENDER + 8).u64(SENDER, fra.0);
        provinces.u64(2, BUILDING, missionary.at() as u64).u8(3, BARRED, 1);
        let france = Object::country(fra, AUTOMATIC + 4).points_to(POOLS, buffers::envoys(&[(Kind::Missionary, &[true, false])])).i32(AUTOMATIC, automatic);
        let countries = Object::countries(&[&france]);
        let session = buffers::session(image.memory(), provinces.world(), fra, france.at(), countries.at(), Caller::Remote);
        France { _image: image, _provinces: provinces, _objects: vec![missionary, france, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the missionary went") };
        (status, message)
    }

    #[test]
    fn a_missionary_is_sent_and_recalled_by_the_province_view_s_command() {
        let france = france(0);
        send(france.session, args::Send { province: ProvinceId(1) }).unwrap();
        recall(france.session, args::Recall { province: ProvinceId(2) }).unwrap();
        assert_eq!(
            record::posted(),
            [r#"send_missionary={ country="ENG" owned_by="FRA" location=1 }"#, r#"send_missionary={ country="ENG" owned_by="FRA" location=2 cancel=yes }"#]
        );
    }

    #[test]
    fn a_missionary_the_province_view_does_not_offer_is_refused() {
        let france = france(0);
        assert_eq!(refused(send(france.session, args::Send { province: ProvinceId(3) })), (Status::Rejected, "not heretic".into()));
        // One works in 2 already.
        assert_eq!(refused(send(france.session, args::Send { province: ProvinceId(2) })).0, Status::Rejected);
        assert_eq!(refused(recall(france.session, args::Recall { province: ProvinceId(1) })), (Status::Rejected, "the player has no missionary there".into()));
        assert_eq!(refused(send(france.session, args::Send { province: ProvinceId(0) })).0, Status::NotFound);
        assert_eq!(refused(send(france.session, args::Send { province: ProvinceId(4) })).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }

    #[test]
    fn the_game_sends_one_more_or_one_fewer_missionary_within_the_player_s() {
        set_automatic(france(1).session, true).unwrap();
        set_automatic(france(1).session, false).unwrap();
        assert_eq!(record::posted(), [r#"automate_missionary_command={ country="FRA" value=yes }"#, r#"automate_missionary_command={ country="FRA" }"#]);
        let rejected = |why: &str| (Status::Rejected, why.to_string());
        assert_eq!(refused(set_automatic(france(2).session, true)), rejected("the game already sends all of the player's 2 missionaries by itself"));
        assert_eq!(refused(set_automatic(france(0).session, false)), rejected("the game sends none of the player's missionaries by itself"));
        assert!(record::posted().is_empty());
    }
}
