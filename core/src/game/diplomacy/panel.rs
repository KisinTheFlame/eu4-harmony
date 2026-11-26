//! The diplomacy panel: the actions it offers the player toward another
//! country, made by the game's own list maker as the panel makes them, each
//! with whether the player can send it, why not, and, where the panel shows
//! it, whether the other country would accept; and sending one of them as
//! the default diplomacy window sends it (docs/game/diplomacy/diplomacy.md, "The
//! diplomacy panel").
use super::{
    acts::{Act, NO_ACCEPTANCE, SCRIPTED},
    judge, letters, other, send, verdict, Kind, Ticket,
};
use crate::{game::{countries, fail, with_state, Ctx, GResult, Session, Status, Tag}, native::game_string, native::memory::Memory, native::objects::{
        campaign::{country as country_fields, province as province_fields},
        diplomacy::{action, action_list, breakdown, classes},
    }, native::points::{GameFunction, Point}, unicode::game_text};
use eu4_harmony_remote::{
    choices::{Acceptance, Choice, Factor},
    diplomacy::{self as remote, args, Options},
    ArmyId, CountryTag, ProvinceId,
};

/// The panel's list maker: (the player's country, the other country's tag,
/// the [`action_list`]), adding each action the panel shows, made for the
/// player toward the other country.
const PANEL_ACTIONS: GameFunction<3> = GameFunction::at(0x35c230);
/// The factors' list of the calling thread, which the panel's entries fill
/// to show whether the other country would accept: (the key's address, out),
/// answering the list; and emptying a list of factors, (list).
const THREAD_BREAKDOWN: GameFunction<2> = GameFunction::at(0xdf320);
const BREAKDOWN_KEY: usize = 0x1fd1c38;
const BREAKDOWN_CLEAR: GameFunction<1> = GameFunction::at(0x5ae8a0);
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
/// order, each named by the capability that sends it.
pub fn options(Session { m, ctx, .. }: Session, args::Options { country: asked }: args::Options) -> GResult<Options> {
    let country = countries::named(m, &ctx, asked)?;
    other(m, &ctx, country)?;
    let panel = Panel::list(m, &ctx, country)?;
    let actions = panel.actions.iter().map(|&at| offered(m, at).map(listed)).collect::<GResult<_>>()?;
    Ok(Options { country: asked, actions })
}

/// An action the panel offers, named by the capability that sends it.
fn listed(o: Offered) -> remote::Offered {
    let action = match &o.what {
        Some(What::Act(act)) => act.name(),
        Some(What::Scripted(_)) => "scripted_action",
        None => "unknown",
    };
    remote::Offered { action: action.into(), key: o.key, name: o.name, choice: o.choice, description: o.description, acceptance: o.acceptance }
}

/// A diplomacy panel action paired with the exact typed choices its window
/// requires, eliminating untyped option bags across the catalogue seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelOrder<Country = Tag> {
    /// Any panel action whose window takes no extra choice beyond the recipient.
    Simple(Act),
    SendGift { amount: u32 },
    SubsidizeWar { amount: u32, duration: Option<u32> },
    SellProvince { province: ProvinceId, price: u32 },
    CharterCompany { province: ProvinceId, price: u32 },
    FabricateClaim { province: ProvinceId },
    ClaimStates { province: ProvinceId },
    TransferTradePower { percent: u32 },
    ThreatenWar { province: ProvinceId },
    BreakAlliance { ally: Country },
    TradeFavorsToBreakAlliance { ally: Country },
    TradeFavorsToReduceRelations { target: Country },
    EnforcePeace { enemy: Country },
    InterveneInWar { enemy: Country },
    TradeFavorsForCore { province: ProvinceId },
    DemandUnlawfulTerritory { province: ProvinceId },
    OfferCondottieri { army: ArmyId, price: u32 },
    OfferLoan { amount: u32 },
    SupportRebels { rebels: String },
    AppointCardinal { pay: bool },
    TradeFavorsForTrust { favors: u32, trust: Option<u32> },
}

