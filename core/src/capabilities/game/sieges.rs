//! Sieges: the siege view's buttons, the artillery barrage, the naval
//! barrage and the assault, each the game's spending of military points,
//! posted as the view posts it, with the game's own answer of whether the
//! player can and why not (docs/game/military/sieges.md).
use super::units::checks;
use super::{power, Ctx, GResult, Session, View};
use crate::{native::game_string, native::memory::Memory, native::points::{GameFunction, Point}};
use eu4_harmony_remote::sieges::args;
use eu4_harmony_remote::ProvinceId;
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    sieges::SiegeAction,
};

/// A province's siege, null when there is none.
const SIEGE: usize = 0x388;
/// `(country, siege, text or null)`: whether the country may order each,
/// appending the tooltip's reasons, or its explanation when it may.
const MAY_BOMBARD: GameFunction<3> = GameFunction::at(0x4032f0);
const MAY_NAVAL_BOMBARD: GameFunction<3> = GameFunction::at(0x4038c0);
const MAY_ASSAULT: GameFunction<3> = GameFunction::at(0x381070);
/// The siege view's buttons spend military points, with the province as
/// the spending's first value.
const POWER: PowerKind = PowerKind::Mil;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x5db7ecusize, "4c 8b 82 88 03 00 00", "province siege").uses(&[SIEGE]),
        Point::keep(MAY_BOMBARD, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57", "may order an artillery barrage"),
        Point::keep(MAY_NAVAL_BOMBARD, "48 89 5c 24 08 4c 89 44 24 18 48 89 54 24 10 55 56 57 41 54 41 55 41 56 41 57", "may order a naval barrage"),
        Point::keep(MAY_ASSAULT, "48 89 5c 24 08 55 56 57 41 56 41 57 48 8d 6c 24 80 48 81 ec 80 01 00 00", "may order an assault"),
        // The siege view's buttons make the spending: the assault's category
        // and power.
        Point::keep(0x13afae0usize, "c7 40 68 25 00 00 00 c7 40 6c 02 00 00 00", "artillery barrage button's spending"),
        // The assault button's command: the spending at its +0x58, so its
        // fields at +0x60 on.
        Point::keep(0x13af98eusize, "48 89 48 58 48 8b cf c7 40 60 65 01 00 00 c7 40 68 09 00 00 00 c7 40 6c 02 00 00 00", "a spending's type and category"),
    ]
}

/// The siege view's three buttons: the capability, the spending's category,
/// and the game's answer.
const ACTIONS: [(&str, i32, GameFunction<3>); 3] = [("bombard", 37, MAY_BOMBARD), ("naval_bombard", 40, MAY_NAVAL_BOMBARD), ("assault", 9, MAY_ASSAULT)];

/// The siege in a province, or the refusal; in the player's view an
/// undiscovered province is refused as terra incognita, stale visibility is
/// refused when `check_visibility` holds, and none shows under fog of war.
fn siege(session: Session, id: u32, check_visibility: bool) -> GResult<(usize, usize)> {
    let address = session.readable_province(id)?;
    if check_visibility {
        session.require_current_visibility()?;
    }
    if session.province_hidden(address)? {
        return Err(checks::no_siege(id));
    }
    match session.m.ptr(address + SIEGE)? {
        0 => Err(checks::no_siege(id)),
        siege => Ok((address, siege)),
    }
}

/// What a button's spending costs.
fn cost(m: &Memory, ctx: &Ctx, category: i32, province: u32) -> GResult<Amount> {
    let points = power::Spending::new(m, category, POWER, province as i32)?.cost(m, ctx.player)?;
    Ok(Amount::new(Resource::Mil, points as f64))
}

fn action(m: &Memory, ctx: &Ctx, siege: usize, province: u32, (name, category, may): (&str, i32, GameFunction<3>)) -> GResult<SiegeAction> {
    let cost = vec![cost(m, ctx, category, province)?];
    let (can, text) = game_string::written(m, |t| may.call(m, [ctx.country, siege, t]))?;
    let choice = if can as u8 != 0 { Choice::can(cost) } else { Choice::cannot(super::reasons(&text), cost) };
    Ok(SiegeAction { action: name.into(), choice })
}

