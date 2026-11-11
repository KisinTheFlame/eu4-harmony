//! The economy: taking a bank loan, reducing inflation, raising and
//! lowering war taxes and declaring bankruptcy, as the economy view's
//! buttons do, and raising mercantilism, as the trade view's button does
//! (docs/research/power-spending.md, "The economy" and "The country's other
//! spendings").
use super::commands::{write, Block};
use super::{
    action, modifiers,
    power::{self, CAN_DECLARE_BANKRUPTCY, CAN_REDUCE_INFLATION, PROMOTE_MERCANTILISM, REDUCE_INFLATION},
    Ctx, GResult, Session, Tag,
};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::country,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    economy::{EconomyAction, EconomyOption},
};

/// Whether the country may take a bank loan now: `(country, text)`, the
/// text, when not null, given why not.
const MAY_TAKE_LOAN: GameFunction<2> = GameFunction::at(0x33e060);
/// Whether the host's rule against ruining a nation keeps the country from
/// loans now: `(country)`; until the date at [`RUIN_BLOCKED_UNTIL`].
const RUIN_BLOCKED: GameFunction<1> = GameFunction::at(0x3ed490);
const RUIN_BLOCKED_UNTIL: usize = 0x2738;
/// A bank loan's size: how many months' income (a byte) times the income,
/// in thousandths of ducats; at least 1000.
const LOAN_SIZE: usize = 0x1105;
const ESTIMATED_LOAN: usize = 0xe3c;
/// Whether the country may reduce inflation now: `(country)`.
const MAY_REDUCE_INFLATION: GameFunction<2> = GameFunction::at(0x38d5a0);
/// The modifier `may_not_reduce_inflation`.
const MAY_NOT_REDUCE_INFLATION: usize = 0xf0;
/// Whether the country may raise mercantilism now, and why not:
/// `(country, text, cost text)`.
const MAY_PROMOTE_MERCANTILISM: GameFunction<3> = GameFunction::at(0x3f47a0);
/// `(country, text) -> bool`: whether the country may declare bankruptcy
/// now, the economy view's button's check, writing its tooltip: why not,
/// but nothing when `can_declare_bankruptcy` does not allow it.
const MAY_GO_BANKRUPT: GameFunction<2> = GameFunction::at(0x33b600);

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
        Point::keep(0x38d5beusize, "48 81 c1 c0 28 00 00 e8 86 01 5b 00 48 8d 48 08 ba f0 00 00 00 e8 88 a5 55 00", "may_not_reduce_inflation").uses(&[
            country::MODIFIERS,
            modifiers::VALUES,
            MAY_NOT_REDUCE_INFLATION,
        ]),
        Point::keep(0x38d625usize, "39 9e d4 0b 00 00", "inflation").uses(&[country::INFLATION]),
        Point::keep(MAY_PROMOTE_MERCANTILISM, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55 41 54 41 55 41 56 41 57", "may promote mercantilism"),
        Point::keep(MAY_GO_BANKRUPT, "48 89 5c 24 08 57 48 83 ec 40 48 8b da 48 8b f9", "may go bankrupt"),
        // The war taxes command's check: raising them while they are not
        // raised, lowering them while they are; its data the country's tag
        // and whether to raise, written as `members` (`0x2aa9`).
        Point::keep(
            0xc20f0usize,
            "48 8b 41 52 48 c1 e8 20 80 79 50 00 4c 0f bf c0 48 8b 05 c9 b7 27 02 48 8b 90 18 01 00 00 4a 8b 04 c2 0f b6 80 08 0d 00 00 74 05 84 c0 0f 94 c0 c3",
            "war taxes command check",
        )
        .uses(&[country::WAR_TAXES]),
        Point::keep(0xc2069usize, "ba a9 2a 00 00", "war taxes command's members"),
        // The go_bankrupt command's writer writes the country alone.
        Point::keep(0x2a4d95usize, "ba 9d 28 00 00", "go_bankrupt command's country"),
    ]
}

