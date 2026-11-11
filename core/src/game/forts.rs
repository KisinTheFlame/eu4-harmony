//! Forts: the player's forts, and mothballing and reopening them with the
//! game's command that the province view's fort maintenance box posts
//! (docs/research/sieges.md, "Mothballing and reopening a fort").
use super::commands::{write, Block};
use super::units::checks;
use super::{action, with_state, Ctx, GResult, Session};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{
        country,
        province::{FORT_LEVEL, MOTHBALL, MOTHBALLED},
    },
    points::{GameFunction, Point},
};
use eu4_harmony_remote::forts::args;
use eu4_harmony_remote::{choices::Choice, forts::PlayerFort, ProvinceId};

/// A province's byte that says it has a fort.
const HAS_FORT: usize = 0x2dd8;
/// `(tag by value, province ID, text or null)`: whether the country may
/// mothball or reopen the fort, with the box's reasons.
const MAY_MOTHBALL: GameFunction<3> = GameFunction::at(0x5db5a0);
/// The most provinces read.
const MOST: i32 = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // The military view's "mothball all" walks the country's provinces
        // and asks each whether it has a fort.
        Point::keep(0x5dbc48usize, "49 8b bd 38 13 00 00 49 63 85 44 13 00 00", "country provinces").uses(&[country::OWNED, country::OWNED_COUNT]),
        Point::keep(0x5dbc71usize, "80 bb d8 2d 00 00 00", "province has a fort").uses(&[HAS_FORT]),
        Point::keep(MAY_MOTHBALL, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 40 49 8b d8 41 b0 01", "may mothball a fort"),
    ]
}

/// The command the box posts, which mothballs an open fort and reopens a
/// mothballed one.
fn toggle_text(country: [u8; 3], province: u32) -> String {
    write("mothball_fort", Block::new().word("province", province).tag("country", country))
}

fn read(m: &Memory, ctx: &Ctx, id: u32, address: usize) -> GResult<PlayerFort> {
    let (may, text) = game_string::written(m, |t| MAY_MOTHBALL.call(m, [ctx.player.0 as usize, id as usize, t]))?;
    let choice = if may as u8 != 0 { Choice::can(Vec::new()) } else { Choice::cannot(super::reasons(&text), Vec::new()) };
    Ok(PlayerFort { province: ProvinceId(id), level: m.i32(address + FORT_LEVEL)?, mothballed: m.u8(address + MOTHBALL)? == MOTHBALLED, choice })
}

/// The player's provinces with a fort.
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<PlayerFort>> {
    let count = m.i32(ctx.country + country::OWNED_COUNT)?;
    if !(0..MOST).contains(&count) {
        return Err(format!("{count} provinces").into());
    }
    let ids = m.ptr(ctx.country + country::OWNED)?;
    let mut out = Vec::new();
    for i in 0..count as usize {
        let id = m.u32(ids + i * 4)?;
        let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
        if m.u8(address + HAS_FORT)? != 0 {
            out.push(read(m, &ctx, id, address)?);
        }
    }
    Ok(out)
}

/// Mothballs the fort (`mothball` true) or reopens it.
fn set(Session { m, ctx, commands, .. }: Session, id: u32, mothball: bool) -> GResult<()> {
    let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
    if m.u8(address + HAS_FORT)? == 0 || m.u64(address + crate::objects::campaign::province::OWNER)? != ctx.player.0 {
        return Err(checks::no_fort(id));
    }
    let fort = read(m, &ctx, id, address)?;
    if fort.mothballed == mothball {
        return Err(checks::fort_already(id, mothball));
    }
    let doing = if mothball { "mothball this fort" } else { "reopen this fort" };
    action::carry_out(commands, &fort.choice, doing, || Ok(toggle_text(ctx.player.letters(), id)))
}

pub fn mothball(session: Session, args::Mothball { province: ProvinceId(province) }: args::Mothball) -> GResult<()> {
    set(session, province, true)
}

pub fn reopen(session: Session, args::Reopen { province: ProvinceId(province) }: args::Reopen) -> GResult<()> {
    set(session, province, false)
}

crate::catalogue::bind! {
    eu4_harmony_remote::forts: FortRequest => FortAnswer;
    List => list;
    Mothball => mothball;
    Reopen => reopen;
}

