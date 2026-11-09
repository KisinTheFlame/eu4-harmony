//! Occupation: handing a province the player occupies over to an ally, as
//! the province view's transfer occupation button and window do
//! (docs/research/peace.md, "Handing over occupied provinces").
//!
//! The button asks the game's check for the player and the province, which
//! says why not, or lists the allies that may take it; the window lists
//! those, and picking one posts the transfer command for it, which asks the
//! same check before the game carries it out.
use super::peace::checks;
use super::{action, countries, diplomacy, with_state, GResult, Session, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::province,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::{
    choices::Choice,
    occupation::{args, Transferred, Transfers},
    CountryTag, ProvinceId,
};

/// The check: (country, province, allies, reasons), the allies a list the
/// check fills ([`receivers`]), `reasons` a `std::string` or null, which
/// gets the button's tooltip when the check says no.
const CHECK: GameFunction<4> = GameFunction::at(0x3dbce0);

/// The list of allies the check fills, a [`crate::objects::list`]: the
/// first node, and in each node the ally's tag and the next node.
mod receivers {
    pub use crate::objects::list::{HEAD as FIRST, ITEM as TAG, NEXT, NODE_SIZE, SIZE};
}

/// The most allies a list is believed to hold.
const MOST: usize = 1_000;

/// Whether the player can hand `province` over, and to which allies, as the
/// transfer occupation button and window tell the player.
pub fn options(Session { m, ctx, .. }: Session, args::Options { province: ProvinceId(province) }: args::Options) -> GResult<Transfers> {
    let at = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let (allies, reasons) = check(m, ctx.country, at)?;
    Ok(match allies {
        Some(allies) => Transfers { choice: Choice::can(Vec::new()), allies: allies.into_iter().map(|a| CountryTag(a.letters())).collect() },
        None => Transfers { choice: Choice::cannot(reasons, Vec::new()), allies: Vec::new() },
    })
}

/// Hands `province` over to the ally `country`, as picking it in the window
/// does; the country that occupies it after.
pub fn transfer(Session { m, ctx, commands, .. }: Session, args::Transfer { province: ProvinceId(province), country }: args::Transfer) -> GResult<Transferred> {
    let ally = countries::named(m, &ctx, country)?;
    diplomacy::other(m, &ctx, ally)?;
    let at = with_state(|s| s.geography.province_address(m, &ctx, province))?;
    let (allies, reasons) = check(m, ctx.country, at)?;
    let Some(allies) = allies else {
        return Err(action::not_allowed("hand it over", &reasons));
    };
    if !allies.iter().any(|a| a.index() == ally.index()) {
        return Err(checks::not_a_receiver(diplomacy::letters(ally), province));
    }
    action::post(commands, &command(ally, province, ctx.player), "hand it over")?;
    let controller = Tag(m.u64(at + province::CONTROLLER)?);
    Ok(Transferred { controller: (controller.valid() && controller.normal()).then(|| CountryTag(controller.letters())) })
}

/// The transfer command's written form: the ally to take the province, and
/// the player handing it over.
fn command(ally: Tag, province: u32, player: Tag) -> String {
    format!("transfer_controller_command={{ country=\"{}\" province={province} commander=\"{}\" }}", diplomacy::letters(ally), diplomacy::letters(player))
}

/// What the check says of the player's country at `country` and the
/// province at `province`: the allies that may take it, or None and why
/// not, the button's tooltip's lines.
fn check(m: &Memory, country: usize, province: usize) -> GResult<(Option<Vec<super::Tag>>, Vec<String>)> {
    let mut list = [0u64; receivers::SIZE / 8];
    let list_at = list.as_mut_ptr() as usize;
    let checked = game_string::written(m, |s| CHECK.call(m, [country, province, list_at, s]));
    let first = unsafe { std::ptr::read_volatile(&list) }[receivers::FIRST / 8] as usize;
    let allies = nodes(m, first);
    let (can, text) = checked?;
    let allies = allies?;
    if can as u8 == 0 {
        return Ok((None, diplomacy::text_lines(&text)));
    }
    Ok((Some(allies), Vec::new()))
}

/// The allies in the list from `first`, its nodes freed as the game frees
/// them.
fn nodes(m: &Memory, mut node: usize) -> GResult<Vec<Tag>> {
    let mut allies = Vec::new();
    while node != 0 {
        if allies.len() > MOST {
            return Err("the allies to take a province are out of bounds".to_string().into());
        }
        let next = m.ptr(node + receivers::NEXT)?;
        allies.push(Tag(m.u64(node + receivers::TAG)?));
        crate::game_heap::delete(m, node, receivers::NODE_SIZE)?;
        node = next;
    }
    Ok(allies)
}

/// The check, and the command's: its check asks the check for the
/// commander and finds the country among the allies, freeing the nodes.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(CHECK, "48 89 5c 24 18 4c 89 4c 24 20 48 89 54 24 10 55 56 57 41 54 41 55 41 56", "transfer occupation check"),
        Point::keep(0x2a59c8usize, "4c 8b 43 50 49 c1 e8 20 48 8b 10 48 c1 ea 20 66 41 3b d0 74 0c 48 8b 50 10", "transfer occupation allies")
            .uses(&[receivers::TAG, receivers::NEXT]),
        Point::keep(0x2a5a00usize, "48 8b 59 10 ba 20 00 00 00 e8 e2 3a 49 01", "transfer occupation allies freed")
            .uses(&[receivers::NEXT, receivers::NODE_SIZE]),
    ]
}