/// What the siege view offers for the siege in a province.
pub fn options(session: Session, args::Options { province: ProvinceId(province) }: args::Options) -> GResult<Vec<SiegeAction>> {
    let Session { m, ctx, .. } = session;
    let (address, siege) = siege(session, province, true)?;
    let mut out = ACTIONS.into_iter().map(|a| action(m, &ctx, siege, province, a)).collect::<GResult<Vec<_>>>()?;
    out.push(SiegeAction { action: "sorties".into(), choice: super::spendings::sorties(m, &ctx, address, province)? });
    Ok(out)
}

/// Presses one of the siege view's buttons (player orders never reach a siege
/// hidden by fog of war).
fn press(session: Session, name: &str, province: u32) -> GResult<()> {
    let Session { m, ctx, commands, .. } = session;
    let (_, siege) = siege(session.with_view(View::Player), province, false)?;
    let Some(entry) = ACTIONS.into_iter().find(|a| a.0 == name) else {
        return Err(format!("no siege action {name}").into());
    };
    let a = action(m, &ctx, siege, province, entry)?;
    super::action::carry_out(commands, &a.choice, &format!("order {name} here"), || Ok(power::written(ctx.player.letters(), entry.1, POWER, province as i32)))
}

pub fn bombard(session: Session, args::Bombard { province: ProvinceId(province) }: args::Bombard) -> GResult<()> {
    press(session, "bombard", province)
}

pub fn naval_bombard(session: Session, args::NavalBombard { province: ProvinceId(province) }: args::NavalBombard) -> GResult<()> {
    press(session, "naval_bombard", province)
}

pub fn assault(session: Session, args::Assault { province: ProvinceId(province) }: args::Assault) -> GResult<()> {
    press(session, "assault", province)
}

/// Sends the garrison of the player's besieged fort out, as the siege
/// view's sorties button does once the player confirms.
pub fn sorties(session: Session, args::Sorties { province: ProvinceId(province) }: args::Sorties) -> GResult<()> {
    let Session { m, ctx, commands, .. } = session;
    let (address, _) = siege(session.with_view(View::Player), province, false)?;
    let choice = super::spendings::sorties(m, &ctx, address, province)?;
    super::action::carry_out(commands, &choice, "sortie from this fort", || Ok(super::spendings::sorties_written(ctx.player, province)))
}

crate::capabilities::catalogue::bind! {
    eu4_harmony_remote::sieges: SiegeRequest => SiegeAnswer;
    Options => options;
    Bombard => bombard;
    NavalBombard => naval_bombard;
    Assault => assault;
    Sorties => sorties;
}

