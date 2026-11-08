//! Recruiting: one regiment or ship of a kind in a province, ordered for the
//! player through the game's own recruit command, as the recruit button in
//! the province view orders it, so that the game's checks and costs apply
//! (docs/research/recruiting.md). The command goes in its written form
//! through the session's commands ([`super::action`]).
use super::{
    action,
    provinces::{can_recruit, unit_type, UnitKind},
    with_state, Fail, GResult, Session, Status,
};
use crate::{game_string, objects::campaign::unit_type as unit_type_fields};
use eu4_harmony_remote::recruiting::args;
use eu4_harmony_remote::ProvinceId;

/// The recruit command in its written form: the country by its tag, the
/// province by its ID, the unit type by its key (`common/units`), as the
/// recruit button makes it, a regular unit with no mercenary company; the
/// keys left out take the game's defaults.
fn written(country: [u8; 3], province: u32, unit: &str) -> String {
    format!("buildunit={{ country=\"{}\" province={province} unit=\"{unit}\" }}", String::from_utf8_lossy(&country))
}

/// Recruits one regiment or ship of `kind` in `province` for the player, the
/// way the recruit button in the province view does. Refused
/// ([`Status::Rejected`]) when the game would not let the player recruit it
/// there now, with the game's reasons, one a line; [`Status::NotFound`] for a
/// number that names no province. Done once the game has carried the order
/// out, paying for it and putting it in the province's recruit queue: the
/// command layer has the game execute it at once (ADR commands-run-at-once), so
/// orders given one after another are each checked against what the ones before
/// left.
pub fn recruit(Session { m, ctx, commands, .. }: Session, args::Order { province: ProvinceId(id), kind }: args::Order) -> GResult<()> {
    let kind = UnitKind::from(kind);
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
    let Some(unit_type) = unit_type(m, ctx.country, kind)? else {
        return Err(no_unit_type(kind.key()));
    };
    let (allowed, reasons) = can_recruit(m, &ctx, address, unit_type)?;
    if !allowed {
        return Err(action::not_allowed("recruit it there", &reasons));
    }
    let unit = game_string::key(m, unit_type + unit_type_fields::NAME)?;
    // The command's own check adds what the snapshot leaves out: its rule
    // for ships.
    action::post(commands, &written(ctx.player.letters(), id, &unit), "recruit it there")
}

/// The refusal of a kind, by its key (`infantry`, `heavy_ship`), the player
/// has no unit type of.
fn no_unit_type(kind: &str) -> Fail {
    Fail::new(Status::Rejected, format!("the player has no unit type of the kind {kind}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_country_the_province_and_the_unit_type() {
        assert_eq!(super::written(*b"FRA", 183, "western_medieval_infantry"), r#"buildunit={ country="FRA" province=183 unit="western_medieval_infantry" }"#);
    }

    use super::*;
    use crate::game::{
        buffers::{self, answers_no, answers_yes, write_inline, Image, Object, Provinces},
        commands::record,
        provinces::{CAN_RECRUIT, RECRUIT_PLACE},
        Caller, Tag,
    };
    use crate::objects::campaign::{classes, country};
    use eu4_harmony_remote::recruiting::UnitKind as Kind;

    /// The game's recruit check: no, for want of money.
    extern "C" fn no_money(_: usize, _: usize, _: usize, _: usize, _: usize, _: usize, reasons: usize, _: usize, _: usize, _: usize) -> usize {
        write_inline(reasons, "  no money\n");
        0
    }

    /// France, with an infantry unit type and none of the other kinds, in a
    /// world of provinces 0 to 2; the game's recruit check answers with
    /// `check`.
    struct France {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn france(check: *const ()) -> France {
        let image = Image::new();
        let m = image.memory();
        image
            .function(CAN_RECRUIT.rva(), check)
            .function(RECRUIT_PLACE.rva(), answers_yes as *const ())
            .function(game_string::DESTROY_AT.rva(), answers_no as *const ());
        let provinces = Provinces::new(&image, 3);
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        let infantry =
            Object::new(unit_type_fields::NAME + game_string::OBJECT).u64(0, m.at(classes::UNIT_TYPE) as u64).string(unit_type_fields::NAME, "w_infantry");
        let france = Object::country(fra, country::UNIT_TYPES + 8 * country::UNIT_KINDS).points_to(country::UNIT_TYPES, infantry);
        let countries = Object::countries(&[&france]);
        let session = buffers::session(m, provinces.world(), fra, france.at(), countries.at(), Caller::Remote);
        France { _image: image, _provinces: provinces, _objects: vec![france, countries], session }
    }

    fn order(province: u32, kind: Kind) -> args::Order {
        args::Order { province: ProvinceId(province), kind }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the regiment was recruited") };
        (status, message)
    }

    #[test]
    fn a_regiment_is_recruited_through_the_recruit_command() {
        let france = france(answers_yes as *const ());
        recruit(france.session, order(2, Kind::Infantry)).unwrap();
        assert_eq!(record::posted(), [r#"buildunit={ country="FRA" province=2 unit="w_infantry" }"#]);
    }

    #[test]
    fn a_recruit_the_game_or_the_player_cannot_make_is_refused() {
        let france = france(no_money as *const ());
        assert_eq!(refused(recruit(france.session, order(1, Kind::Infantry))), (Status::Rejected, "no money".into()));
        assert_eq!(refused(recruit(france.session, order(1, Kind::Galley))), (Status::Rejected, "the player has no unit type of the kind galley".into()));
        assert_eq!(refused(recruit(france.session, order(0, Kind::Infantry))).0, Status::NotFound);
        assert_eq!(refused(recruit(france.session, order(3, Kind::Infantry))).0, Status::NotFound);
        assert!(record::posted().is_empty());
    }
}
