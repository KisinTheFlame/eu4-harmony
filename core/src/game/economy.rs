//! The economy: taking a bank loan and reducing inflation, as the economy
//! view's buttons do, and raising mercantilism, as the trade view's button
//! does (docs/research/power-spending.md, "The economy").
use super::{
    context, game,
    power::{self, CAN_REDUCE_INFLATION, PROMOTE_MERCANTILISM, REDUCE_INFLATION},
    Caller, Ctx, GResult,
};
use crate::{game_string, memory::Memory, points::Point};
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    economy::{EconomyAction, EconomyOption},
};

/// Whether the country may take a bank loan now: `(country, text)`, the
/// text, when not null, given why not.
const MAY_TAKE_LOAN: usize = 0x33e060;
/// Whether the host's rule against ruining a nation keeps the country from
/// loans now: `(country)`; until the date at [`RUIN_BLOCKED_UNTIL`].
const RUIN_BLOCKED: usize = 0x3ed490;
const RUIN_BLOCKED_UNTIL: usize = 0x2738;
/// A bank loan's size: how many months' income (a byte) times the income,
/// in thousandths of ducats; at least 1000.
const LOAN_SIZE: usize = 0x1105;
const ESTIMATED_LOAN: usize = 0xe3c;
/// Whether the country may reduce inflation now: `(country)`.
const MAY_REDUCE_INFLATION: usize = 0x38d5a0;
/// Inflation, in thousandths of a percentage point.
const INFLATION: usize = 0xbd4;
/// A country's modifiers: `(country + MODIFIERS)` answers the object whose
/// `+8` [`HAS_MODIFIER`] asks of one: `(modifiers, modifier)`.
const MODIFIERS_OF: usize = 0x93d750;
const MODIFIERS: usize = 0x28c0;
const HAS_MODIFIER: usize = 0x8e7b60;
/// The modifier `may_not_reduce_inflation`.
const MAY_NOT_REDUCE_INFLATION: usize = 0xf0;
/// Whether the country may raise mercantilism now, and why not:
/// `(country, text, cost text)`.
const MAY_PROMOTE_MERCANTILISM: usize = 0x3f47a0;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(MAY_TAKE_LOAN, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 56 41 57 48 8d ac 24", "may take a loan"),
        Point::keep(RUIN_BLOCKED, "48 89 5c 24 08 57 48 83 ec 20 48 8b 1d d7 29 f5 01 48 8b f9 48 85 db 74", "nation ruining blocked"),
        // The economy view asks the rule, then reads its date.
        Point::keep(
            0xec2aa1usize,
            "e8 ea a9 52 ff 84 c0 0f 84 9d 00 00 00 45 33 c9 45 33 c0 48 8d 15 35 ae d9 00 48 8d 4d a8 e8 dc 09 5f 00 90 41 8b 85 38 27 00 00",
            "economy view's ruin rule",
        )
        .uses(&[RUIN_BLOCKED_UNTIL]),
        // The number of loans allowed starts from the loan's size.
        Point::keep(0x33eb7dusize, "0f b6 99 05 11 00 00 0f af 99 3c 0e 00 00", "bank loan size").uses(&[LOAN_SIZE, ESTIMATED_LOAN]),
        Point::keep(MAY_REDUCE_INFLATION, "48 89 5c 24 08 48 89 6c 24 18 48 89 74 24 20 48 89 54 24 10 57 48 83 ec", "may reduce inflation"),
        // It asks the modifier first, then reads inflation.
        Point::keep(0x38d5beusize, "48 81 c1 c0 28 00 00 e8 86 01 5b 00 48 8d 48 08 ba f0 00 00 00 e8 88 a5 55 00", "may_not_reduce_inflation")
            .uses(&[MODIFIERS, MAY_NOT_REDUCE_INFLATION]),
        Point::keep(0x38d625usize, "39 9e d4 0b 00 00", "inflation").uses(&[INFLATION]),
        Point::keep(MODIFIERS_OF, "48 89 5c 24 10 48 89 7c 24 18 55 48 8d 6c 24 b0 48 81 ec 50 01 00 00 48", "country modifiers"),
        Point::keep(HAS_MODIFIER, "48 89 5c 24 08 57 48 83 ec 20 4c 8b 0d 0f 05 a7 01 48 8b f9 8b da 4c 8d", "has modifier"),
        Point::keep(MAY_PROMOTE_MERCANTILISM, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57", "may promote mercantilism"),
    ]
}

/// The ducats a bank loan of the country at `country` brings, as the view
/// offers it and the loan command reckons them when it runs: the treasury
/// gets them and the loan is of them. The estimated income behind it changes
/// at once with what changes the income (developing a province, granting a
/// privilege).
fn loan_amount(m: &Memory, country: usize) -> GResult<i32> {
    let size = (m.u8(country + LOAN_SIZE)? as i32).saturating_mul(m.i32(country + ESTIMATED_LOAN)?);
    Ok(size.max(1000) / 1000)
}

