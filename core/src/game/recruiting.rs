//! Recruiting: one regiment or ship of a kind in a province, ordered for the
//! player through the game's own recruit command, as the recruit button in
//! the province view orders it, so that the game's checks and costs apply
//! (docs/research/recruiting.md). The command goes in its written form
//! through the command layer ([`super::commands`]).
use super::{
    commands::{self, Refused},
    context, fail, game,
    provinces::{can_recruit, unit_type, UnitKind},
    with_state, Caller, GResult, Status,
};
use crate::{game_string, objects::campaign::unit_type as unit_type_fields, unicode::game_text};

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
pub fn recruit(_: Caller, id: u32, kind: UnitKind) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    if id == 0 {
        return Err(eu4_harmony_checks::provinces::not_found(0).into());
    }
    let address = with_state(|s| s.geography.province_address(m, &ctx, id))?;
    let Some(unit_type) = unit_type(m, ctx.country, kind)? else {
        return Err(eu4_harmony_checks::recruiting::no_unit_type(kind.key()).into());
    };
    let (allowed, reasons) = can_recruit(m, &ctx, address, unit_type)?;
    if !allowed {
        return Err(eu4_harmony_checks::recruiting::not_allowed(&reasons).into());
    }
    let unit = game_text::to_utf8(&game_string::read(m, unit_type + unit_type_fields::NAME)?);
    match commands::post(&written(ctx.player.letters(), id, &unit)) {
        Ok(()) => Ok(()),
        // What the snapshot leaves out: the command's own rule for ships.
        Err(Refused::Check) => fail(Status::Rejected, "the game's recruit command refuses it, and gives no reason"),
        Err(Refused::NotTaken) => fail(Status::Rejected, "the game takes no orders from the player now"),
        Err(Refused::Unread(how)) => fail(Status::Internal, format!("the game read the recruit command otherwise: {how}")),
        Err(Refused::Game(fail)) => Err(fail),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_country_the_province_and_the_unit_type() {
        assert_eq!(super::written(*b"FRA", 183, "western_medieval_infantry"), r#"buildunit={ country="FRA" province=183 unit="western_medieval_infantry" }"#);
    }
}