impl<Country> PanelOrder<Country> {
    /// The underlying panel action this order sends.
    pub fn act(&self) -> Act {
        match self {
            Self::Simple(act) => *act,
            Self::SendGift { .. } => Act::SendGift,
            Self::SubsidizeWar { .. } => Act::SubsidizeWar,
            Self::SellProvince { .. } => Act::SellProvince,
            Self::CharterCompany { .. } => Act::CharterCompany,
            Self::FabricateClaim { .. } => Act::FabricateClaim,
            Self::ClaimStates { .. } => Act::ClaimStates,
            Self::TransferTradePower { .. } => Act::TransferTradePower,
            Self::ThreatenWar { .. } => Act::ThreatenWar,
            Self::BreakAlliance { .. } => Act::BreakAlliance,
            Self::TradeFavorsToBreakAlliance { .. } => Act::TradeFavorsToBreakAlliance,
            Self::TradeFavorsToReduceRelations { .. } => Act::TradeFavorsToReduceRelations,
            Self::EnforcePeace { .. } => Act::EnforcePeace,
            Self::InterveneInWar { .. } => Act::InterveneInWar,
            Self::TradeFavorsForCore { .. } => Act::TradeFavorsForCore,
            Self::DemandUnlawfulTerritory { .. } => Act::DemandUnlawfulTerritory,
            Self::OfferCondottieri { .. } => Act::OfferCondottieri,
            Self::OfferLoan { .. } => Act::OfferLoan,
            Self::SupportRebels { .. } => Act::SupportRebels,
            Self::AppointCardinal { .. } => Act::AppointCardinal,
            Self::TradeFavorsForTrust { .. } => Act::TradeFavorsForTrust,
        }
    }

    /// Maps country tags carried in the order into resolved game tags.
    pub fn countries<Found, E>(self, mut find: impl FnMut(Country) -> Result<Found, E>) -> Result<PanelOrder<Found>, E> {
        Ok(match self {
            Self::Simple(act) => PanelOrder::Simple(act),
            Self::SendGift { amount } => PanelOrder::SendGift { amount },
            Self::SubsidizeWar { amount, duration } => PanelOrder::SubsidizeWar { amount, duration },
            Self::SellProvince { province, price } => PanelOrder::SellProvince { province, price },
            Self::CharterCompany { province, price } => PanelOrder::CharterCompany { province, price },
            Self::FabricateClaim { province } => PanelOrder::FabricateClaim { province },
            Self::ClaimStates { province } => PanelOrder::ClaimStates { province },
            Self::TransferTradePower { percent } => PanelOrder::TransferTradePower { percent },
            Self::ThreatenWar { province } => PanelOrder::ThreatenWar { province },
            Self::BreakAlliance { ally } => PanelOrder::BreakAlliance { ally: find(ally)? },
            Self::TradeFavorsToBreakAlliance { ally } => PanelOrder::TradeFavorsToBreakAlliance { ally: find(ally)? },
            Self::TradeFavorsToReduceRelations { target } => PanelOrder::TradeFavorsToReduceRelations { target: find(target)? },
            Self::EnforcePeace { enemy } => PanelOrder::EnforcePeace { enemy: find(enemy)? },
            Self::InterveneInWar { enemy } => PanelOrder::InterveneInWar { enemy: find(enemy)? },
            Self::TradeFavorsForCore { province } => PanelOrder::TradeFavorsForCore { province },
            Self::DemandUnlawfulTerritory { province } => PanelOrder::DemandUnlawfulTerritory { province },
            Self::OfferCondottieri { army, price } => PanelOrder::OfferCondottieri { army, price },
            Self::OfferLoan { amount } => PanelOrder::OfferLoan { amount },
            Self::SupportRebels { rebels } => PanelOrder::SupportRebels { rebels },
            Self::AppointCardinal { pay } => PanelOrder::AppointCardinal { pay },
            Self::TradeFavorsForTrust { favors, trust } => PanelOrder::TradeFavorsForTrust { favors, trust },
        })
    }
}

/// Sends `order` to the country a request names, each country found in the
/// game, as [`sent`].
pub fn act(session: Session, country: CountryTag, order: PanelOrder<CountryTag>) -> GResult<Ticket> {
    let found = |tag| countries::named(session.m, &session.ctx, tag);
    let (country, order) = (found(country)?, order.countries(found)?);
    sent(session, country, &What::Act(order.act()), Some(&order))
}

