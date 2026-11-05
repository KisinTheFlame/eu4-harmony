//! Sieges: the siege view's buttons, the artillery barrage, the naval
//! barrage and the assault, each the game's spending of military points,
//! posted as the view posts it, with the game's own answer of whether the
//! player can and why not (docs/research/sieges.md).
use super::{commands, context, game, with_state, Caller, Ctx, GResult};
use crate::{game_string, memory::Memory, points::Point};
use eu4_harmony_checks::military as checks;
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    sieges::SiegeAction,
};

/// A province's siege, null when there is none.
const SIEGE: usize = 0x388;
/// `(country, siege, text or null)`: whether the country may order each,
/// appending the tooltip's reasons, or its explanation when it may.
const MAY_BOMBARD: usize = 0x4032f0;
const MAY_NAVAL_BOMBARD: usize = 0x4038c0;
const MAY_ASSAULT: usize = 0x381070;
/// `(spending, tag by value, text or null, 0)`: what a spending of monarch
/// points costs, in whole points, as its check and its payment count it.
const SPENDING_COST: usize = 0x9526d0;
/// A spending, as the siege view's buttons make it inside their command:
/// its vtable, its type, category, power (2 military), what it is for (the
/// province), and a second value, -1 for none; the rest zero.
const SPENDING_VTABLE: usize = 0x1c55480;
mod spending {
    pub const TYPE: usize = 0x08;
    pub const TYPE_VALUE: i32 = 0x165;
    pub const CATEGORY: usize = 0x10;
    pub const SKILL: usize = 0x14;
    pub const DATA: usize = 0x18;
    pub const SECOND: usize = 0x1c;
    pub const SIZE: usize = 0x40;
    pub const MILITARY: i32 = 2;
}

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x5db7ecusize, "4c 8b 82 88 03 00 00", "province siege").uses(&[SIEGE]),
        Point::keep(MAY_BOMBARD, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57", "may order an artillery barrage"),
        Point::keep(MAY_NAVAL_BOMBARD, "48 89 5c 24 08 4c 89 44 24 18 48 89 54 24 10 55 56 57 41 54 41 55 41 56 41 57", "may order a naval barrage"),
        Point::keep(MAY_ASSAULT, "48 89 5c 24 08 55 56 57 41 56 41 57 48 8d 6c 24 80 48 81 ec 80 01 00 00", "may order an assault"),
        Point::keep(SPENDING_COST, "48 8b c4 48 89 58 10 4c 89 48 20 4c 89 40 18 48 89 48 08", "monarch point spending cost"),
        // The siege view's buttons make the spending: the assault's category
        // and power.
        Point::keep(0x13afae0usize, "c7 40 68 25 00 00 00 c7 40 6c 02 00 00 00", "artillery barrage button's spending"),
        Point::keep(0x9526d0usize + 0x41, "48 63 49 10", "spending category").uses(&[spending::CATEGORY]),
        // The assault button's command: the spending at its +0x58, so its
        // fields at +0x60 on.
        Point::keep(0x13af964usize, "48 8d 0d 15 5b 8a 00", "a spending's vtable").uses(&[SPENDING_VTABLE]),
        Point::keep(0x13af98eusize, "48 89 48 58 48 8b cf c7 40 60 65 01 00 00 c7 40 68 09 00 00 00 c7 40 6c 02 00 00 00", "a spending's type and category"),
    ]
}

/// The siege view's three buttons: the capability, the spending's category,
/// and the game's answer.
const ACTIONS: [(&str, i32, usize); 3] = [("bombard", 37, MAY_BOMBARD), ("naval_bombard", 40, MAY_NAVAL_BOMBARD), ("assault", 9, MAY_ASSAULT)];

/// The command that spends the points, as the button posts it.
fn spend_text(country: [u8; 3], category: i32, province: u32) -> String {
    format!(
        "spend_power_command={{ country=\"{}\" power={{ data={province} category={category} skill={} }} }}",
        String::from_utf8_lossy(&country),
        spending::MILITARY
    )
}

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
    let mut object = [0u64; spending::SIZE / 8];
    let at = object.as_mut_ptr() as usize;
    m.write(at, &m.at(SPENDING_VTABLE).to_le_bytes())?;
    m.write(at + spending::TYPE, &spending::TYPE_VALUE.to_le_bytes())?;
    m.write(at + spending::CATEGORY, &category.to_le_bytes())?;
    m.write(at + spending::SKILL, &spending::MILITARY.to_le_bytes())?;
    m.write(at + spending::DATA, &province.to_le_bytes())?;
    m.write(at + spending::SECOND, &(-1i32).to_le_bytes())?;
    let points = m.call(m.at(SPENDING_COST), &[at, ctx.player.0 as usize, 0, 0])? as i32;
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
    commands::post(&spend_text(ctx.player.letters(), entry.1, province))?;
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
        assert_eq!(super::spend_text(*b"FRA", 37, 183), r#"spend_power_command={ country="FRA" power={ data=183 category=37 skill=2 } }"#);
    }
}
