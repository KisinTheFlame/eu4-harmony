//! Sieges: the siege view's buttons, the artillery barrage, the naval
//! barrage and the assault, each the game's spending of military points,
//! posted as the view posts it, with the game's own answer of whether the
//! player can and why not (docs/research/sieges.md).
use super::{commands, context, game, power, with_state, Caller, Ctx, GResult};
use crate::{game_string, memory::Memory, points::Point};
use eu4_harmony_checks::military as checks;
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    sieges::SiegeAction,
};

/// A province's siege, null when there is none.
const SIEGE: usize = 0x388;
/// `(country, siege, text or null)`: whether the country may order each,
/// appending the tooltip's reasons, or its explanation when it may.
const MAY_BOMBARD: usize = 0x4032f0;
const MAY_NAVAL_BOMBARD: usize = 0x4038c0;
const MAY_ASSAULT: usize = 0x381070;
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
const ACTIONS: [(&str, i32, usize); 3] = [("bombard", 37, MAY_BOMBARD), ("naval_bombard", 40, MAY_NAVAL_BOMBARD), ("assault", 9, MAY_ASSAULT)];

/// The siege in a province, or the refusal.
fn siege(m: &Memory, ctx: &Ctx, id: u32) -> GResult<usize> {
    let address = with_state(|s| s.geography.province_address(m, ctx, id))?;
    match m.ptr(address + SIEGE)? {
        0 => Err(checks::no_siege(id).into()),
        siege => Ok(siege),
    }
}

/// What a button's spending costs.
fn cost(m: &Memory, ctx: &Ctx, category: i32, province: u32) -> GResult<Amount> {
    let points = power::Spending::new(m, category, POWER, province as i32)?.cost(m, ctx.player)?;
    Ok(Amount::new(Resource::Mil, points as f64))
}

fn action(m: &Memory, ctx: &Ctx, siege: usize, province: u32, (name, category, may): (&str, i32, usize)) -> GResult<SiegeAction> {
    let cost = vec![cost(m, ctx, category, province)?];
    let (can, text) = game_string::written(m, |t| m.call(m.at(may), &[ctx.country, siege, t]))?;
    let choice = if can as u8 != 0 { Choice::can(cost) } else { Choice::cannot(super::reasons(&text), cost) };
    Ok(SiegeAction { action: name.into(), choice })
}

/// What the siege view offers for the siege in a province.
pub fn options(_: Caller, province: u32) -> GResult<Vec<SiegeAction>> {
    let m = game()?;
    let ctx = context(m)?;
    let siege = siege(m, &ctx, province)?;
    ACTIONS.into_iter().map(|a| action(m, &ctx, siege, province, a)).collect()
}

/// Presses one of the siege view's buttons.
fn press(name: &str, province: u32) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let siege = siege(m, &ctx, province)?;
    let Some(entry) = ACTIONS.into_iter().find(|a| a.0 == name) else {
        return Err(format!("no siege action {name}").into());
    };
    let a = action(m, &ctx, siege, province, entry)?;
    if !a.choice.can {
        return Err(checks::not_allowed(&format!("order {name} here"), &a.choice.reasons).into());
    }
    commands::post(&power::written(ctx.player.letters(), entry.1, POWER, province as i32))?;
    Ok(())
}

pub fn bombard(_: Caller, province: u32) -> GResult<()> {
    press("bombard", province)
}

pub fn naval_bombard(_: Caller, province: u32) -> GResult<()> {
    press("naval_bombard", province)
}

pub fn assault(_: Caller, province: u32) -> GResult<()> {
    press("assault", province)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_spending_names_the_province_and_the_category() {
        assert_eq!(super::power::written(*b"FRA", 37, super::POWER, 183), r#"spend_power_command={ country="FRA" power={ data=183 category=37 skill=2 } }"#);
    }
}