/// Sends the scripted action of this content key to the country a request
/// names, as [`sent`].
pub fn scripted(session: Session, args::ScriptedAction { country, action }: args::ScriptedAction) -> GResult<Ticket> {
    let country = countries::named(session.m, &session.ctx, country)?;
    sent(session, country, &What::Scripted(action), None)
}

/// Sends `what` to `country`, as the panel's button and the window it opens
/// send it, with `order`'s typed window choices: [`Status::Rejected`] when
/// the panel does not offer it toward that country now, or the game does not
/// let the player send it, with its reasons; [`Status::InvalidArgument`] when
/// a value violates the act's window bounds.
pub(super) fn sent(Session { m, ctx, commands, .. }: Session, country: Tag, what: &What, order: Option<&PanelOrder>) -> GResult<Ticket> {
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
    if let Some(order) = order {
        fill(m, &ctx, at, country, order)?;
    }
    verdict(m, at)?;
    send(commands, m, &ctx, at, country, Vec::new())
}

/// Pure validation of a war-subsidy duration against the window's bounds.
fn check_subsidy_duration(duration: Option<u32>) -> GResult<u32> {
    let duration = duration.unwrap_or(SUBSIDIES_DURATION);
    if !(SUBSIDIES_DURATION..=SUBSIDIES_LONGEST).contains(&duration) || !duration.is_multiple_of(SUBSIDIES_DURATION) {
        return fail(
            Status::InvalidArgument,
            format!("a duration of {duration}: the subsidies window goes from {SUBSIDIES_DURATION} to {SUBSIDIES_LONGEST}, by {SUBSIDIES_DURATION}"),
        );
    }
    Ok(duration)
}

/// Pure validation of a trade-power transfer percentage (1..=100).
fn check_trade_power_percent(percent: u32) -> GResult<u32> {
    if percent == 0 {
        return fail(Status::InvalidArgument, "transfer_trade_power needs at least 1 percent");
    }
    if percent > 100 {
        return fail(Status::InvalidArgument, format!("{percent} percent: a country has no more than all its trade power to transfer"));
    }
    Ok(percent)
}

/// Pure validation of favors spent for trust against step sizes, owed favors,
/// current trust, and an optional caller-supplied trust check.
fn check_trust_favors(recipient: &str, favors: u32, trust_requested: Option<u32>, owed: i64, trust: i64, step: i64, trust_step: i64) -> GResult<i64> {
    if favors == 0 {
        return fail(Status::InvalidArgument, "trade_favors_for_trust needs at least 1 favor");
    }
    if step <= 0 || trust_step <= 0 {
        return Err(format!("the favors for trust's step is {step} favors for {trust_step} trust").into());
    }
    let favors = favors as i64;
    if favors % step != 0 {
        return fail(Status::InvalidArgument, format!("{favors} favors: the window spends them {step} at a time"));
    }
    let most = most_trust_steps(owed, trust, step, trust_step);
    if favors / step > most {
        return fail(
            Status::InvalidArgument,
            format!("{favors} favors: {recipient} owes {owed} and trusts the player {trust}, so the window lets the player spend at most {}", most * step),
        );
    }
    let bought = favors / step * trust_step;
    if let Some(expected) = trust_requested.filter(|&t| t as i64 != bought) {
        return fail(Status::InvalidArgument, format!("{expected} trust: {favors} favors buy {bought}, {trust_step} for each {step}"));
    }
    Ok(bought)
}