/// Whether the player can take a bank loan now, as the button's tooltip
/// says.
fn take_loan(m: &Memory, ctx: &Ctx) -> GResult<EconomyOption> {
    let gives = vec![Amount::new(Resource::Ducats, loan_amount(m, ctx.country)? as f64)];
    let option = |choice| Ok(EconomyOption { action: EconomyAction::TakeLoan, choice, gives: gives.clone() });
    let (may, why) = game_string::written(m, |out| m.call(m.at(MAY_TAKE_LOAN), &[ctx.country, out]))?;
    if may & 0xff == 0 {
        let mut reasons = power::tooltip(m, c"NO_MORE_BANKLOANS", &[])?;
        reasons.extend(super::reasons(&why));
        return option(Choice::cannot(reasons, Vec::new()));
    }
    if m.call(m.at(RUIN_BLOCKED), &[ctx.country])? & 0xff != 0 {
        let rule = crate::unicode::game_text::to_utf8(&game_string::localized(m, c"FE_BLOCK_NATION_RUINING".as_ptr() as usize)?);
        let until = power::date_text(m, m.i32(ctx.country + RUIN_BLOCKED_UNTIL)?)?;
        return option(Choice::cannot(power::tooltip(m, c"NO_NATION_RUINING_LOANS", &[(c"DATE", until), (c"OPTION", rule)])?, Vec::new()));
    }
    option(Choice::can(Vec::new()))
}

/// Whether the player can reduce inflation now, as the button's tooltip
/// says.
fn reduce_inflation(m: &Memory, ctx: &Ctx) -> GResult<Choice> {
    let cost = power::cost(m, ctx.player, REDUCE_INFLATION, PowerKind::Adm, 0)?;
    let price = vec![Amount::new(Resource::Adm, cost as f64)];
    if m.call(m.at(MAY_REDUCE_INFLATION), &[ctx.country, 0])? & 0xff != 0 {
        return Ok(Choice::can(price));
    }
    let mut reasons = Vec::new();
    let modifiers = m.call(m.at(MODIFIERS_OF), &[ctx.country + MODIFIERS])?;
    if m.call(m.at(HAS_MODIFIER), &[modifiers + 8, MAY_NOT_REDUCE_INFLATION])? & 0xff != 0 {
        reasons.extend(power::tooltip(m, c"CAN_NOT_REDUCE_INFLATION_TT", &[(c"VAL", power::whole(0))])?);
    }
    if m.i32(ctx.country + INFLATION)? <= 0 {
        reasons.extend(power::tooltip(m, c"NO_INFLATION", &[(c"VAL", power::whole(0))])?);
    }
    let have = power::points_of(m, ctx.country, PowerKind::Adm)?;
    if cost > have {
        let values = [(c"POWER_COST", power::whole(cost)), (c"POWER_AVAILABLE", power::whole(have))];
        reasons.extend(power::tooltip(m, c"CV_COMBAT_INFLATION_NOPOWER", &values)?);
    }
    reasons.extend(power::scripted(m, CAN_REDUCE_INFLATION, ctx.player)?.1);
    Ok(Choice::cannot(reasons, price))
}

/// Whether the player can raise mercantilism now, as the trade view's
/// button says: the game's own check writes why not.
fn raise_mercantilism(m: &Memory, ctx: &Ctx) -> GResult<Choice> {
    let cost = power::cost(m, ctx.player, PROMOTE_MERCANTILISM, PowerKind::Dip, 0)?;
    let price = vec![Amount::new(Resource::Dip, cost as f64)];
    let (may, why) = game_string::written(m, |out| m.call(m.at(MAY_PROMOTE_MERCANTILISM), &[ctx.country, out, 0]))?;
    Ok(if may & 0xff != 0 { Choice::can(price) } else { Choice::cannot(power::plain_lines(&why), price) })
}

/// What the buttons offer now.
pub fn options(_: Caller) -> GResult<Vec<EconomyOption>> {
    let m = game()?;
    let ctx = context(m)?;
    let spending = |action, choice| EconomyOption { action, choice, gives: Vec::new() };
    Ok(vec![
        take_loan(m, &ctx)?,
        spending(EconomyAction::ReduceInflation, reduce_inflation(m, &ctx)?),
        spending(EconomyAction::RaiseMercantilism, raise_mercantilism(m, &ctx)?),
    ])
}

/// Refuses `choice` with its reasons when the player cannot.
fn allowed(choice: Choice, doing: &str) -> GResult<()> {
    if choice.can {
        Ok(())
    } else {
        Err(eu4_harmony_checks::choices::not_allowed(doing, &choice.reasons).into())
    }
}

/// The bank loan command in its written form, as the button makes it: the
/// country by its tag, the rest the game's defaults.
fn written_loan(country: [u8; 3]) -> String {
    format!("take_bank_loan={{ country=\"{}\" }}", String::from_utf8_lossy(&country))
}

/// Takes a bank loan, as the button does.
pub fn take_bank_loan(_: Caller) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    allowed(take_loan(m, &ctx)?.choice, "take a loan")?;
    power::post(&written_loan(ctx.player.letters()), "take a loan")
}

/// Reduces inflation, as the button does.
pub fn reduce(_: Caller) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    allowed(reduce_inflation(m, &ctx)?, "reduce inflation")?;
    power::spend(ctx.player, REDUCE_INFLATION, PowerKind::Adm, 0, "reduce inflation")
}

/// Raises mercantilism, as the trade view's button does.
pub fn promote(_: Caller) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    allowed(raise_mercantilism(m, &ctx)?, "raise mercantilism")?;
    power::spend(ctx.player, PROMOTE_MERCANTILISM, PowerKind::Dip, 0, "raise mercantilism")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_loan_command_names_the_country() {
        assert_eq!(super::written_loan(*b"FRA"), r#"take_bank_loan={ country="FRA" }"#);
    }
}