/// Whether the player can raise war taxes (`raise`) or lower them now:
/// raising them while they are not raised, lowering them while they are, as
/// the command's check allows; they cost nothing at once.
fn war_taxes(m: &Memory, ctx: &Ctx, raise: bool) -> GResult<Choice> {
    let raised = m.u8(ctx.country + country::WAR_TAXES)? != 0;
    Ok(match (raise, raised) {
        (true, false) | (false, true) => Choice::can(Vec::new()),
        (true, true) => Choice::cannot(vec![checks::RAISED.to_string()], Vec::new()),
        (false, false) => Choice::cannot(vec![checks::NOT_RAISED.to_string()], Vec::new()),
    })
}

/// Whether the player can declare bankruptcy now, as the button's tooltip
/// says, or `can_declare_bankruptcy`'s failing lines when it says nothing.
fn bankruptcy(m: &Memory, ctx: &Ctx) -> GResult<Choice> {
    let (may, why) = game_string::written(m, |out| MAY_GO_BANKRUPT.call(m, [ctx.country, out]))?;
    if may & 0xff != 0 {
        return Ok(Choice::can(Vec::new()));
    }
    let mut reasons = power::plain_lines(&why);
    if reasons.is_empty() {
        reasons = power::scripted(m, CAN_DECLARE_BANKRUPTCY, ctx.player)?.1;
    }
    Ok(Choice::cannot(reasons, Vec::new()))
}

/// The war taxes command: the country's tag, then whether to raise them.
fn written_war_taxes(tag: Tag, raise: bool) -> String {
    write("raise_war_taxes", Block::new().block("members", Block::new().tag("dummy", tag.letters()).word("dummy", if raise { "yes" } else { "no" })))
}

/// The bankruptcy command: the country alone.
fn written_bankruptcy(tag: Tag) -> String {
    write("go_bankrupt", Block::new().tag("country", tag.letters()))
}