/// The window an act opens writes what the player chooses into the action's
/// own fields, from `+0x40`, before it sends it; this writes `order` there
/// as that window does, refusing what the window would not let the player
/// choose (docs/game/diplomacy/diplomacy.md, "The windows the panel opens"). Money
/// is in thousandths of a ducat, but where a window keeps whole ducats; a
/// province is its number, or where a window keeps it its object; a country
/// its tag; a rebel faction its object.
fn fill(m: &Memory, ctx: &Ctx, at: usize, recipient: Tag, order: &PanelOrder) -> GResult<()> {
    let act = order.act();
    let invalid = |message: String| crate::game::Fail { status: Status::InvalidArgument, message };
    let i32_at = |offset: usize, value: i64| -> GResult<()> {
        let value = i32::try_from(value).map_err(|_| invalid(format!("{value} is too large for {}", act.name())))?;
        Ok(m.write(at + offset, &value.to_le_bytes())?)
    };
    let tag_at = |offset: usize, tag: Tag| -> GResult<()> { Ok(m.write(at + offset, &tag.0.to_le_bytes())?) };
    let province = |id: ProvinceId| -> GResult<(u32, usize)> { Ok((id.0, with_state(|s| s.geography.province_address(m, ctx, id.0))?)) };
    let owner = |address: usize| -> GResult<Tag> { Ok(Tag(m.u64(address + province_fields::OWNER)?)) };
    let controller = |address: usize| -> GResult<Tag> { Ok(Tag(m.u64(address + province_fields::CONTROLLER)?)) };
    // The provinces these windows list are the other country's own.
    let recipients = |id: ProvinceId| -> GResult<(u32, usize)> {
        let (id, address) = province(id)?;
        if owner(address)? != recipient {
            return Err(invalid(format!("province {id} is not {}'s, and {} chooses one of theirs", letters(recipient), act.name())));
        }
        Ok((id, address))
    };
    let country = |tag: Tag| -> GResult<Tag> {
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
    match order {
        PanelOrder::SendGift { amount } => i32_at(FIELDS, at_least_one(*amount, "ducat")? as i64 * 1000),
        PanelOrder::SubsidizeWar { amount, duration } => {
            let amount = at_least_one(*amount, "ducat")?;
            within_treasury(amount)?;
            let duration = check_subsidy_duration(*duration)?;
            i32_at(FIELDS, amount as i64 * 1000)?;
            i32_at(FIELDS + 4, duration as i64)
        }
        PanelOrder::SellProvince { province: id, price } => {
            let (id, address) = province(*id)?;
            if owner(address)? != ctx.player || controller(address)? != ctx.player {
                return Err(invalid(format!("province {id} is not one the player owns and controls, which the sell province window lists")));
            }
            if m.ptr(ctx.country + country_fields::CAPITAL)? == address {
                return Err(invalid(format!("province {id} is the player's capital, which the sell province window leaves out")));
            }
            i32_at(FIELDS, *price as i64)?;
            i32_at(FIELDS + 4, id as i64)
        }
        PanelOrder::CharterCompany { province: id, price } => {
            let (id, _) = province(*id)?;
            i32_at(FIELDS, *price as i64)?;
            i32_at(FIELDS + 4, id as i64)
        }
        PanelOrder::FabricateClaim { province: id } | PanelOrder::ClaimStates { province: id } => Ok(m.write(at + FIELDS, &(recipients(*id)?.1 as u64).to_le_bytes())?),
        PanelOrder::TransferTradePower { percent } => i32_at(FIELDS, check_trade_power_percent(*percent)? as i64 * 10),
        PanelOrder::ThreatenWar { province: id } => {
            let (id, address) = province(*id)?;
            i32_at(FIELDS, id as i64)?;
            tag_at(FIELDS + 4, owner(address)?)
        }
        PanelOrder::BreakAlliance { ally } | PanelOrder::TradeFavorsToBreakAlliance { ally } => tag_at(FIELDS, country(*ally)?),
        PanelOrder::TradeFavorsToReduceRelations { target } => tag_at(FIELDS, country(*target)?),
        PanelOrder::EnforcePeace { enemy } | PanelOrder::InterveneInWar { enemy } => tag_at(FIELDS, country(*enemy)?),
        // The window lists the provinces the other country owns and controls
        // with a core of the player's, and sends the core's holder with one.
        PanelOrder::TradeFavorsForCore { province: id } => {
            let (id, address) = recipients(*id)?;
            if controller(address)? != recipient {
                return Err(invalid(format!("province {id} is not controlled by {}, and the window lists only those it controls", letters(recipient))));
            }
            let cores = m.ptr(address + province_fields::CORES)?;
            let count = crate::native::containers::span(cores, m.ptr(address + province_fields::CORES + 8)?, 8, MOST_CORES)?;
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
        PanelOrder::DemandUnlawfulTerritory { province: id } => i32_at(FIELDS, province(*id)?.0 as i64),
        PanelOrder::OfferCondottieri { army: id, price } => {
            let army = crate::game::army::own_land(m, ctx, id.0)?;
            i32_at(FIELDS, *price as i64 * 1000)?;
            m.write(at + CONDOTTIERI_ARMY, &(army as u64).to_le_bytes())?;
            Ok(m.write(at + CONDOTTIERI_ARMY + 8, &id.0.to_le_bytes())?)
        }
        PanelOrder::OfferLoan { amount } => {
            let amount = at_least_one(*amount, "ducat")?;
            within_treasury(amount)?;
            i32_at(FIELDS, amount as i64)?;
            i32_at(FIELDS + 4, LOAN_INTEREST)
        }
        PanelOrder::SupportRebels { rebels } => {
            let at_country = crate::game::country(m, ctx, recipient)?;
            let Some(faction) = crate::game::countries::rebel_faction(m, at_country, rebels)? else {
                return fail(Status::Rejected, format!("{} has no {rebels} faction", letters(recipient)));
            };
            Ok(m.write(at + FIELDS, &(faction as u64).to_le_bytes())?)
        }
        PanelOrder::AppointCardinal { pay } => Ok(m.write(at + FIELDS, &[*pay as u8])?),
        // Each press of the window's button spends a step of favors for a
        // step of trust, while the other country owes the favors and its
        // trust is not yet full.
        PanelOrder::TradeFavorsForTrust { favors, trust } => {
            let (step, trust_step) = (m.i32(m.at(TRUST_STEP_FAVORS))? as i64, m.i32(m.at(TRUST_STEP_TRUST))? as i64);
            let other = crate::game::country(m, ctx, recipient)?;
            let record = m.ptr(other + country_fields::RELATIONS)? + ctx.player.index() as usize * country_fields::RELATION_SIZE;
            let owed = m.i32(record + country_fields::RELATION_FAVORS)? as i64 / 1000;
            let current_trust = m.u8(record + country_fields::RELATION_TRUST)? as i64;
            let bought = check_trust_favors(&letters(recipient), *favors, *trust, owed, current_trust, step, trust_step)?;
            i32_at(FIELDS, *favors as i64 * 1000)?;
            i32_at(FIELDS + 4, bought)
        }
        // Its window has the player choose the ships and their price, which
        // the framework cannot choose yet, and the game takes it up with no
        // ships.
        PanelOrder::Simple(Act::SellShips) => fail(
            Status::Unavailable,
            "sell_ships is not sent: its window has the player choose the ships, which the framework cannot choose yet, and the game would take it up without",
        ),
        PanelOrder::Simple(_) => Ok(()),
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
    let factors = THREAD_BREAKDOWN.call(m, [m.at(BREAKDOWN_KEY), out.as_mut_ptr() as usize])?;
    if factors == 0 {
        return Err("the thread has no list of factors".to_string().into());
    }
    let clear = || BREAKDOWN_CLEAR.call(m, [factors]);
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
    let count = crate::native::containers::span(begin, end, breakdown::SIZE, MOST_FACTORS)?;
    (0..count)
        .map(|i| {
            let item = begin + i * breakdown::SIZE;
            let text = game_string::shown(m, item + breakdown::TEXT)?.trim().to_string();
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
        PANEL_ACTIONS.call(m, [ctx.country, country.0 as usize, at])?;
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
    use crate::game::As;
    use super::*;
    use crate::{
        game::buffers::{byz, tur, wal},
        game::{
            buffers::{self, Image, Object},
            Caller, Fail, Session,
        },
    };
    use std::cell::RefCell;

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

    /// What the panel's actions are, as the game's functions below answer for
    /// them: each action's address and its type's token, the actions the
    /// panel destroyed, and the reason the game's check gives for every one.
    #[derive(Default)]
    struct Script {
        actions: Vec<(usize, u32)>,
        destroyed: Vec<usize>,
    }

    thread_local! {
        static SCRIPT: RefCell<Script> = RefCell::default();
    }

    /// The panel's list maker: (the player's country, the other's tag, the
    /// list), the list's room inline, the actions added there.
    extern "C" fn list_maker(_: usize, _: usize, list: usize) -> usize {
        SCRIPT.with(|s| {
            let actions = &s.borrow().actions;
            let items = unsafe { *((list + action_list::ITEMS) as *const usize) };
            for (i, (at, _)) in actions.iter().enumerate() {
                unsafe { *((items + i * 8) as *mut usize) = *at };
            }
            unsafe { *((list + action_list::COUNT) as *mut u32) = actions.len() as u32 };
        });
        0
    }

    /// An action's type: its token.
    extern "C" fn kind(at: usize) -> usize {
        SCRIPT.with(|s| s.borrow().actions.iter().find(|(a, _)| *a == at).map_or(0, |(_, token)| *token as usize))
    }

    /// The panel's check, which offers every action.
    extern "C" fn offered(_: usize, _: usize, _: usize) -> usize {
        1
    }

    /// The window's check, which refuses every action, its reason a line of
    /// the game's, as it appends it to the `std::string` given.
    extern "C" fn refused(_: usize, reasons: usize, _: usize) -> usize {
        if reasons != 0 {
            let said = b"- no money";
            unsafe {
                std::ptr::copy_nonoverlapping(said.as_ptr(), reasons as *mut u8, said.len());
                *((reasons + game_string::SIZE) as *mut usize) = said.len();
            }
        }
        0
    }

    /// An action's destructor: (the action, 1, freeing it).
    extern "C" fn destroy(at: usize, _: usize) -> usize {
        SCRIPT.with(|s| s.borrow_mut().destroyed.push(at));
        0
    }

    /// A `std::string`'s destructor, for one whose text is inline.
    extern "C" fn destroy_string(_: usize) -> usize {
        0
    }

    /// The Ottomans' session, with 500 ducats, toward Byzantium, which owes
    /// them 50 favors and trusts them 50; the diplomacy panel offers the
    /// player `acts` toward any country, and the game lets the player send
    /// none of them, saying the player has no money.
    struct Panel {
        _image: Image,
        session: Session,
        actions: Vec<(Act, Object)>,
        _objects: Vec<Object>,
    }

    impl Panel {
        fn offering(acts: &[Act]) -> Self {
            let image = Image::new();
            let m = image.memory();
            image.function(PANEL_ACTIONS.rva(), list_maker as *const ()).function(game_string::DESTROY_AT.rva(), destroy_string as *const ());
            image.i32(TRUST_STEP_FAVORS, 10).i32(TRUST_STEP_TRUST, 5);
            let player = Object::country(tur(), country_fields::TREASURY + 4).i32(country_fields::TREASURY, 500_000);
            let record = tur().index() as usize * country_fields::RELATION_SIZE;
            let relations = Object::new(record + country_fields::RELATION_SIZE)
                .u8(record + country_fields::RELATION_TRUST, 50)
                .i32(record + country_fields::RELATION_FAVORS, 50_000);
            let other = Object::country(byz(), country_fields::RELATIONS + 8).points_to(country_fields::RELATIONS, relations);
            let countries = Object::countries(&[&player, &other]);
            let manager = image.country_manager(&countries);
            let vtable = Object::new(action::NAME + 8)
                .u64(action::DESTROY, destroy as *const () as u64)
                .u64(action::KIND, kind as *const () as u64)
                .u64(action::OFFERED, offered as *const () as u64)
                .u64(action::ALLOWED, refused as *const () as u64);
            let actions: Vec<(Act, Object)> =
                acts.iter().map(|&act| (act, Object::new(0x80).u64(0, vtable.at() as u64).u8(action::OFFER, act.offer() as u8))).collect();
            SCRIPT.with(|s| *s.borrow_mut() = Script { actions: actions.iter().map(|(act, o)| (o.at(), act.token())).collect(), destroyed: Vec::new() });
            let session = buffers::session(m, 0, tur(), player.at(), countries.at(), Caller::Remote(As::Player));
            Self { _image: image, session, actions, _objects: vec![player, other, countries, vtable, manager] }
        }

        /// Sends `order` to Byzantium, as the capability does.
        fn order(&self, order: PanelOrder) -> GResult<Ticket> {
            order_toward(self, byz(), order)
        }

        /// The action of `act`'s field at `offset`.
        fn field(&self, act: Act, offset: usize) -> i32 {
            self.actions.iter().find(|(a, _)| *a == act).unwrap().1.read_i32(offset)
        }
    }

    fn order_toward(panel: &Panel, country: Tag, order: PanelOrder) -> GResult<Ticket> {
        super::sent(panel.session, country, &What::Act(order.act()), Some(&order))
    }

    /// The failure's status and message.
    fn failed(result: GResult<Ticket>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was sent") };
        (status, message)
    }

    fn invalid(message: &str) -> (Status, String) {
        (Status::InvalidArgument, message.to_string())
    }

    /// What the game says once the window let the player choose it.
    fn no_money() -> (Status, String) {
        (Status::Rejected, "no money".to_string())
    }

    #[test]
    fn an_act_the_panel_does_not_offer_is_refused_and_its_actions_destroyed() {
        let panel = Panel::offering(&[Act::SendGift]);
        let (status, message) = failed(panel.order(PanelOrder::OfferLoan { amount: 10 }));
        assert_eq!((status, message.as_str()), (Status::Rejected, "the diplomacy panel does not offer offer_loan toward BYZ"));
        assert_eq!(SCRIPT.with(|s| s.borrow().destroyed.clone()), [panel.actions[0].1.at()]);
        // A call to arms says so in its own words.
        assert_eq!(
            failed(super::super::call_to_arms(panel.session, eu4_harmony_remote::diplomacy::args::CallToArms { country: CountryTag(*b"BYZ") })),
            (Status::Rejected, "the diplomacy panel offers no call to arms to BYZ".to_string())
        );

        // Neither the player's own country nor one that is none is another.
        assert_eq!(failed(order_toward(&panel, tur(), PanelOrder::SendGift { amount: 5 })).0, Status::InvalidArgument);
        assert_eq!(failed(order_toward(&panel, wal(), PanelOrder::SendGift { amount: 5 })).0, Status::NotFound);
    }

    #[test]
    fn a_gift_is_of_a_ducat_at_least() {
        let panel = Panel::offering(&[Act::SendGift]);
        let gift = |amount| panel.order(PanelOrder::SendGift { amount });
        assert_eq!(failed(gift(0)), invalid("send_gift needs at least 1 ducat"));
        // The window writes it in thousandths, and the game's check is asked.
        assert_eq!(failed(gift(5)), no_money());
        assert_eq!(panel.field(Act::SendGift, FIELDS), 5000);
    }

    #[test]
    fn war_subsidies_go_as_far_as_the_treasury_for_the_window_s_durations() {
        let panel = Panel::offering(&[Act::SubsidizeWar]);
        let subsidies = |amount, duration| panel.order(PanelOrder::SubsidizeWar { amount, duration });
        assert_eq!(failed(subsidies(501, None)), invalid("501 ducats is more than the treasury's 500, as far as subsidize_war's window goes"));
        let durations = "the subsidies window goes from 10 to 400, by 10";
        assert_eq!(failed(subsidies(500, Some(15))), invalid(&format!("a duration of 15: {durations}")));
        assert_eq!(failed(subsidies(500, Some(410))), invalid(&format!("a duration of 410: {durations}")));
        assert_eq!(failed(subsidies(500, Some(0))), invalid(&format!("a duration of 0: {durations}")));
        assert_eq!(failed(subsidies(500, None)), no_money());
        assert_eq!((panel.field(Act::SubsidizeWar, FIELDS), panel.field(Act::SubsidizeWar, FIELDS + 4)), (500_000, 10));
        assert_eq!(failed(subsidies(20, Some(400))), no_money());
        assert_eq!((panel.field(Act::SubsidizeWar, FIELDS), panel.field(Act::SubsidizeWar, FIELDS + 4)), (20_000, 400));
    }

    #[test]
    fn a_loan_goes_as_far_as_the_treasury_and_trade_power_as_all_of_it() {
        let panel = Panel::offering(&[Act::OfferLoan, Act::TransferTradePower]);
        let loan = |amount| panel.order(PanelOrder::OfferLoan { amount });
        assert_eq!(failed(loan(0)), invalid("offer_loan needs at least 1 ducat"));
        assert_eq!(failed(loan(501)), invalid("501 ducats is more than the treasury's 500, as far as offer_loan's window goes"));
        assert_eq!(failed(loan(500)), no_money());
        assert_eq!((panel.field(Act::OfferLoan, FIELDS), panel.field(Act::OfferLoan, FIELDS + 4)), (500, LOAN_INTEREST as i32));
        let transfer = |percent| panel.order(PanelOrder::TransferTradePower { percent });
        assert_eq!(failed(transfer(101)), invalid("101 percent: a country has no more than all its trade power to transfer"));
        assert_eq!(failed(transfer(0)), invalid("transfer_trade_power needs at least 1 percent"));
        assert_eq!(failed(transfer(100)), no_money());
        assert_eq!(panel.field(Act::TransferTradePower, FIELDS), 1000);
    }

    #[test]
    fn favors_buy_trust_in_the_window_s_steps_up_to_the_favors_owed() {
        let panel = Panel::offering(&[Act::TradeFavorsForTrust]);
        let trade = |favors, trust| panel.order(PanelOrder::TradeFavorsForTrust { favors, trust });
        assert_eq!(failed(trade(15, None)), invalid("15 favors: the window spends them 10 at a time"));
        assert_eq!(failed(trade(60, None)), invalid("60 favors: BYZ owes 50 and trusts the player 50, so the window lets the player spend at most 50"));
        assert_eq!(failed(trade(50, Some(20))), invalid("20 trust: 50 favors buy 25, 5 for each 10"));
        assert_eq!(failed(trade(50, Some(25))), no_money());
        assert_eq!(failed(trade(50, None)), no_money());
        let fields = (panel.field(Act::TradeFavorsForTrust, FIELDS), panel.field(Act::TradeFavorsForTrust, FIELDS + 4));
        assert_eq!(fields, (50_000, 25));
    }

    #[test]
    fn what_the_window_chooses_and_the_framework_cannot_is_not_sent() {
        let panel = Panel::offering(&[Act::SellShips, Act::BreakAlliance]);
        assert_eq!(failed(panel.order(PanelOrder::Simple(Act::SellShips))).0, Status::Unavailable);
        assert_eq!(failed(panel.order(PanelOrder::BreakAlliance { ally: wal() })).0, Status::NotFound);
        assert_eq!(failed(panel.order(PanelOrder::BreakAlliance { ally: tur() })), no_money());
        assert_eq!(panel.actions[1].1.read_u64(FIELDS), tur().0);
    }

    #[test]
    fn pure_window_bounds_validate_subsidies_trade_power_and_trust_steps() {
        assert_eq!(check_subsidy_duration(None).unwrap(), 10);
        assert_eq!(check_subsidy_duration(Some(120)).unwrap(), 120);
        assert_eq!(check_subsidy_duration(Some(15)).unwrap_err().status, Status::InvalidArgument);

        assert_eq!(check_trade_power_percent(100).unwrap(), 100);
        assert_eq!(check_trade_power_percent(0).unwrap_err().status, Status::InvalidArgument);
        assert_eq!(check_trade_power_percent(101).unwrap_err().status, Status::InvalidArgument);

        assert_eq!(check_trust_favors("BYZ", 30, None, 50, 50, 10, 5).unwrap(), 15);
        assert_eq!(check_trust_favors("BYZ", 30, Some(15), 50, 50, 10, 5).unwrap(), 15);
        assert_eq!(check_trust_favors("BYZ", 30, Some(10), 50, 50, 10, 5).unwrap_err().status, Status::InvalidArgument);
        assert_eq!(check_trust_favors("BYZ", 0, None, 50, 50, 10, 5).unwrap_err().status, Status::InvalidArgument);
    }
}
