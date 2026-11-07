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
pub fn recruit(Session { m, ctx, commands, .. }: Session, id: u32, kind: UnitKind) -> GResult<()> {
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
    let unit = game_text::to_utf8(&game_string::read(m, unit_type + unit_type_fields::NAME)?);
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
}