crate::catalogue::bind! {
    eu4_harmony_remote::occupation: OccupationRequest => OccupationAnswer;
    Options => options;
    Transfer => transfer;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::game_tag;

    #[test]
    fn the_command_names_the_ally_to_take_it_and_the_player() {
        assert_eq!(command(game_tag(b"DUL", 2), 151, game_tag(b"TUR", 1)), "transfer_controller_command={ country=\"DUL\" province=151 commander=\"TUR\" }");
    }

    use crate::game::{
        buffers::{self, no, write_inline, Image, Object, Provinces},
        commands::record,
        Caller, Fail, Status,
    };
    use crate::game_heap::DELETE;

    /// A province of the test's the check says no for, in a byte the
    /// province does not use.
    const BARRED: usize = 8;

    fn dul() -> Tag {
        game_tag(b"DUL", 2)
    }

    /// The check: no for a barred province, else Dulkadir to take it, in a
    /// node the game would make and free.
    extern "C" fn check(_: usize, province: usize, list: usize, reasons: usize) -> usize {
        if unsafe { *((province + BARRED) as *const u8) } != 0 {
            write_inline(reasons, "- not at war");
            return 0;
        }
        let node = Box::into_raw(Box::new([0u64; receivers::NODE_SIZE / 8])) as usize;
        unsafe {
            *((node + receivers::TAG) as *mut u64) = dul().0;
            *((list + receivers::FIRST) as *mut usize) = node;
        }
        1
    }

    /// The game's heap freeing a node of the check's.
    extern "C" fn free(node: usize) -> usize {
        drop(unsafe { Box::from_raw(node as *mut [u64; receivers::NODE_SIZE / 8]) });
        0
    }

    /// The Ottomans occupying provinces 1 and 2, the second barred, with
    /// Dulkadir and Poland in the world.
    struct Ottomans {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn ottomans() -> Ottomans {
        use crate::catalogue::fake::tur;
        let image = Image::new();
        image.function(CHECK.rva(), check as *const ()).function(DELETE.rva(), free as *const ()).function(game_string::DESTROY_AT.rva(), no as *const ());
        let provinces = Provinces::new(&image, 3);
        provinces.u64(1, province::CONTROLLER, tur().0).u64(2, province::CONTROLLER, tur().0).u8(2, BARRED, 1);
        let (player, ally, other) = (Object::country(tur(), 0), Object::country(dul(), 0), Object::country(game_tag(b"POL", 3), 0));
        let countries = Object::countries(&[&player, &ally, &other]);
        let manager = image.country_manager(&countries);
        let session = buffers::session(image.memory(), provinces.world(), tur(), player.at(), countries.at(), Caller::Remote);
        Ottomans { _image: image, _provinces: provinces, _objects: vec![player, ally, other, countries, manager], session }
    }

    /// Hands `province` over to the country of `tag`.
    fn transfer(session: Session, province: u32, tag: &[u8; 3]) -> GResult<Transferred> {
        super::transfer(session, args::Transfer { province: ProvinceId(province), country: CountryTag(*tag) })
    }

    fn options(session: Session, province: u32) -> GResult<Transfers> {
        super::options(session, args::Options { province: ProvinceId(province) })
    }

    fn refused(result: GResult<Transferred>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the province was handed over") };
        (status, message)
    }

    #[test]
    fn a_province_is_handed_to_an_ally_the_window_lists() {
        let ottomans = ottomans();
        assert_eq!(transfer(ottomans.session, 1, b"DUL").unwrap().controller, Some(CountryTag(*b"TUR")));
        assert_eq!(record::posted(), [r#"transfer_controller_command={ country="DUL" province=1 commander="TUR" }"#]);
        assert_eq!(options(ottomans.session, 1).unwrap().allies, [CountryTag(*b"DUL")]);
    }

    #[test]
    fn a_province_the_check_bars_or_a_country_the_window_does_not_list_is_refused() {
        let ottomans = ottomans();
        assert_eq!(refused(transfer(ottomans.session, 2, b"DUL")), (Status::Rejected, "not at war".into()));
        assert_eq!(
            refused(transfer(ottomans.session, 1, b"POL")),
            (Status::Rejected, "the transfer occupation window does not list POL to take province 1".into())
        );
        assert_eq!(refused(transfer(ottomans.session, 1, b"TUR")), (Status::InvalidArgument, "the player's own country".into()));
        assert_eq!(refused(transfer(ottomans.session, 3, b"DUL")).0, Status::NotFound);
        assert_eq!(refused(transfer(ottomans.session, 1, b"QQQ")).0, Status::NotFound);

        assert!(record::posted().is_empty());
    }
}
