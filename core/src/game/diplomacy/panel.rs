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
        campaign::province as province_fields,
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
        fill(m, &ctx, at, *act, details)?;
    }
    verdict(m, at)?;
    send(m, &ctx, at, country, Vec::new())
}

/// The window an act opens writes what the player chooses into the action's
/// own fields, from `+0x40`, before it sends it; this writes `details` there
/// as that window does (docs/research/diplomacy.md, "The windows the panel
/// opens"). Money is in thousandths of a ducat, but where a window keeps
/// whole ducats; a province is its number, or where a window keeps it its
/// object; a country its tag.
fn fill(m: &Memory, ctx: &Ctx, at: usize, act: Act, details: &Details) -> GResult<()> {
    let need = |value: Option<u32>, what: &str| {
        value.ok_or_else(|| crate::game::Fail { status: Status::InvalidArgument, message: format!("{} needs {what}", act.name()) })
    };
    let i32_at = |offset: usize, value: i64| -> GResult<()> {
        let value = i32::try_from(value)
            .map_err(|_| crate::game::Fail { status: Status::InvalidArgument, message: format!("{value} is too large for {}", act.name()) })?;
        Ok(m.write(at + offset, &value.to_le_bytes())?)
    };
    let tag_at = |offset: usize, tag: Tag| -> GResult<()> { Ok(m.write(at + offset, &tag.0.to_le_bytes())?) };
    let province = || -> GResult<(u32, usize)> {
        let id = need(details.province, "a province")?;
        Ok((id, with_state(|s| s.geography.province_address(m, ctx, id))?))
    };
    let country = |what: &str| -> GResult<Tag> {
        let tag = details.country.ok_or_else(|| crate::game::Fail { status: Status::InvalidArgument, message: format!("{} needs {what}", act.name()) })?;
        crate::game::country(m, ctx, tag)?;
        Ok(tag)
    };
    match act {
        Act::SendGift => i32_at(FIELDS, need(details.amount, "an amount")? as i64 * 1000),
        Act::SubsidizeWar => {
            i32_at(FIELDS, need(details.amount, "an amount")? as i64 * 1000)?;
            i32_at(FIELDS + 4, details.second.unwrap_or(SUBSIDIES_DURATION) as i64)
        }
        Act::SellProvince | Act::CharterCompany => {
            let (id, _) = province()?;
            i32_at(FIELDS, need(details.second, "a price")? as i64)?;
            i32_at(FIELDS + 4, id as i64)
        }
        Act::FabricateClaim | Act::ClaimStates => Ok(m.write(at + FIELDS, &(province()?.1 as u64).to_le_bytes())?),
        Act::TransferTradePower => i32_at(FIELDS, need(details.amount, "a percent")? as i64 * 10),
        Act::ThreatenWar => {
            let (id, address) = province()?;
            i32_at(FIELDS, id as i64)?;
            tag_at(FIELDS + 4, Tag(m.u64(address + province_fields::OWNER)?))
        }
        Act::BreakAlliance | Act::TradeFavorsToBreakAlliance => tag_at(FIELDS, country("an ally")?),
        Act::TradeFavorsToReduceRelations => tag_at(FIELDS, country("a target")?),
        Act::EnforcePeace | Act::InterveneInWar => tag_at(FIELDS, country("an enemy")?),
        Act::TradeFavorsForCore => {
            let (id, address) = province()?;
            i32_at(FIELDS, id as i64)?;
            tag_at(FIELDS + 4, Tag(m.u64(address + province_fields::OWNER)?))
        }
        Act::DemandUnlawfulTerritory => i32_at(FIELDS, province()?.0 as i64),
        Act::OfferCondottieri => {
            let id = details.army.ok_or_else(|| crate::game::Fail { status: Status::InvalidArgument, message: "offer_condottieri needs an army".into() })?;
            let army = crate::game::world::own_land_army(m, ctx, id)?;
            i32_at(FIELDS, need(details.second, "a price")? as i64 * 1000)?;
            m.write(at + CONDOTTIERI_ARMY, &(army as u64).to_le_bytes())?;
            Ok(m.write(at + CONDOTTIERI_ARMY + 8, &id.to_le_bytes())?)
        }
        Act::OfferLoan => {
            i32_at(FIELDS, need(details.amount, "an amount")? as i64)?;
            i32_at(FIELDS + 4, LOAN_INTEREST)
        }
        Act::AppointCardinal => Ok(m.write(at + FIELDS, &[details.flag.unwrap_or(false) as u8])?),
        Act::TradeFavorsForTrust => {
            i32_at(FIELDS, need(details.amount, "the favors")? as i64 * 1000)?;
            i32_at(FIELDS + 4, need(details.second, "the trust")? as i64 * 1000)
        }
        _ => Ok(()),
    }
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
    ]
}
