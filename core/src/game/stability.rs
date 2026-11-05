//! Stability: raising it with administrative points, as the stability and
//! expansion view's button does (docs/research/power-spending.md,
//! "Stability").
use super::{
    context, game,
    power::{self, BOOST_STABILITY, CAN_INCREASE_STABILITY},
    Caller, GResult,
};
use crate::points::Point;
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    stability::{StabilityAction, StabilityOption},
};

/// A country's stability, in thousandths, and the most it can be.
const STABILITY: usize = 0xbd8;
const MOST: i32 = 3000;

pub fn points() -> Vec<Point> {
    // The spend_power command's check refuses stability at its most.
    vec![Point::keep(
        0x94f6b1usize,
        "48 8b 05 18 e2 9e 01 48 8b 88 18 01 00 00 4a 8b 04 31 81 b8 d8 0b 00 00 b8 0b 00 00 0f 8c 30 03 00 00 32 c0",
        "stability at its most",
    )
    .uses(&[STABILITY, MOST as usize])]
}

/// Whether the player can raise stability now, as the button's tooltip
/// says: not at the most, not when `can_increase_stability` does not allow
/// it, not with too few points.
fn choice() -> GResult<Choice> {
    let m = game()?;
    let ctx = context(m)?;
    let cost = power::cost(m, ctx.player, BOOST_STABILITY, PowerKind::Adm, 0)?;
    let price = vec![Amount::new(Resource::Adm, cost as f64)];
    if m.i32(ctx.country + STABILITY)? >= MOST {
        return Ok(Choice::cannot(power::tooltip(m, c"STABILITY_BOOST_MAXED", &[])?, price));
    }
    let (allowed, why) = power::scripted(m, CAN_INCREASE_STABILITY, ctx.player)?;
    if !allowed {
        return Ok(Choice::cannot(why, price));
    }
    let have = power::points_of(m, ctx.country, PowerKind::Adm)?;
    if cost > have {
        let values = [(c"POWER_COST", power::whole(cost)), (c"POWER_AVAILABLE", power::whole(have))];
        return Ok(Choice::cannot(power::tooltip(m, c"STABILITY_BOOST_NOPOWER", &values)?, price));
    }
    Ok(Choice::can(price))
}

/// What the stability button offers now.
pub fn options(_: Caller) -> GResult<Vec<StabilityOption>> {
    Ok(vec![StabilityOption { action: StabilityAction::Raise, choice: choice()? }])
}

/// Raises stability by one, as the button does.
pub fn raise(_: Caller) -> GResult<()> {
    let choice = choice()?;
    if !choice.can {
        return Err(eu4_harmony_checks::choices::not_allowed("raise stability", &choice.reasons).into());
    }
    let ctx = context(game()?)?;
    power::spend(ctx.player, BOOST_STABILITY, PowerKind::Adm, 0, "raise stability")
}