/// The refusals the framework words itself in the economy view.
pub mod checks {
    /// Raising war taxes that are raised, and lowering ones that are not:
    /// the command's check says no without a word.
    pub const RAISED: &str = "War taxes are raised already.";
    pub const NOT_RAISED: &str = "War taxes are not raised.";
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
    let (may, why) = game_string::written(m, |out| MAY_TAKE_LOAN.call(m, [ctx.country, out]))?;
    if may & 0xff == 0 {
        let mut reasons = power::tooltip(m, c"NO_MORE_BANKLOANS", &[])?;
        reasons.extend(super::reasons(&why));
        return option(Choice::cannot(reasons, Vec::new()));
    }
    if RUIN_BLOCKED.call(m, [ctx.country])? & 0xff != 0 {
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
    if MAY_REDUCE_INFLATION.call(m, [ctx.country, 0])? & 0xff != 0 {
        return Ok(Choice::can(price));
    }
    let mut reasons = Vec::new();
    if modifiers::has(m, ctx.country, MAY_NOT_REDUCE_INFLATION)? {
        reasons.extend(power::tooltip(m, c"CAN_NOT_REDUCE_INFLATION_TT", &[(c"VAL", power::whole(0))])?);
    }
    if m.i32(ctx.country + country::INFLATION)? <= 0 {
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
    let (may, why) = game_string::written(m, |out| MAY_PROMOTE_MERCANTILISM.call(m, [ctx.country, out, 0]))?;
    Ok(if may & 0xff != 0 { Choice::can(price) } else { Choice::cannot(power::plain_lines(&why), price) })
}

/// What the buttons offer now.
pub fn options(Session { m, ctx, .. }: Session) -> GResult<Vec<EconomyOption>> {
    let spending = |action, choice| EconomyOption { action, choice, gives: Vec::new() };
    Ok(vec![
        take_loan(m, &ctx)?,
        spending(EconomyAction::ReduceInflation, reduce_inflation(m, &ctx)?),
        spending(EconomyAction::RaiseMercantilism, raise_mercantilism(m, &ctx)?),
        spending(EconomyAction::RaiseWarTaxes, war_taxes(m, &ctx, true)?),
        spending(EconomyAction::LowerWarTaxes, war_taxes(m, &ctx, false)?),
        spending(EconomyAction::DeclareBankruptcy, bankruptcy(m, &ctx)?),
    ])
}

/// Raises war taxes, as the button does.
pub fn raise_war_taxes(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::carry_out(commands, &war_taxes(m, &ctx, true)?, "raise war taxes", || Ok(written_war_taxes(ctx.player, true)))
}

/// Lowers war taxes, as the button does.
pub fn lower_war_taxes(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::carry_out(commands, &war_taxes(m, &ctx, false)?, "lower war taxes", || Ok(written_war_taxes(ctx.player, false)))
}

/// Declares bankruptcy, as the button does once the player confirms.
pub fn declare_bankruptcy(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::carry_out(commands, &bankruptcy(m, &ctx)?, "declare bankruptcy", || Ok(written_bankruptcy(ctx.player)))
}

/// The bank loan command in its written form, as the button makes it: the
/// country by its tag, the rest the game's defaults.
fn written_loan(country: [u8; 3]) -> String {
    write("take_bank_loan", Block::new().tag("country", country))
}

/// Takes a bank loan, as the button does.
pub fn take_bank_loan(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::carry_out(commands, &take_loan(m, &ctx)?.choice, "take a loan", || Ok(written_loan(ctx.player.letters())))
}

/// Reduces inflation, as the button does.
pub fn reduce(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::allowed(&reduce_inflation(m, &ctx)?, "reduce inflation")?;
    power::spend(commands, ctx.player, REDUCE_INFLATION, PowerKind::Adm, 0, "reduce inflation")
}

/// Raises mercantilism, as the trade view's button does.
pub fn promote(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::allowed(&raise_mercantilism(m, &ctx)?, "raise mercantilism")?;
    power::spend(commands, ctx.player, PROMOTE_MERCANTILISM, PowerKind::Dip, 0, "raise mercantilism")
}

crate::catalogue::bind! {
    eu4_harmony_remote::economy: EconomyRequest => EconomyAnswer;
    Options => options;
    TakeLoan => take_bank_loan;
    ReduceInflation => reduce;
    RaiseMercantilism => promote;
    RaiseWarTaxes => raise_war_taxes;
    LowerWarTaxes => lower_war_taxes;
    DeclareBankruptcy => declare_bankruptcy;
}

#[cfg(test)]
mod tests {
    use crate::game::As;
    use super::*;
    use crate::catalogue::fake::tur;
    use crate::game::{
        buffers::{self, write_reason, Image, Object},
        commands::record,
        modifiers::{HAS, OF_COUNTRY},
        power::{COST, FUNCTIONS, NO_ID},
        Caller, Fail, Status,
    };
    use std::cell::Cell;

    #[test]
    fn the_loan_command_names_the_country() {
        assert_eq!(super::written_loan(*b"FRA"), r#"take_bank_loan={ country="FRA" }"#);
    }

    /// What the game's checks answer in this test's thread.
    #[derive(Clone, Copy)]
    struct Game {
        loan: bool,
        ruin_blocked: bool,
        reduce: bool,
        may_not_reduce: bool,
        promote: bool,
    }

    const LETS: Game = Game { loan: true, ruin_blocked: false, reduce: true, may_not_reduce: false, promote: true };

    thread_local! {
        static GAME: Cell<Game> = const { Cell::new(LETS) };
    }

    fn game() -> Game {
        GAME.with(Cell::get)
    }

    extern "C" fn fifty() -> usize {
        50
    }

    extern "C" fn may_take_loan(_: usize, text: usize) -> usize {
        if game().loan {
            return 1;
        }
        unsafe { write_reason(text, b"- in debt") };
        0
    }

    extern "C" fn ruin_blocked() -> usize {
        game().ruin_blocked.into()
    }

    extern "C" fn may_reduce() -> usize {
        game().reduce.into()
    }

    /// The country's modifiers: a buffer of the test's whose values are
    /// never read but by [`has`].
    extern "C" fn modifiers(at: usize) -> usize {
        at
    }

    extern "C" fn has() -> usize {
        game().may_not_reduce.into()
    }

    extern "C" fn may_promote(_: usize, text: usize) -> usize {
        if game().promote {
            return 1;
        }
        unsafe { write_reason(text, b"Too many traders") };
        0
    }

    /// The bankruptcy check: refused with no loans, as this test arranges
    /// it, writing the game's line.
    extern "C" fn may_go_bankrupt(country: usize, text: usize) -> usize {
        if unsafe { *((country + LOAN_SIZE) as *const u8) } != 0 {
            return 1;
        }
        unsafe { write_reason(text, b"VOLUNTEER_BANKRUPT_NO_LOANS") };
        0
    }

    /// The month and year a date is written as, for a test.
    extern "C" fn month_year(_: usize, out: usize) -> usize {
        unsafe { write_reason(out, b"May 1450") };
        0
    }

    struct Ottomans {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    /// The Ottomans with 100 administrative points, 2 thousandths of
    /// inflation, loans of 12 months' income of 50 ducats; the game's checks
    /// answering as `game` says, every spending costing 50.
    fn ottomans(game: Game, adm: i32, inflation: i32) -> Ottomans {
        GAME.with(|g| g.set(game));
        let image = Image::new();
        image
            .localisation()
            .function(COST.rva(), fifty as *const ())
            .u64(NO_ID, 0)
            .u64(FUNCTIONS, 0)
            .function(MAY_TAKE_LOAN.rva(), may_take_loan as *const ())
            .function(RUIN_BLOCKED.rva(), ruin_blocked as *const ())
            .function(MAY_REDUCE_INFLATION.rva(), may_reduce as *const ())
            .function(OF_COUNTRY.rva(), modifiers as *const ())
            .function(HAS.rva(), has as *const ())
            .function(MAY_PROMOTE_MERCANTILISM.rva(), may_promote as *const ())
            .function(MAY_GO_BANKRUPT.rva(), may_go_bankrupt as *const ())
            .function(crate::wording::dates::NEXT_MONTH_YEAR.rva(), month_year as *const ());
        let player = Object::country(tur(), country::MODIFIERS + 8)
            .i32(country::POWER, adm)
            .i32(country::INFLATION, inflation)
            .u8(LOAN_SIZE, 12)
            .i32(ESTIMATED_LOAN, 50_000)
            .i32(RUIN_BLOCKED_UNTIL, 1);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), 0, tur(), player.at(), countries.at(), Caller::Remote(As::Player));
        Ottomans { _image: image, _objects: vec![player, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn the_buttons_offer_a_loan_of_months_income_and_their_spendings() {
        let options = options(ottomans(LETS, 100, 2).session).unwrap();
        let summary: Vec<_> = options.iter().map(|o| (o.action, o.choice.can, o.choice.cost.clone(), o.gives.clone())).collect();
        assert_eq!(
            summary,
            [
                (EconomyAction::TakeLoan, true, vec![], vec![Amount::new(Resource::Ducats, 600.0)]),
                (EconomyAction::ReduceInflation, true, vec![Amount::new(Resource::Adm, 50.0)], vec![]),
                (EconomyAction::RaiseMercantilism, true, vec![Amount::new(Resource::Dip, 50.0)], vec![]),
                (EconomyAction::RaiseWarTaxes, true, vec![], vec![]),
                (EconomyAction::LowerWarTaxes, false, vec![], vec![]),
                (EconomyAction::DeclareBankruptcy, true, vec![], vec![]),
            ]
        );
    }

    #[test]
    fn war_taxes_are_raised_or_lowered_by_the_command_as_they_stand() {
        let ottomans = ottomans(LETS, 100, 2);
        assert_eq!(refused(lower_war_taxes(ottomans.session)), (Status::Rejected, checks::NOT_RAISED.into()));
        raise_war_taxes(ottomans.session).unwrap();
        let raised = self::ottomans(LETS, 100, 2);
        let country = raised.session.ctx.country;
        unsafe { *((country + country::WAR_TAXES) as *mut u8) = 1 };
        assert_eq!(refused(raise_war_taxes(raised.session)), (Status::Rejected, checks::RAISED.into()));
        lower_war_taxes(raised.session).unwrap();
        assert_eq!(
            record::posted(),
            [r#"raise_war_taxes={ members={ dummy="TUR" dummy=yes } }"#, r#"raise_war_taxes={ members={ dummy="TUR" dummy=no } }"#]
        );
    }

    #[test]
    fn bankruptcy_is_declared_or_refused_with_the_button_s_tooltip() {
        let ottomans = ottomans(LETS, 100, 2);
        declare_bankruptcy(ottomans.session).unwrap();
        assert_eq!(record::posted(), [r#"go_bankrupt={ country="TUR" }"#]);
        let no_loans = self::ottomans(LETS, 100, 2);
        unsafe { *((no_loans.session.ctx.country + LOAN_SIZE) as *mut u8) = 0 };
        assert_eq!(refused(declare_bankruptcy(no_loans.session)), (Status::Rejected, "VOLUNTEER_BANKRUPT_NO_LOANS".into()));
        assert!(record::posted().is_empty());
    }

    #[test]
    fn the_buttons_post_their_commands() {
        let ottomans = ottomans(LETS, 100, 2);
        take_bank_loan(ottomans.session).unwrap();
        reduce(ottomans.session).unwrap();
        promote(ottomans.session).unwrap();
        assert_eq!(
            record::posted(),
            [
                r#"take_bank_loan={ country="TUR" }"#,
                r#"spend_power_command={ country="TUR" power={ category=15 skill=0 } }"#,
                r#"spend_power_command={ country="TUR" power={ category=26 skill=1 } }"#,
            ]
        );
    }

    #[test]
    fn a_loan_is_refused_with_the_game_s_reasons_or_the_ruin_rule() {
        let no_loan = Game { loan: false, ..LETS };
        assert_eq!(refused(take_bank_loan(ottomans(no_loan, 100, 2).session)), (Status::Rejected, "NO_MORE_BANKLOANS\nin debt".into()));
        let ruined = Game { ruin_blocked: true, ..LETS };
        assert_eq!(
            refused(take_bank_loan(ottomans(ruined, 100, 2).session)),
            (Status::Rejected, "NO_NATION_RUINING_LOANS DATE=May 1450 OPTION=FE_BLOCK_NATION_RUINING".into())
        );
        assert!(record::posted().is_empty());
    }

    #[test]
    fn reducing_inflation_is_refused_with_every_reason_the_tooltip_gives() {
        let refusing = Game { reduce: false, may_not_reduce: true, ..LETS };
        assert_eq!(
            refused(reduce(ottomans(refusing, 10, 0).session)),
            (Status::Rejected, "CAN_NOT_REDUCE_INFLATION_TT VAL=0\nNO_INFLATION VAL=0\nCV_COMBAT_INFLATION_NOPOWER POWER_COST=50 POWER_AVAILABLE=10".into())
        );
        let no_promoting = Game { promote: false, ..LETS };
        assert_eq!(refused(promote(ottomans(no_promoting, 100, 2).session)), (Status::Rejected, "Too many traders".into()));
        assert!(record::posted().is_empty());
    }
}
