//! The diplomacy panel: the actions it offers the player toward another
//! country, made by the game's own list maker as the panel makes them, each
//! with whether the player can send it, why not, and, where the panel shows
//! it, whether the other country would accept; and sending one of them as
//! the default diplomacy window sends it (docs/research/diplomacy.md, "The
//! diplomacy panel").
use super::{
    acts::{Act, NO_ACCEPTANCE, SCRIPTED},
    judge, letters, other, send, verdict, Kind, Ticket,
};
use crate::{
    game::{context, fail, game, with_state, Caller, Ctx, GResult, Status, Tag},
    game_string,
    memory::Memory,
    objects::{
        campaign::{country as country_fields, province as province_fields},
        diplomacy::{action, action_list, breakdown, classes},
    },
    points::Point,
    unicode::game_text,
};
use eu4_harmony_remote::choices::{Acceptance, Choice, Factor};

/// The panel's list maker: (the player's country, the other country's tag,
/// the [`action_list`]), adding each action the panel shows, made for the
/// player toward the other country.
const PANEL_ACTIONS: usize = 0x35c230;
/// The factors' list of the calling thread, which the panel's entries fill
/// to show whether the other country would accept: (the key's address, out),
/// answering the list; and emptying a list of factors, (list).
const THREAD_BREAKDOWN: usize = 0xdf320;
const BREAKDOWN_KEY: usize = 0x1fd1c38;
const BREAKDOWN_CLEAR: usize = 0x5ae8a0;
/// A call to arms, whose acceptance the panel asks with the byte at `+0x54`
/// set.
const CALL_TO_ARMS: u32 = 0x294d;
const CALL_TO_ARMS_ASKING: usize = 0x54;
/// Demanding unlawful territory, whose acceptance the panel asks province by
/// province; the framework gives none.
const UNLAWFUL_TERRITORY: u32 = 0x2e0b;
/// Where an action's own fields start, after the fields every action has.
const FIELDS: usize = 0x40;
/// A condottieri offer's army, the army's object then its id.
const CONDOTTIERI_ARMY: usize = 0x48;
/// The war subsidies window's duration as it opens, and the loan window's
/// interest.
const SUBSIDIES_DURATION: u32 = 10;
const LOAN_INTEREST: i64 = 1000;
/// The longest war subsidies the window lets the player choose, and its
/// step.
const SUBSIDIES_LONGEST: u32 = 400;
/// The favors the trade favors for trust window spends at each press of its
/// button, and the trust they buy (i32 each, the defines it reads).
const TRUST_STEP_FAVORS: usize = 0x233db7c;
const TRUST_STEP_TRUST: usize = 0x233db80;
/// The most trust a country has in another.
const FULL_TRUST: i64 = 100;
/// The most cores a province is read with.
const MOST_CORES: usize = 1000;
/// How many actions the framework's list has room for, inline: more than the
/// panel's list maker can add, so that it never grows.
const ROOM: usize = 512;
/// The most factors read.
const MOST_FACTORS: usize = 1000;

/// How a refusal for an action the panel does not offer begins.
pub(super) const NOT_OFFERED: &str = "the diplomacy panel does not offer";

/// What the player does in the panel: one of its acts, or a scripted
/// diplomatic action, by its content key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    Act(Act),
    Scripted(String),
}

/// An action the panel offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Offered {
    /// Which it is; None for a type of action the framework does not know.
    pub what: Option<What>,
    /// The key of its type in saves, such as `allianceaction`; a scripted
    /// action's content key, such as `sell_indulgence`.
    pub key: String,
    /// Its name, as the panel shows it.
    pub name: String,
    pub choice: Choice,
    /// What its description says, as its tooltip shows it after the reasons.
    pub description: Vec<String>,
    /// Whether the other country would accept it, where the panel shows it.
    pub acceptance: Option<Acceptance>,
}