#[cfg(test)]
mod tests {
    use crate::capabilities::game::As;
    #[test]
    fn the_spending_names_the_province_and_the_category() {
        assert_eq!(super::power::written(*b"FRA", 37, super::POWER, 183), r#"spend_power_command={ country="FRA" power={ data=183 category=37 skill=2 } }"#);
    }

    use super::*;
    use crate::capabilities::game::{
        buffers::{self, no, write_inline, yes, Image, Object, Provinces},
        commands::record,
        power::{COST, NO_ID},
        Caller, Fail, Session, Status, Tag,
    };
    use crate::native::objects::campaign::province;

    /// The game's cost of a spending: 50 points.
    extern "C" fn fifty() -> usize {
        50
    }

    /// The game's answer of whether the player may: no, the siege too young.
    extern "C" fn too_early(_: usize, _: usize, text: usize) -> usize {
        write_inline(text, "too early");
        0
    }

    /// France besieging province 2 of provinces 0 to 3, province 3 hidden
    /// by fog of war with a siege of its own; the game lets the player order
    /// what `may` answers.
    struct Siege {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn siege(may: *const ()) -> Siege {
        let image = Image::new();
        for (_, _, f) in ACTIONS {
            image.function(f.rva(), may);
        }
        image.function(crate::capabilities::game::spendings::MAY_SORTIE.rva(), may);
        image.function(COST.rva(), fifty as *const ()).function(game_string::DESTROY_AT.rva(), no as *const ()).u64(NO_ID, 0);
        let provinces = Provinces::new(&image, 4);
        let (sieges, fra) = (Object::new(8), Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0")));
        provinces.u64(2, SIEGE, sieges.at() as u64).u64(3, SIEGE, sieges.at() as u64);
        provinces.u8(3, province::FOGGED, province::FOGGED_BIT).u8(3, province::HIDDEN, province::HIDDEN_BIT);
        let france = Object::country(fra, 0);
        let countries = Object::countries(&[&france]);
        let session = buffers::session(image.memory(), provinces.world(), fra, france.at(), countries.at(), Caller::Remote(As::Player));
        Siege { _image: image, _provinces: provinces, _objects: vec![sieges, france, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the siege action was ordered") };
        (status, message)
    }

    #[test]
    fn each_button_spends_its_category_of_military_points_on_the_province() {
        let siege = siege(yes as *const ());
        bombard(siege.session, args::Bombard { province: ProvinceId(2) }).unwrap();
        naval_bombard(siege.session, args::NavalBombard { province: ProvinceId(2) }).unwrap();
        assault(siege.session, args::Assault { province: ProvinceId(2) }).unwrap();
        sorties(siege.session, args::Sorties { province: ProvinceId(2) }).unwrap();
        let spent = |category| format!(r#"spend_power_command={{ country="FRA" power={{ data=2 category={category} skill=2 }} }}"#);
        assert_eq!(record::posted(), [spent(37), spent(40), spent(9), spent(30)]);
    }

    #[test]
    fn a_siege_the_game_refuses_or_the_player_does_not_see_is_refused() {
        let siege = siege(too_early as *const ());
        assert_eq!(refused(assault(siege.session, args::Assault { province: ProvinceId(2) })), (Status::Rejected, "too early".into()));
        let no_siege = |id| (Status::Rejected, format!("there is no siege in province {id}"));
        assert_eq!(refused(bombard(siege.session, args::Bombard { province: ProvinceId(1) })), no_siege(1));
        // What fog of war hides, the player's orders do not reach.
        assert_eq!(refused(bombard(siege.session, args::Bombard { province: ProvinceId(3) })), no_siege(3));
        assert_eq!(refused(bombard(siege.session, args::Bombard { province: ProvinceId(4) })).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }

    #[test]
    fn the_options_are_the_four_buttons_with_their_costs_and_the_game_s_answers() {
        let siege = siege(too_early as *const ());
        let options = options(siege.session, args::Options { province: ProvinceId(2) }).unwrap();
        assert_eq!(options.iter().map(|a| a.action.as_str()).collect::<Vec<_>>(), ["bombard", "naval_bombard", "assault", "sorties"]);
        assert!(options.iter().all(|a| !a.choice.can && a.choice.reasons == ["too early"] && a.choice.cost == [Amount::new(Resource::Mil, 50.0)]));
    }

    #[test]
    fn options_checks_undiscovered_and_visibility_freshness_in_player_view() {
        let s = siege(too_early as *const ());
        // Full view reads fogged siege in province 3 without visibility proof.
        assert!(options(s.session, args::Options { province: ProvinceId(3) }).is_ok());
        // Player's view without current visibility proof refuses with NotReady.
        let player_view = s.session.with_view(View::Player);
        assert_eq!(options(player_view, args::Options { province: ProvinceId(2) }).unwrap_err().status, Status::NotReady);
        // Undiscovered province is refused as terra incognita before visibility proof.
        let discovered = Object::new(16);
        s._provinces.u8(2, province::FOGGED, province::UNDISCOVERED_BIT).u64(2, province::DISCOVERED_BY, discovered.at() as u64);
        let err = options(player_view, args::Options { province: ProvinceId(2) }).unwrap_err();
        assert_eq!(err.status, Status::NotFound);
        assert!(err.message.contains("discovered"), "{}", err.message);
        drop(discovered);
    }
}
