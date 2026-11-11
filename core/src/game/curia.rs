//! The papacy view: the papal actions, calling the ecumenical council,
//! buying indulgence, investing papal influence in the curia, and the curia
//! controller's golden bulls, through the game's own commands, with the
//! game's own checks (docs/research/religion.md, "The papacy and the
//! curia").
use super::{action, conditions, content, Ctx, GResult, Session, Tag};
use crate::{
    containers, game_string,
    memory::Memory,
    objects::{campaign::country, map},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::choices::{Amount, Choice, Resource};
use eu4_harmony_remote::religion::{args, ReligionOption};

/// A religion's papacy, null for a religion without one (Catholicism has
/// it).
const RELIGION_PAPACY: usize = 0x578;
/// `(papacy) -> curia`: the curia of the papacy's religion, which the
/// world keeps with the religion's record.
const CURIA: GameFunction<1> = GameFunction::at(0xa47db0);
/// The papacy's actions, a `std::vector` of pointers, which the papal
/// action command names one by its place in.
const PAPACY_ACTIONS: usize = 0x18;
/// A papal action: its key, a `std::string`; its place among the
/// papacy's actions, i32; its `allow` and `potential`, content triggers.
const ACTION_KEY: usize = 0x10;
const ACTION_INDEX: usize = 0x30;
const ACTION_ALLOW: usize = 0x38;
const ACTION_POTENTIAL: usize = 0xb0;
/// `(action, i32 out, tag) -> i32 out`: what the action costs the country
/// in papal influence, and from the curia's treasury, in thousandths, its
/// modifiers counted.
const ACTION_COST: GameFunction<3> = GameFunction::at(0xa80960);
const ACTION_TREASURY_COST: GameFunction<3> = GameFunction::at(0xa80a50);
/// The curia: whether it is active, a byte, which the papal action
/// command's check asks; its treasury, i32 thousandths of a ducat; the
/// golden bull enacted, a pointer, null for none.
const CURIA_ACTIVE: usize = 0x20;
const CURIA_TREASURY: usize = 0x100;
const CURIA_BULL: usize = 0xf8;
/// `(curia, tag, text) -> bool`: whether the country may call the
/// ecumenical council, buy indulgence or invest papal influence in the
/// curia now, writing the papacy view's lines into the `std::string` text
/// when it is not null; the commands' checks ask them with none. Neither
/// of the first two reads its first argument.
const MAY_CALL_COUNCIL: GameFunction<3> = GameFunction::at(0xdbc020);
const MAY_BUY_INDULGENCE: GameFunction<3> = GameFunction::at(0xdbdca0);
const MAY_INVEST: GameFunction<3> = GameFunction::at(0xdb7c80);
/// `(curia, i32 out) -> i32 out`: what calling the council costs, and
/// `(curia, i32 out, tag) -> i32 out`: what indulgence costs the country,
/// thousandths of a ducat; and what investing costs it, thousandths of
/// papal influence.
const COUNCIL_PRICE: GameFunction<2> = GameFunction::at(0xdbc3a0);
const INDULGENCE_PRICE: GameFunction<3> = GameFunction::at(0xdbdb80);
const INVEST_COST: GameFunction<3> = GameFunction::at(0xdb8080);
/// How much the view's button invests, the command's `index`, a define.
const INVEST_AMOUNT: usize = 0x233dadc;
/// The golden bulls, a `std::map` by their keys whose node holds the bull
/// itself from [`BULL`].
const GOLDEN_BULLS: usize = 0x2358050;
const BULL: usize = 0x40;
/// `(curia, bull, tag, text) -> bool`: whether the country may enact the
/// golden bull now, as the curia controller, writing the view's lines.
const MAY_PICK_BULL: GameFunction<4> = GameFunction::at(0xdbf750);
/// What a golden bull costs from the curia's treasury: the value at `+0xe4`
/// of the world's `+0x2420` times a define, in thousandths.
const WORLD_CURIA_BASE: usize = 0x2420;
const CURIA_BASE_VALUE: usize = 0xe4;
const BULL_COST_FACTOR: usize = 0x233e598;

pub fn points() -> Vec<Point> {
    vec![
        // The council command's check: the religion's papacy, then its curia.
        Point::keep(0xc2d7eusize, "48 8b 89 78 05 00 00", "religion's papacy").uses(&[RELIGION_PAPACY]),
        Point::keep(CURIA, "40 53 48 83 ec 20 48 8b 59 60 48 8b cb 48 8b 03 ff 50 40 84 c0 74", "papacy's curia"),
        // The papal action command's execute: the action by its place, the
        // papal influence it takes, the treasury it adds to; its check, the
        // curia active.
        Point::keep(0xaab1e0usize, "48 8b 40 18", "papacy's actions").uses(&[PAPACY_ACTIONS]),
        Point::keep(0xaab3e3usize, "41 8b 97 50 0f 00 00", "papal influence").uses(&[country::PAPAL_INFLUENCE]),
        Point::keep(0xaab470usize, "01 b8 00 01 00 00", "curia treasury").uses(&[CURIA_TREASURY]),
        Point::keep(0xaaba3ausize, "80 78 20 00", "curia active").uses(&[CURIA_ACTIVE]),
        // The view's tooltip: the action's key; its button: the action's
        // place; the action's lines: its allow; the execute: its potential.
        Point::keep(0x12c7ce7usize, "48 83 c2 10", "papal action key").uses(&[ACTION_KEY]),
        Point::keep(0x12c7759usize, "8b 50 30", "papal action index").uses(&[ACTION_INDEX]),
        Point::keep(0xa806d6usize, "48 83 c1 38", "papal action allow").uses(&[ACTION_ALLOW]),
        Point::keep(0xaab35busize, "48 8b 9e b8 00 00 00", "papal action potential").uses(&[ACTION_POTENTIAL + 8]),
        Point::keep(ACTION_COST, "48 89 5c 24 18 57 48 83 ec 20 48 8b da 48 8b f9 48 8b 05 59 cf 8b 01 49 c1 e8 20 4d 0f bf c8 48", "papal action cost"),
        Point::keep(
            ACTION_TREASURY_COST,
            "48 89 5c 24 18 55 56 57 48 83 ec 20 48 8b f2 48 8b e9 4c 8b 0d 67 ce 8b 01 49 c1 e8 20 49 0f bf",
            "papal action curia cost",
        ),
        Point::keep(
            MAY_CALL_COUNCIL,
            "48 89 5c 24 08 48 89 74 24 10 55 57 41 55 41 56 41 57 48 8d 6c 24 c9 48 81 ec 90 00 00 00 4d 8b f8 48 8b da",
            "may call ecumenical council",
        ),
        Point::keep(
            MAY_BUY_INDULGENCE,
            "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 48 89 4c 24 08 55 41 54 41 55 41 56 41 57 48 8d 6c 24 c9 48 81",
            "may buy indulgence",
        ),
        Point::keep(
            MAY_INVEST,
            "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55 41 56 41 57 48 8b ec 48 83 ec 60 4d 8b f0 48 8b da 48 8b f1",
            "may invest papal influence",
        ),
        Point::keep(COUNCIL_PRICE, "4c 8b d2 48 8b 41 18 48 8b 48 58 48 c1 e9 20 4c 0f bf c1", "ecumenical council price"),
        Point::keep(INDULGENCE_PRICE, "48 89 5c 24 08 4c 8b d2 49 c1 e8 20 4d 0f bf c8", "indulgence price"),
        Point::keep(INVEST_COST, "48 8b 41 18 4c 8b d2 49 c1 e8 20 4d 0f bf c8 4c 63 58 6c 44 89 1a", "papal investment cost"),
        // The view's invest button: the amount it invests.
        Point::keep(0x12cc040usize, "8b 15 96 1a 07 01", "papal investment amount").uses(&[INVEST_AMOUNT]),
        // The golden bull command's check: the bulls by their keys, the
        // bull in its node; the bull's check: the bull enacted, the cost
        // against the treasury.
        Point::keep(0x478034usize, "4c 8b 25 15 00 ee 01", "golden bulls").uses(&[GOLDEN_BULLS]),
        Point::keep(0x47812dusize, "49 8d 56 40", "golden bull in its node").uses(&[BULL]),
        Point::keep(
            MAY_PICK_BULL,
            "48 89 5c 24 18 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec a0 00 00 00",
            "may pick golden bull",
        ),
        Point::keep(0xdbf7c1usize, "49 39 9d f8 00 00 00", "golden bull enacted").uses(&[CURIA_BULL]),
        Point::keep(0xdbfc58usize, "48 8b 88 20 24 00 00 48 63 91 e4 00 00 00 48 63 05 2b e9 57 01", "golden bull cost").uses(&[
            WORLD_CURIA_BASE,
            CURIA_BASE_VALUE,
            BULL_COST_FACTOR,
        ]),
    ]
}

/// The player's papacy and its curia, while its religion has a papacy.
fn papacy(m: &Memory, country: usize) -> GResult<Option<(usize, usize)>> {
    let religion = m.ptr(country + country::STATE_RELIGION)?;
    if religion == 0 {
        return Ok(None);
    }
    match m.ptr(religion + RELIGION_PAPACY)? {
        0 => Ok(None),
        papacy => Ok(Some((papacy, CURIA.call(m, [papacy])?))),
    }
}

fn own_papacy(m: &Memory, country: usize) -> GResult<(usize, usize)> {
    papacy(m, country)?.ok_or_else(|| super::religion::checks::not_of_religion("papacy"))
}

/// Thousandths as the number they stand for.
fn whole(thousandths: i32) -> f64 {
    f64::from(thousandths) / 1000.0
}

/// What `function` writes into its out argument, given `(subject, out,
/// more...)`.
fn out(function: impl FnOnce(usize) -> GResult<usize>) -> GResult<i32> {
    let mut value = 0i32;
    function(&mut value as *mut i32 as usize)?;
    Ok(value)
}

/// A line of the view's own, its `$COST$` the amount.
fn line(m: &Memory, key: &str, cost: f64) -> GResult<String> {
    let key = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
    let amount = format!("{}", cost.round() as i64);
    let text = game_string::localized_with(m, key.as_ptr() as usize, &[(c"COST", amount.as_bytes())])?;
    Ok(crate::unicode::game_text::plain(&text))
}

/// The papal actions the view offers the player: those whose potential
/// holds.
fn actions(m: &Memory, ctx: &Ctx, papacy: usize) -> GResult<Vec<usize>> {
    let mut out = Vec::new();
    for action in containers::pointers(m, papacy + PAPACY_ACTIONS, containers::MOST)? {
        if conditions::holds(m, action + ACTION_POTENTIAL, ctx.player, 0)? {
            out.push(action);
        }
    }
    Ok(out)
}

/// Whether the player may take `action` now, as the view's tooltip says:
/// its allow's conditions that do not hold, too little papal influence or
/// curia treasury; and its costs.
fn action_choice(m: &Memory, ctx: &Ctx, curia: usize, action: usize) -> GResult<Choice> {
    let tag = ctx.player.0 as usize;
    let influence = out(|o| Ok(ACTION_COST.call(m, [action, o, tag])?))?;
    let treasury = out(|o| Ok(ACTION_TREASURY_COST.call(m, [action, o, tag])?))?;
    let mut cost = vec![Amount::new(Resource::PapalInfluence, whole(influence))];
    if treasury > 0 {
        cost.push(Amount::new(Resource::CuriaTreasury, whole(treasury)));
    }
    let mut reasons = Vec::new();
    if !conditions::holds(m, action + ACTION_ALLOW, ctx.player, 0)? {
        reasons.extend(conditions::failing(m, action + ACTION_ALLOW, ctx.player, 0)?);
        if reasons.is_empty() {
            reasons.push(line(m, "PAPAL_NOT_ALLOW", 0.0)?);
        }
    }
    if m.i32(ctx.country + country::PAPAL_INFLUENCE)? < influence {
        reasons.push(line(m, "PAPAL_NOT_ENOUGH", whole(influence))?);
    }
    if treasury > 0 && m.i32(curia + CURIA_TREASURY)? < treasury {
        reasons.push(line(m, "PAPAL_CURIA_TREASURY_NOT_ENOUGH", whole(treasury))?);
    }
    Ok(if reasons.is_empty() { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

/// A check of `(curia, tag, text)` and its cost.
fn checked(m: &Memory, ctx: &Ctx, curia: usize, check: GameFunction<3>, cost: Vec<Amount>) -> GResult<Choice> {
    let tag = ctx.player.0 as usize;
    let (may, reasons) = content::checked(m, |text| Ok(check.call(m, [curia, tag, text])?))?;
    Ok(if may { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

fn council_choice(m: &Memory, ctx: &Ctx, curia: usize) -> GResult<Choice> {
    let price = out(|o| Ok(COUNCIL_PRICE.call(m, [curia, o])?))?;
    checked(m, ctx, curia, MAY_CALL_COUNCIL, vec![Amount::new(Resource::Ducats, whole(price))])
}

fn indulgence_choice(m: &Memory, ctx: &Ctx, curia: usize) -> GResult<Choice> {
    let price = out(|o| Ok(INDULGENCE_PRICE.call(m, [curia, o, ctx.player.0 as usize])?))?;
    checked(m, ctx, curia, MAY_BUY_INDULGENCE, vec![Amount::new(Resource::Ducats, whole(price))])
}

fn invest_choice(m: &Memory, ctx: &Ctx, curia: usize) -> GResult<Choice> {
    let cost = out(|o| Ok(INVEST_COST.call(m, [curia, o, ctx.player.0 as usize])?))?;
    checked(m, ctx, curia, MAY_INVEST, vec![Amount::new(Resource::PapalInfluence, whole(cost))])
}

/// The golden bulls, by their keys.
fn bulls(m: &Memory) -> GResult<Vec<(String, usize)>> {
    let head = m.ptr(m.at(GOLDEN_BULLS))?;
    if head == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for node in containers::map(m, head, containers::MOST)? {
        out.push((game_string::key(m, node + map::VALUE)?, node + BULL));
    }
    out.sort();
    Ok(out)
}

fn bull_choice(m: &Memory, ctx: &Ctx, curia: usize, bull: usize) -> GResult<Choice> {
    let base = m.ptr(ctx.world + WORLD_CURIA_BASE)?;
    let cost = match base {
        0 => Vec::new(),
        base => {
            let thousandths = i64::from(m.i32(base + CURIA_BASE_VALUE)?) * i64::from(m.i32(m.at(BULL_COST_FACTOR))?) / 1000;
            vec![Amount::new(Resource::CuriaTreasury, thousandths as f64 / 1000.0)]
        }
    };
    let tag = ctx.player.0 as usize;
    let (may, reasons) = content::checked(m, |text| Ok(MAY_PICK_BULL.call(m, [curia, bull, tag, text])?))?;
    Ok(if may { Choice::can(cost) } else { Choice::cannot(reasons, cost) })
}

fn entry(action: &str, key: String, name: String, current: bool, choice: Choice) -> ReligionOption {
    ReligionOption { action: action.into(), key, name, current, province: None, choice }
}

/// What the papacy view offers the player, while its religion has a
/// papacy: the papal actions, while the curia is active; the council,
/// indulgence and investing; and the golden bulls.
pub fn options(m: &Memory, ctx: &Ctx) -> GResult<Vec<ReligionOption>> {
    let Some((papacy, curia)) = papacy(m, ctx.country)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    if m.u8(curia + CURIA_ACTIVE)? != 0 {
        for action in actions(m, ctx, papacy)? {
            let key = game_string::key(m, action + ACTION_KEY)?;
            out.push(entry("take_papal_action", key.clone(), content::name(m, &key)?, false, action_choice(m, ctx, curia, action)?));
        }
    }
    out.push(entry(
        "call_ecumenical_council",
        "call_ecumenical_council".into(),
        content::name(m, "CURIA_CALL_ECUMENICAL_COUNCIL_TITLE")?,
        false,
        council_choice(m, ctx, curia)?,
    ));
    out.push(entry("buy_indulgence", "buy_indulgence".into(), content::name(m, "CURIA_BUY_INDULGENCE_TITLE")?, false, indulgence_choice(m, ctx, curia)?));
    out.push(entry("invest_papal_influence", "invest_papal_influence".into(), content::name(m, "papal_influence")?, false, invest_choice(m, ctx, curia)?));
    let enacted = m.ptr(curia + CURIA_BULL)?;
    for (key, bull) in bulls(m)? {
        let name = content::name(m, &key)?;
        out.push(entry("pick_golden_bull", key, name, bull == enacted, bull_choice(m, ctx, curia, bull)?));
    }
    Ok(out)
}

/// The commands the view's buttons post.
fn papal_action_text(country: Tag, index: i32) -> String {
    format!("papacy_action_command={{ country=\"{}\" id={index} }}", content::letters(country))
}

fn members_text(command: &str, country: Tag) -> String {
    format!("{command}={{ members={{ dummy=\"{}\" }} }}", content::letters(country))
}

fn invest_text(country: Tag, amount: i32) -> String {
    format!("invest_papal_influence_command={{ country=\"{}\" index={amount} }}", content::letters(country))
}

fn bull_text(country: Tag, bull: &str) -> String {
    format!("pick_golden_bull={{ members={{ dummy=\"{}\" dummy=\"{bull}\" }} }}", content::letters(country))
}

/// Takes a papal action.
pub fn take_papal_action(Session { m, ctx, commands, .. }: Session, args::TakePapalAction { action: key }: args::TakePapalAction) -> GResult<()> {
    let (papacy, curia) = own_papacy(m, ctx.country)?;
    if m.u8(curia + CURIA_ACTIVE)? == 0 {
        return Err(content::checks::not_now("the curia is not active"));
    }
    for action in actions(m, &ctx, papacy)? {
        if game_string::key(m, action + ACTION_KEY)? == key {
            let index = m.i32(action + ACTION_INDEX)?;
            return action::carry_out(commands, &action_choice(m, &ctx, curia, action)?, "take this papal action", || Ok(papal_action_text(ctx.player, index)));
        }
    }
    Err(content::checks::not_found("papal action", &key))
}

/// Calls the ecumenical council.
pub fn call_ecumenical_council(Session { m, ctx, commands, .. }: Session, _: args::CallEcumenicalCouncil) -> GResult<()> {
    let (_, curia) = own_papacy(m, ctx.country)?;
    action::carry_out(commands, &council_choice(m, &ctx, curia)?, "call the ecumenical council", || Ok(members_text("call_ecumenical_council", ctx.player)))
}

/// Buys indulgence.
pub fn buy_indulgence(Session { m, ctx, commands, .. }: Session, _: args::BuyIndulgence) -> GResult<()> {
    let (_, curia) = own_papacy(m, ctx.country)?;
    action::carry_out(commands, &indulgence_choice(m, &ctx, curia)?, "buy indulgence", || Ok(members_text("buy_indulgence", ctx.player)))
}

/// Invests papal influence in the curia, as much as the view's button does.
pub fn invest_papal_influence(Session { m, ctx, commands, .. }: Session, _: args::InvestPapalInfluence) -> GResult<()> {
    let (_, curia) = own_papacy(m, ctx.country)?;
    let amount = m.i32(m.at(INVEST_AMOUNT))?;
    action::carry_out(commands, &invest_choice(m, &ctx, curia)?, "invest papal influence", || Ok(invest_text(ctx.player, amount)))
}

/// Enacts a golden bull.
pub fn pick_golden_bull(Session { m, ctx, commands, .. }: Session, args::PickGoldenBull { bull: key }: args::PickGoldenBull) -> GResult<()> {
    let (_, curia) = own_papacy(m, ctx.country)?;
    let Some((_, bull)) = bulls(m)?.into_iter().find(|(k, _)| *k == key) else {
        return Err(content::checks::not_found("golden bull", &key));
    };
    action::carry_out(commands, &bull_choice(m, &ctx, curia, bull)?, "enact this golden bull", || Ok(bull_text(ctx.player, &key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(letters: &[u8; 3]) -> Tag {
        let mut bytes = [0u8; 8];
        bytes[..3].copy_from_slice(letters);
        Tag(u64::from_le_bytes(bytes))
    }

    #[test]
    fn the_commands_name_the_country_and_what_they_do() {
        assert_eq!(papal_action_text(tag(b"FRA"), 1), r#"papacy_action_command={ country="FRA" id=1 }"#);
        assert_eq!(members_text("call_ecumenical_council", tag(b"FRA")), r#"call_ecumenical_council={ members={ dummy="FRA" } }"#);
        assert_eq!(members_text("buy_indulgence", tag(b"FRA")), r#"buy_indulgence={ members={ dummy="FRA" } }"#);
        assert_eq!(invest_text(tag(b"FRA"), 10), r#"invest_papal_influence_command={ country="FRA" index=10 }"#);
        assert_eq!(bull_text(tag(b"PAP"), "apostolicae_servitutis"), r#"pick_golden_bull={ members={ dummy="PAP" dummy="apostolicae_servitutis" } }"#);
    }
}