/// The actions the diplomacy panel offers the player toward `country`, in its
/// order.
pub fn options(_: Caller, country: Tag) -> GResult<Vec<Offered>> {
    let m = game()?;
    let ctx = context(m)?;
    other(m, &ctx, country)?;
    let panel = Panel::list(m, &ctx, country)?;
    panel.actions.iter().map(|&at| offered(m, at)).collect()
}

/// What the window an action opens has the player choose, for the acts whose
/// window is not the default one ([`fill`]); left out for the rest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Details {
    pub province: Option<u32>,
    /// Another country: an ally, an enemy, a target.
    pub country: Option<Tag>,
    /// Ducats, favors, or a percent, as the act takes it.
    pub amount: Option<u32>,
    /// A price in ducats, a duration, or the trust gained, as the act takes
    /// it.
    pub second: Option<u32>,
    pub army: Option<u64>,
    pub flag: Option<bool>,
}

/// Sends `what` to `country`, as the panel's button and the window it opens
/// send it, with what `details` chooses there: [`Status::Rejected`] when the
/// panel does not offer it toward that country now, or the game does not let
/// the player send it, with its reasons; [`Status::InvalidArgument`] when
/// the act's window needs a choice `details` does not make.
pub fn act(_: Caller, country: Tag, what: &What, details: &Details) -> GResult<Ticket> {
    let m = game()?;
    let ctx = context(m)?;
    other(m, &ctx, country)?;
    let panel = Panel::list(m, &ctx, country)?;
    let mut found = None;
    for &at in &panel.actions {
        if identify(m, at)?.as_ref() == Some(what) {
            found = Some(at);
            break;
        }
    }
    let Some(at) = found else {
        let name = match what {
            What::Act(act) => act.name().to_string(),
            What::Scripted(key) => format!("the scripted action {key}"),
        };
        return fail(Status::Rejected, format!("{NOT_OFFERED} {name} toward {}", letters(country)));
    };
    if let What::Act(act) = what {
        fill(m, &ctx, at, country, *act, details)?;
    }
    verdict(m, at)?;
    send(m, &ctx, at, country, Vec::new())
}