#[cfg(test)]
mod tests {
    use crate::game::As;
    use super::*;
    use crate::catalogue::fake::{byz, tur};
    use crate::game::{
        buffers::{self, write_reason, Image, Object, Provinces},
        commands::record,
        Caller, Fail, Status,
    };

    #[test]
    fn the_command_names_the_province_and_the_country() {
        assert_eq!(super::toggle_text(*b"FRA", 183), r#"mothball_fort={ province=183 country="FRA" }"#);
    }

    /// The fort maintenance box's check, which lets the player toggle any
    /// fort but province 4's.
    extern "C" fn may(_: usize, province: usize, text: usize) -> usize {
        if province == 4 {
            unsafe { write_reason(text, b"- at war") };
            return 0;
        }
        1
    }

    /// The Ottomans, owning provinces 1 (an open fort of level 2), 2 (none),
    /// 3 (a mothballed fort) and 4 (an open fort the game will not let them
    /// toggle); Byzantium owning province 5, with a fort.
    struct Ottomans {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn ottomans() -> Ottomans {
        let image = Image::new();
        image.strings().function(MAY_MOTHBALL.rva(), may as *const ());
        let provinces = Provinces::new(&image, 6);
        for id in 1..=4 {
            provinces.owner(id, tur());
        }
        provinces.owner(5, byz()).u8(5, HAS_FORT, 1);
        provinces.u8(1, HAS_FORT, 1).i32(1, FORT_LEVEL, 2);
        provinces.u8(3, HAS_FORT, 1).u8(3, MOTHBALL, MOTHBALLED);
        provinces.u8(4, HAS_FORT, 1);
        let owned = Object::new(16).u32(0, 1).u32(4, 2).u32(8, 3).u32(12, 4);
        let player = Object::country(tur(), country::OWNED_COUNT + 4).points_to(country::OWNED, owned).i32(country::OWNED_COUNT, 4);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), provinces.world(), tur(), player.at(), countries.at(), Caller::Remote(As::Player));
        Ottomans { _image: image, _provinces: provinces, _objects: vec![player, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_list_holds_the_player_s_forts_with_what_the_box_lets_the_player_do() {
        let ottomans = ottomans();
        let forts = list(ottomans.session).unwrap();
        let summary: Vec<_> = forts.iter().map(|f| (f.province.0, f.level, f.mothballed, f.choice.can)).collect();
        assert_eq!(summary, [(1, 2, false, true), (3, 0, true, true), (4, 0, false, false)]);
        assert_eq!(forts[2].choice.reasons, ["at war"]);
    }

    #[test]
    fn a_fort_is_mothballed_and_reopened_by_the_box_s_command() {
        let ottomans = ottomans();
        mothball(ottomans.session, args::Mothball { province: ProvinceId(1) }).unwrap();
        reopen(ottomans.session, args::Reopen { province: ProvinceId(3) }).unwrap();
        assert_eq!(record::posted(), [r#"mothball_fort={ province=1 country="TUR" }"#, r#"mothball_fort={ province=3 country="TUR" }"#]);
    }

    #[test]
    fn a_fort_the_player_lacks_or_has_in_that_state_already_is_refused() {
        let ottomans = ottomans();
        let session = ottomans.session;
        assert_eq!(refused(mothball(session, args::Mothball { province: ProvinceId(2) })), (Status::Rejected, "province 2 has no fort of the player's".into()));
        assert_eq!(refused(mothball(session, args::Mothball { province: ProvinceId(5) })), (Status::Rejected, "province 5 has no fort of the player's".into()));
        assert_eq!(refused(reopen(session, args::Reopen { province: ProvinceId(1) })), (Status::Rejected, "the fort in province 1 is open already".into()));
        assert_eq!(
            refused(mothball(session, args::Mothball { province: ProvinceId(3) })),
            (Status::Rejected, "the fort in province 3 is mothballed already".into())
        );
        assert_eq!(refused(mothball(session, args::Mothball { province: ProvinceId(4) })), (Status::Rejected, "at war".into()));
        assert_eq!(refused(mothball(session, args::Mothball { province: ProvinceId(9) })).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }
}