/// The window an act opens writes what the player chooses into the action's
/// own fields, from `+0x40`, before it sends it; this writes `details` there
/// as that window does, refusing what the window would not let the player
/// choose (docs/research/diplomacy.md, "The windows the panel opens"). Money
/// is in thousandths of a ducat, but where a window keeps whole ducats; a
/// province is its number, or where a window keeps it its object; a country
/// its tag.
fn fill(m: &Memory, ctx: &Ctx, at: usize, recipient: Tag, act: Act, details: &Details) -> GResult<()> {
    let invalid = |message: String| crate::game::Fail { status: Status::InvalidArgument, message };
    let need = |value: Option<u32>, what: &str| value.ok_or_else(|| invalid(format!("{} needs {what}", act.name())));
    let i32_at = |offset: usize, value: i64| -> GResult<()> {
        let value = i32::try_from(value).map_err(|_| invalid(format!("{value} is too large for {}", act.name())))?;
        Ok(m.write(at + offset, &value.to_le_bytes())?)
    };
    let tag_at = |offset: usize, tag: Tag| -> GResult<()> { Ok(m.write(at + offset, &tag.0.to_le_bytes())?) };
    let province = || -> GResult<(u32, usize)> {
        let id = need(details.province, "a province")?;
        Ok((id, with_state(|s| s.geography.province_address(m, ctx, id))?))
    };
    let owner = |address: usize| -> GResult<Tag> { Ok(Tag(m.u64(address + province_fields::OWNER)?)) };
    let controller = |address: usize| -> GResult<Tag> { Ok(Tag(m.u64(address + province_fields::CONTROLLER)?)) };
    // The provinces these windows list are the other country's own.
    let recipients = || -> GResult<(u32, usize)> {
        let (id, address) = province()?;
        if owner(address)? != recipient {
            return Err(invalid(format!("province {id} is not {}'s, and {} chooses one of theirs", letters(recipient), act.name())));
        }
        Ok((id, address))
    };
    let country = |what: &str| -> GResult<Tag> {
        let tag = details.country.ok_or_else(|| invalid(format!("{} needs {what}", act.name())))?;
        crate::game::country(m, ctx, tag)?;
        Ok(tag)
    };
    let at_least_one = |value: u32, what: &str| -> GResult<u32> {
        if value == 0 {
            return Err(invalid(format!("{} needs at least 1 {what}", act.name())));
        }
        Ok(value)
    };
    // The window's money slider goes up to the player's treasury.
    let within_treasury = |ducats: u32| -> GResult<()> {
        let treasury = m.i32(ctx.country + country_fields::TREASURY)? as i64 / 1000;
        if ducats as i64 > treasury {
            return Err(invalid(format!("{ducats} ducats is more than the treasury's {treasury}, as far as {}'s window goes", act.name())));
        }
        Ok(())
    };
    match act {
        Act::SendGift => i32_at(FIELDS, at_least_one(need(details.amount, "an amount")?, "ducat")? as i64 * 1000),
        Act::SubsidizeWar => {
            let amount = at_least_one(need(details.amount, "an amount")?, "ducat")?;
            within_treasury(amount)?;
            let duration = details.second.unwrap_or(SUBSIDIES_DURATION);
            if !(SUBSIDIES_DURATION..=SUBSIDIES_LONGEST).contains(&duration) || duration % SUBSIDIES_DURATION != 0 {
                return Err(invalid(format!(
                    "a duration of {duration}: the subsidies window goes from {SUBSIDIES_DURATION} to {SUBSIDIES_LONGEST}, by {SUBSIDIES_DURATION}"
                )));
            }
            i32_at(FIELDS, amount as i64 * 1000)?;
            i32_at(FIELDS + 4, duration as i64)
        }
        Act::SellProvince => {
            let (id, address) = province()?;
            if owner(address)? != ctx.player || controller(address)? != ctx.player {
                return Err(invalid(format!("province {id} is not one the player owns and controls, which the sell province window lists")));
            }
            if m.ptr(ctx.country + country_fields::CAPITAL)? == address {
                return Err(invalid(format!("province {id} is the player's capital, which the sell province window leaves out")));
            }
            i32_at(FIELDS, need(details.second, "a price")? as i64)?;
            i32_at(FIELDS + 4, id as i64)
        }
        Act::CharterCompany => {
            let (id, _) = province()?;
            i32_at(FIELDS, need(details.second, "a price")? as i64)?;
            i32_at(FIELDS + 4, id as i64)
        }
        Act::FabricateClaim | Act::ClaimStates => Ok(m.write(at + FIELDS, &(recipients()?.1 as u64).to_le_bytes())?),
        Act::TransferTradePower => {
            let percent = at_least_one(need(details.amount, "a percent")?, "percent")?;
            if percent > 100 {
                return Err(invalid(format!("{percent} percent: a country has no more than all its trade power to transfer")));
            }
            i32_at(FIELDS, percent as i64 * 10)
        }
        Act::ThreatenWar => {
            let (id, address) = province()?;
            i32_at(FIELDS, id as i64)?;
            tag_at(FIELDS + 4, owner(address)?)
        }
        Act::BreakAlliance | Act::TradeFavorsToBreakAlliance => tag_at(FIELDS, country("an ally")?),
        Act::TradeFavorsToReduceRelations => tag_at(FIELDS, country("a target")?),
        Act::EnforcePeace | Act::InterveneInWar => tag_at(FIELDS, country("an enemy")?),
        // The window lists the provinces the other country owns and controls
        // with a core of the player's, and sends the core's holder with one.
        Act::TradeFavorsForCore => {
            let (id, address) = recipients()?;
            if controller(address)? != recipient {
                return Err(invalid(format!("province {id} is not controlled by {}, and the window lists only those it controls", letters(recipient))));
            }
            let cores = m.ptr(address + province_fields::CORES)?;
            let count = crate::game::span(cores, m.ptr(address + province_fields::CORES + 8)?, 8, MOST_CORES)?;
            let mut held = false;
            for i in 0..count {
                held |= Tag(m.u64(cores + i * 8)?) == ctx.player;
            }
            if !held {
                return Err(invalid(format!("province {id} is no core of the player's")));
            }
            i32_at(FIELDS, id as i64)?;
            tag_at(FIELDS + 4, ctx.player)
        }
        Act::DemandUnlawfulTerritory => i32_at(FIELDS, province()?.0 as i64),
        Act::OfferCondottieri => {
            let id = details.army.ok_or_else(|| invalid("offer_condottieri needs an army".into()))?;
            let army = crate::game::world::own_land_army(m, ctx, id)?;
            i32_at(FIELDS, need(details.second, "a price")? as i64 * 1000)?;
            m.write(at + CONDOTTIERI_ARMY, &(army as u64).to_le_bytes())?;
            Ok(m.write(at + CONDOTTIERI_ARMY + 8, &id.to_le_bytes())?)
        }
        Act::OfferLoan => {
            let amount = at_least_one(need(details.amount, "an amount")?, "ducat")?;
            within_treasury(amount)?;
            i32_at(FIELDS, amount as i64)?;
            i32_at(FIELDS + 4, LOAN_INTEREST)
        }
        Act::AppointCardinal => Ok(m.write(at + FIELDS, &[details.flag.unwrap_or(false) as u8])?),
        // Each press of the window's button spends a step of favors for a
        // step of trust, while the other country owes the favors and its
        // trust is not yet full.
        Act::TradeFavorsForTrust => {
            let favors = at_least_one(need(details.amount, "the favors")?, "favor")? as i64;
            let (step, trust_step) = (m.i32(m.at(TRUST_STEP_FAVORS))? as i64, m.i32(m.at(TRUST_STEP_TRUST))? as i64);
            if step <= 0 || trust_step <= 0 {
                return Err(format!("the favors for trust's step is {step} favors for {trust_step} trust").into());
            }
            if favors % step != 0 {
                return Err(invalid(format!("{favors} favors: the window spends them {step} at a time")));
            }
            let other = crate::game::country(m, ctx, recipient)?;
            let record = m.ptr(other + country_fields::RELATIONS)? + ctx.player.index() as usize * country_fields::RELATION_SIZE;
            let owed = m.i32(record + country_fields::RELATION_FAVORS)? as i64 / 1000;
            let trust = m.u8(record + country_fields::RELATION_TRUST)? as i64;
            let most = most_trust_steps(owed, trust, step, trust_step);
            if favors / step > most {
                return Err(invalid(format!(
                    "{favors} favors: {} owes {owed} and trusts the player {trust}, so the window lets the player spend at most {}",
                    letters(recipient),
                    most * step
                )));
            }
            let bought = favors / step * trust_step;
            if let Some(trust) = details.second.filter(|&t| t as i64 != bought) {
                return Err(invalid(format!("{trust} trust: {favors} favors buy {bought}, {trust_step} for each {step}")));
            }
            i32_at(FIELDS, favors * 1000)?;
            i32_at(FIELDS + 4, bought)
        }
        _ => Ok(()),
    }
}

/// How many presses of the trade favors for trust window's button the
/// player has, `owed` favors owed and `trust` trust had, each press spending
/// `step` favors for `trust_step` trust: the window stops at the favors
/// owed, and at the press that fills the trust.
fn most_trust_steps(owed: i64, trust: i64, step: i64, trust_step: i64) -> i64 {
    (owed / step).min((trust_step - trust + FULL_TRUST) / trust_step).max(0)
}

/// Which action the one at `at` is.
fn identify(m: &Memory, at: usize) -> GResult<Option<What>> {
    let kind = Kind::of(m, at)?;
    if kind.token == SCRIPTED {
        return Ok(Some(What::Scripted(name_key(m, at)?)));
    }
    Ok(Act::of(kind.token, kind.offer).map(What::Act))
}

/// What the panel says of the action at `at`.
fn offered(m: &Memory, at: usize) -> GResult<Offered> {
    let kind = Kind::of(m, at)?;
    let what = identify(m, at)?;
    let name_key = name_key(m, at)?;
    let (key, title) = match &what {
        Some(What::Scripted(key)) => (key.clone(), format!("{key}_title")),
        Some(What::Act(act)) => (act.key().to_string(), format!("{name_key}TITLE")),
        None => (format!("{:#x}", kind.token), format!("{name_key}TITLE")),
    };
    let judged = judge(m, at)?;
    let choice = if judged.can { Choice::can(Vec::new()) } else { Choice::cannot(judged.reasons, Vec::new()) };
    Ok(Offered { what, key, name: localized(m, &title)?, choice, description: judged.description, acceptance: acceptance(m, at, kind.token)? })
}

/// The localisation key of the action's name, as its method writes it; a
/// scripted action's content key.
fn name_key(m: &Memory, at: usize) -> GResult<String> {
    let name = m.vslot(at, action::NAME)?;
    Ok(game_text::to_utf8(&game_string::written(m, |s| m.call(name, &[at, s]))?.1))
}

fn localized(m: &Memory, key: &str) -> GResult<String> {
    let key = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
    Ok(game_text::plain(&game_string::localized(m, key.as_ptr() as usize)?).trim().to_string())
}

/// Whether the other country would accept the action at `at`, as the panel's
/// entry shows it: for an action that needs an answer, but those whose
/// acceptance the panel never shows, the factors the other country's AI
/// weighs, their sum, and whether it accepts (a sum of at least 1).
fn acceptance(m: &Memory, at: usize, token: u32) -> GResult<Option<Acceptance>> {
    if NO_ACCEPTANCE.contains(&token) || token == UNLAWFUL_TERRITORY || m.call(m.vslot(at, action::NEEDS_ANSWER)?, &[at])? as u8 == 0 {
        return Ok(None);
    }
    let mut out = [0u64; 4];
    let factors = m.call(m.at(THREAD_BREAKDOWN), &[m.at(BREAKDOWN_KEY), out.as_mut_ptr() as usize])?;
    if factors == 0 {
        return Err("the thread has no list of factors".to_string().into());
    }
    let clear = || m.call(m.at(BREAKDOWN_CLEAR), &[factors]);
    clear()?;
    if token == CALL_TO_ARMS {
        m.write(at + CALL_TO_ARMS_ASKING, &[1])?;
    }
    let total = m.call(m.vslot(at, action::ACCEPTANCE)?, &[at, factors, 1]).map(|t| t as i32);
    if token == CALL_TO_ARMS {
        m.write(at + CALL_TO_ARMS_ASKING, &[0])?;
    }
    let read = total.map_err(crate::game::Fail::from).and_then(|total| Ok((total, read_factors(m, factors)?)));
    clear()?;
    let (total, factors) = read?;
    Ok(Some(Acceptance { accepts: total >= 1, total: total as f64, factors }))
}

fn read_factors(m: &Memory, factors: usize) -> GResult<Vec<Factor>> {
    let (begin, end) = (m.ptr(factors)?, m.ptr(factors + 8)?);
    let count = crate::game::span(begin, end, breakdown::SIZE, MOST_FACTORS)?;
    (0..count)
        .map(|i| {
            let item = begin + i * breakdown::SIZE;
            let text = game_text::plain(&game_string::read(m, item + breakdown::TEXT)?).trim().to_string();
            Ok(Factor { text, value: m.i32(item + breakdown::VALUE)? as f64 })
        })
        .collect()
}

/// The actions the panel's list maker made in a list of the framework's;
/// each destroyed, as the panel destroys them, when dropped.
struct Panel<'a> {
    m: &'a Memory,
    list: Box<[u64]>,
    actions: Vec<usize>,
}

impl<'a> Panel<'a> {
    fn list(m: &'a Memory, ctx: &Ctx, country: Tag) -> GResult<Self> {
        let mut list = vec![0u64; action_list::INLINE / 8 + ROOM].into_boxed_slice();
        let at = list.as_ptr() as usize;
        list[0] = m.at(classes::ACTION_LIST) as u64;
        list[action_list::ITEMS / 8] = (at + action_list::INLINE) as u64;
        list[action_list::CAPACITY / 8] = ROOM as u64;
        let mut panel = Self { m, list, actions: Vec::new() };
        m.call(m.at(PANEL_ACTIONS), &[ctx.country, country.0 as usize, at])?;
        let (items, count) = panel.items()?;
        for i in 0..count {
            let action = m.ptr(items + i * 8)?;
            if action != 0 {
                panel.actions.push(action);
            }
        }
        Ok(panel)
    }

    /// Where the list's actions are, and how many: inline, as it has room.
    fn items(&self) -> GResult<(usize, usize)> {
        let at = self.list.as_ptr() as usize;
        let (items, count) = (self.list[action_list::ITEMS / 8] as usize, (self.list[action_list::COUNT / 8] >> 32) as usize);
        if items != at + action_list::INLINE || count > ROOM {
            return Err("the diplomacy panel's list of actions grew".to_string().into());
        }
        Ok((items, count))
    }
}

impl Drop for Panel<'_> {
    fn drop(&mut self) {
        let Ok((items, count)) = self.items() else { return };
        for i in 0..count {
            let destroyed =
                self.m.ptr(items + i * 8).and_then(|a| if a == 0 { Ok(0) } else { self.m.vslot(a, action::DESTROY).and_then(|d| self.m.call(d, &[a, 1])) });
            if let Err(e) = destroyed {
                log!("diplomacy: destroying the panel's action: {e}");
            }
        }
    }
}

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(PANEL_ACTIONS, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 56 41 57 48 83 ec 40 48 8b c2 49 8b e8", "diplomacy panel's actions"),
        // The panel calls it with the player's country, the other country's
        // tag and its list.
        Point::keep(
            0xf0bf50usize,
            "48 8b 05 79 19 43 01 48 c1 eb 20 48 0f bf cb 48 8b 80 18 01 00 00 4c 8d 45 d0 48 8b d7 48 8b 0c c8 e8 ba 02 45 ff",
            "diplomacy panel's call",
        ),
        Point::keep(THREAD_BREAKDOWN, "40 53 55 57 41 54 41 56 48 83 ec 30 4c 8b f1 48 8b da", "thread's factors"),
        Point::keep(0xef2a0ausize, "48 8d 0d 27 f2 0d 01 e8 0a c9 1e ff 48 8b f8 48 8b c8 e8 7f be 6b ff", "panel's factors").uses(&[BREAKDOWN_KEY]),
        Point::keep(BREAKDOWN_CLEAR, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48 8b 71 08 48 8b f9 48 8b 19", "factors emptied"),
        // The trade favors for trust window's button reading the favors and
        // the trust of a press.
        Point::keep(0xeebc39usize, "8b 1d 41 1f 45 01 48 89 6c 24 38 48 8b a9 c0 01 00 00 48 89 7c 24 40 8b 3d 26 1f 45 01", "favors for trust's step")
            .uses(&[TRUST_STEP_TRUST, TRUST_STEP_FAVORS]),
    ]
}

#[cfg(test)]
mod tests {
    use super::most_trust_steps;

    #[test]
    fn the_favors_for_trust_window_stops_at_the_favors_owed_and_full_trust() {
        // 10 favors for 5 trust a press, as the game's defines have it.
        assert_eq!(most_trust_steps(50, 50, 10, 5), 5);
        assert_eq!(most_trust_steps(200, 50, 10, 5), 11);
        assert_eq!(most_trust_steps(200, 98, 10, 5), 1);
        assert_eq!(most_trust_steps(200, 100, 10, 5), 1);
        assert_eq!(most_trust_steps(9, 0, 10, 5), 0);
        assert_eq!(most_trust_steps(-20, 0, 10, 5), 0);
    }
}
