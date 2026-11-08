//! Diplomacy: the player's diplomatic actions, each sent as the player's
//! click in the diplomacy panel sends it (docs/research/diplomacy.md).
//!
//! The framework makes the game's own diplomatic action, from the player to
//! the other country, and asks it the checks the diplomacy panel and the
//! window that sends it ask; a refusal comes back with the game's reasons.
//! Then it has the action make the command that carries it, as the window
//! does, and posts the command in its written form through the command layer
//! ([`super::commands`]), which asks the command's own check too and has the
//! game carry it out at once (ADR commands-run-at-once), with its own checks
//! and costs: an AI country answers a proposal there, and its answer comes back
//! as a command of its own, carried out at the game's next frame. Each action
//! sent is a [`Ticket`]; [`answer`] says what came of it, from the commands the
//! game carried out, which an observer of the game's own function sees.
use super::peace::checks as checks_peace;
use super::{commands::Commands, context_any, countries, country, fail, game, Caller, Ctx, GResult, Session, Status, Tag, WorldSession};
use crate::called_by_game;
use crate::{
    game_string,
    hooks::Call,
    memory::Memory,
    objects::{
        diplomacy::{action, casus_belli, classes, command, factor, globals, send_window, side, sides, war_entry, war_goal, war_goal_type, war_goals, world},
        gui,
    },
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::choices;
use std::collections::VecDeque;
use std::sync::Mutex;

pub mod acts;
mod panel;

pub use acts::Act;
pub use panel::{act, options, Details, Offered, What};

/// The constructor of a declaration of war: (action, from, to, flag, date,
/// war goal), the war goal copied; the flag 0 as the diplomacy panel's.
const DECLARE_WAR_NEW: GameFunction<6> = GameFunction::at(0x500480);
/// The war goals the game offers a country against another: (from, to,
/// list, filter), into a [`classes::WAR_GOALS`] list; the filter an empty
/// `std::function`.
const WAR_GOAL_LIST: GameFunction<4> = GameFunction::at(0xd9eb40);
/// The list's destructor, its vtable's first method: (list, flags), which
/// destroys the war goals.
const WAR_GOAL_LIST_DESTROY: GameFunction<2> = GameFunction::at(0x114b80);
/// Carrying out a diplomatic action's command, its vtable's
/// [`command::CARRY_OUT`]: (command).
const CARRY_OUT: usize = 0x4e7c50;
/// The command's destructor, its vtable's [`command::DESTROY`]: (command,
/// flags), flag 1 freeing it.
const COMMAND_DESTROY: GameFunction<2> = GameFunction::at(0x4e7470);
const FREE: usize = 1;
/// The keys of the descriptions the checks append after their reasons.
const ALLIANCE_DESCRIPTION: usize = 0x1c83dc0;
const PEACE_DESCRIPTION: usize = 0x1c8abc0;
const WAR_DESCRIPTION: usize = 0x1c83498;
/// The types of a declaration of war, a peace offer and an alliance.
const DECLARE_WAR_KIND: u32 = 0x293b;
const PEACE_KIND: u32 = 0x293c;
const ALLIANCE_KIND: u32 = 0x293d;
/// The types of action whose sending the diplomacy window records as the
/// player's last alliance offer, when they start something: an alliance, an
/// invitation into the empire, into a trade league, and asking to join one.
const RECORDED_AS_ALLIANCE: [u32; 4] = [ALLIANCE_KIND, 0x3b85, 0x331d, 0x3320];
/// Whether the AI would accept an action, its base's method at `+0xb8`:
/// (action, factors), filling the `std::vector` of [`factor`]s given; and
/// that vector's destructor, (vector).
const ACCEPTS: GameFunction<2> = GameFunction::at(0x4ea1e0);
const FACTORS_FREE: GameFunction<1> = GameFunction::at(0x4183f0);
/// The most factors the game is believed to give.
const MOST_FACTORS: usize = 1_000;

/// What the declare war window gathers about a declaration of war: (action,
/// out [`sides`], 1 as the window passes it).
const GATHER: GameFunction<3> = GameFunction::at(0x503e70);
/// Frees a `std::vector` of [`side`] items: (vector).
const SIDES_FREE: GameFunction<1> = GameFunction::at(0x124150);
/// The declare war window's check whether the player may call the country
/// of one of its entries ([`war_entry`]): (entry, action, whether the
/// country is not among those promised land, reasons), the reasons a
/// `std::string` or null. Its call box asks it before it calls an ally.
const MAY_CALL: GameFunction<4> = GameFunction::at(0xee03f0);
/// A full `std::vector` of tags growing to take one more, as the call box
/// grows the allies called: (vector, where, the tag's address).
const TAGS_GROW: GameFunction<3> = GameFunction::at(0xdfde0);
/// The key of the check's reason when the ally is on the other side too.
const ALLIED_TO_ENEMY: usize = 0x1d1f5e0;

/// The most war goals the game is believed to offer.
const MOST_WAR_GOALS: usize = 10_000;
/// The most countries the game is believed to gather on one side of a war.
const MOST_ON_A_SIDE: usize = 1_000;
/// How many of the player's actions sent, and carried out, are kept.
const KEPT: usize = 256;

/// A type of diplomatic action, and whether it starts or ends something:
/// what the framework tells the player's actions apart by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kind {
    token: u32,
    offer: bool,
}

impl Kind {
    /// The kind of the action at `at`, as its methods and fields say.
    fn of(m: &Memory, at: usize) -> GResult<Self> {
        Ok(Self { token: m.call(m.vslot(at, action::KIND)?, &[at])? as u32, offer: m.u8(at + action::OFFER)? != 0 })
    }
}

/// How remote clients and extensions name a declaration of war without a
/// casus belli, where they name a casus belli by its key.
/// The casus belli a declaration without one names.
pub const NO_CASUS_BELLI: &str = "none";

/// A war goal the game offers the player for a declaration of war, as the
/// declare war window lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WarGoal {
    /// The key of its casus belli, such as `cb_claim`; None for a
    /// declaration without one.
    pub casus_belli: Option<String>,
    /// The key of its type, such as `take_claim`.
    pub war_goal: Option<String>,
    /// The province it is about, if any.
    pub province: Option<u32>,
    /// The country it is about, if any.
    pub target: Option<Tag>,
    /// Its name, as the window shows it.
    pub name: String,
}

/// Which of the war goals offered a declaration of war is for: the first
/// offered that has all that is given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    /// A casus belli's key, or None for a declaration without one.
    pub casus_belli: Option<String>,
    pub war_goal: Option<String>,
    pub province: Option<u32>,
    pub target: Option<Tag>,
}

impl Choice {
    fn matches(&self, goal: &WarGoal) -> bool {
        self.casus_belli == goal.casus_belli
            && self.war_goal.as_ref().is_none_or(|w| goal.war_goal.as_ref() == Some(w))
            && self.province.is_none_or(|p| goal.province == Some(p))
            && self.target.is_none_or(|t| goal.target.is_some_and(|g| g.index() == t.index()))
    }
}

/// An action sent, for [`answer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticket(pub u64);

/// Which of the player's allies a declaration of war calls to arms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Calls {
    /// Each ally the declare war window would let the player call.
    #[default]
    All,
    /// None.
    None,
    /// These allies, each one the window must let the player call.
    Only(Vec<Tag>),
}

/// A declaration of war sent: its ticket, the allies it calls to arms, and,
/// calling every ally the window would let the player call ([`Calls::All`]),
/// every other ally the window listed, each with why it was not called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub ticket: Ticket,
    pub called: Vec<Tag>,
    pub passed_over: Vec<PassedOver>,
}

/// An ally the declare war window listed that a declaration did not call to
/// arms, and why, one a line: when the window would not let the player call
/// it, the reasons its call box's check gives, as its tooltip shows them;
/// when the window no longer lists it once others are called, that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassedOver {
    pub country: Tag,
    pub reasons: Vec<String>,
}

/// What came of an action sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The game carried out an action that needs no answer.
    Done,
    /// Not answered yet: the game has not carried it out, or the other
    /// country has not answered. A proposal the game drops as it carries it
    /// out, its own check failing then, stays pending: the game says nothing
    /// of it.
    Pending,
    /// The other country accepted the proposal.
    Accepted,
    /// The other country declined it.
    Declined,
    /// The war began; its name, as the game shows it.
    Declared(String),
}

/// The war goals the game offers the player against `enemy`, as the
/// declare war window lists them, and last, when the game offers it, the
/// declaration without a casus belli.
pub fn war_goals(Session { m, ctx, .. }: Session, enemy: Tag) -> GResult<Vec<WarGoal>> {
    other(m, &ctx, enemy)?;
    let offered = WarGoalsOffered::list(m, &ctx, enemy)?;
    Ok(offered.goals.iter().map(|(_, goal)| goal.clone()).collect())
}

/// Declares war on `enemy` for the war goal `choice` picks among those the
/// game offers, calling to arms the allies `calls` names ([`call_allies`]).
/// [`Status::InvalidArgument`] for an empty casus belli, or for a country
/// to call that the declare war window does not list among the allies;
/// [`Status::Rejected`] when the game offers no such war goal, does not let
/// the player declare war now, or would not let the player call an ally
/// named, with its reasons.
pub fn declare_war(Session { m, ctx, commands, .. }: Session, enemy: Tag, choice: &Choice, calls: &Calls) -> GResult<Declaration> {
    other(m, &ctx, enemy)?;
    if choice.casus_belli.as_deref() == Some("") {
        return Err(checks::no_casus_belli(NO_CASUS_BELLI));
    }
    let offered = WarGoalsOffered::list(m, &ctx, enemy)?;
    let Some(&(goal, _)) = offered.goals.iter().find(|(_, g)| choice.matches(g)) else {
        return Err(checks::no_such_war_goal(choice.casus_belli.as_deref().unwrap_or(NO_CASUS_BELLI), letters(enemy)));
    };
    let mut made = Made::new(m, action::DECLARE_WAR_SIZE);
    DECLARE_WAR_NEW.call(m, [made.at(), ctx.player.0 as usize, enemy.0 as usize, 0, ctx.date as u32 as usize, goal])?;
    made.built = true;
    verdict(m, made.at())?;
    let (called, passed_over) = call_allies(&mut Window::new(m, &ctx, made.at()), ctx.player, enemy, calls)?;
    let ticket = send(commands, m, &ctx, made.at(), enemy, called.clone())?;
    Ok(Declaration { ticket, called, passed_over })
}

/// Offers `enemy` a white peace: a peace treaty with no terms chosen,
/// demanded by the player, in the war between them, as the peace view sends
/// one.
pub fn offer_peace(Session { m, ctx, commands, .. }: Session, enemy: Tag) -> GResult<Ticket> {
    other(m, &ctx, enemy)?;
    // The game's check refuses an offer to a country the player is not at
    // war with without a reason: it looks for the war between them first.
    if !super::world::at_war(m, &ctx, ctx.player, enemy)? {
        return fail(Status::Rejected, format!("the player is not at war with {}", letters(enemy)));
    }
    let treaty = super::peace::Treaty::new(m, &ctx, enemy, false)?;
    let offer = super::peace::Offer::new(m, &ctx, enemy, &treaty, false)?;
    verdict(m, offer.at())?;
    send(commands, m, &ctx, offer.at(), enemy, Vec::new())
}

/// Proposes an alliance to `ally`.
pub fn propose_alliance(Session { m, ctx, commands, .. }: Session, ally: Tag) -> GResult<Ticket> {
    other(m, &ctx, ally)?;
    let mut made = Made::new(m, action::ALLIANCE_SIZE);
    // As the game makes one, field by field: it has no constructor of its own.
    let fields: [(usize, &[u8]); 11] = [
        (0, &(m.at(classes::ALLIANCE) as u64).to_le_bytes()),
        (action::TYPE, &action::TYPE_ACTION.to_le_bytes()),
        (action::FROM, &ctx.player.0.to_le_bytes()),
        (action::ACTOR, &ctx.player.0.to_le_bytes()),
        (action::TO, &ally.0.to_le_bytes()),
        (action::MADE, &ctx.date.to_le_bytes()),
        (action::SENT, &ctx.date.to_le_bytes()),
        (action::PLAN, &(-1i32).to_le_bytes()),
        (action::STATE, &action::UNANSWERED.to_le_bytes()),
        (action::OFFER, &[1, 0]),
        (action::ID, &0i32.to_le_bytes()),
    ];
    for (at, bytes) in fields {
        made.put(at, bytes);
    }
    made.built = true;
    verdict(m, made.at())?;
    send(commands, m, &ctx, made.at(), ally, Vec::new())
}

/// Makes `rival` one of the player's rivals, with the game's `add_rival`
/// command, as the diplomacy view's rivals list posts it: refused
/// ([`Status::Rejected`]) with the game's reasons when the game does not let
/// the player rival that country, and with none when the player has as many
/// rivals as the game allows.
pub fn add_rival(session: Session, rival: Tag) -> GResult<()> {
    rival_command(session, "add_rival", CAN_RIVAL, rival, "rival that country")
}

/// Drops `rival`, with the game's `remove_rival` command; refused with the
/// game's reasons when it does not let the player.
pub fn remove_rival(session: Session, rival: Tag) -> GResult<()> {
    rival_command(session, "remove_rival", CAN_DROP_RIVAL, rival, "drop that rival")
}

/// Whether a country may rival another, and drop it as a rival: (country,
/// other's tag, reasons), the reasons appended to a `std::string` or null; the
/// rivals list asks them, and so do the commands' checks.
const CAN_RIVAL: GameFunction<3> = GameFunction::at(0x3d2760);
const CAN_DROP_RIVAL: GameFunction<3> = GameFunction::at(0x3d2970);

fn rival_command(Session { m, ctx, commands, .. }: Session, kind: &str, check: GameFunction<3>, rival: Tag, doing: &str) -> GResult<()> {
    other(m, &ctx, rival)?;
    let args = |reasons: usize| [ctx.country, rival.0 as usize, reasons];
    if check.call(m, args(0))? as u8 == 0 {
        let said = game_string::written(m, |s| check.call(m, args(s)))?.1;
        return Err(super::action::not_allowed(doing, &reasons_said(&said)));
    }
    super::action::post(commands, &format!("{kind}={{ country=\"{}\" rival=\"{}\" }}", letters(ctx.player), letters(rival)), doing)
}

/// Calls `ally` to arms in the player's wars, as the diplomacy panel's call
/// to arms button and the window it opens send the call, with no land
/// promised. [`Status::Rejected`] when the panel offers no call to arms to
/// it, or the game does not let the player send it, with its reasons.
pub fn call_to_arms(session: Session, ally: Tag) -> GResult<Ticket> {
    act(session, ally, &What::Act(Act::CallToArms), &Details::default()).map_err(|fail| {
        if fail.status == Status::Rejected && fail.message.starts_with(panel::NOT_OFFERED) {
            checks_peace::no_call_to_arms(letters(ally))
        } else {
            fail
        }
    })
}

/// Whether the other country would accept the action at `at`, as the
/// window that sends it shows (the peace view's, for a peace offer): the verdict, the sum, and each factor, with
/// its text as the window's tooltip writes it.
pub(super) fn acceptance(m: &Memory, at: usize) -> GResult<choices::Acceptance> {
    let mut factors = [0usize; 3];
    let factors_at = factors.as_mut_ptr() as usize;
    let accepts = ACCEPTS.call(m, [at, factors_at]);
    let factors = unsafe { std::ptr::read_volatile(&factors) };
    let read = accepts.map_err(super::Fail::from).and_then(|accepts| {
        let size = factors[1].wrapping_sub(factors[0]);
        if factors[1] < factors[0] || size % factor::SIZE != 0 || size / factor::SIZE > MOST_FACTORS {
            return Err("the factors are out of bounds".to_string().into());
        }
        let mut read = Vec::new();
        for item in (factors[0]..factors[1]).step_by(factor::SIZE) {
            let text = game_string::shown(m, item + factor::TEXT)?.trim().to_string();
            read.push(choices::Factor { text, value: m.i32(item + factor::VALUE)? as f64 });
        }
        Ok((accepts as u8 != 0, read))
    });
    FACTORS_FREE.call(m, [factors_at])?;
    let (accepts, factors) = read?;
    Ok(choices::Acceptance { accepts, total: factors.iter().map(|f| f.value).sum(), factors })
}

/// What came of an action sent. [`Status::NotFound`] for a ticket that is
/// no longer kept; [`Status::NotInGame`] once the game it was sent in has
/// ended; [`Status::Rejected`] for a declaration of war the game carried
/// out without the war beginning, and once another country is acted as than
/// the one that sent it: in observe mode, another country in view, or none
/// (ADR observe-mode-acts-as-the-country-in-view).
pub fn answer(world: WorldSession, ticket: Ticket) -> GResult<Answer> {
    let (sent, seen) = kept(ticket)?;
    still_acting(sent.from, world.world.player.acting())?;
    let session = Session::open(world.caller)?;
    Ok(match seen {
        Seen::Answered(true) => Answer::Accepted,
        Seen::Answered(false) => Answer::Declined,
        // A declaration has no answer: it is done once carried out.
        Seen::CarriedOut if sent.kind.token == DECLARE_WAR_KIND => match war_with(session, sent.to)? {
            Some(name) => Answer::Declared(name),
            None => return fail(Status::Rejected, format!("the game carried the declaration out, but no war with {} began", letters(sent.to))),
        },
        Seen::CarriedOut if !sent.answers => Answer::Done,
        Seen::CarriedOut | Seen::Nothing => Answer::Pending,
    })
}

/// Whether the country an action was sent as, `from`, is still the one acted
/// as; [`Status::Rejected`] if not.
fn still_acting(from: Tag, acting: Option<Tag>) -> GResult<()> {
    if acting.map(Tag::index) == Some(from.index()) {
        return Ok(());
    }
    let now = acting.map_or("no country is in view".to_string(), |country| format!("{} is in view", letters(country)));
    fail(Status::Rejected, format!("it was sent as {}, and {now}: what comes of it is no longer seen", letters(from)))
}

/// The allies a declaration of war sent called to arms, none for a proposal;
/// [`Status::NotFound`] and [`Status::NotInGame`] as [`answer`].
pub fn allies_called(_: Caller, ticket: Ticket) -> GResult<Vec<Tag>> {
    game()?;
    Ok(kept(ticket)?.0.called)
}

/// The action sent with `ticket`, and what the game did with it so far,
/// while it is kept and its game goes on.
fn kept(ticket: Ticket) -> GResult<(Sent, Seen)> {
    let (sent, seen) = ledger(|l| l.seen(ticket)).ok_or_else(|| checks::not_kept(ticket.0))?;
    if sent.session != crate::sessions::number() {
        return fail(Status::NotInGame, "the game the diplomatic action was sent in has ended");
    }
    Ok((sent, seen))
}

/// The name of the player's latest war with `enemy` on the other side.
fn war_with(session: Session, enemy: Tag) -> GResult<Option<String>> {
    let (player, ctx) = (countries::snapshot(session.world()?, session.ctx.player)?, session.ctx);
    let opposed = |ours: &[Tag], theirs: &[Tag]| ours.iter().any(|t| t.index() == ctx.player.index()) && theirs.iter().any(|t| t.index() == enemy.index());
    Ok(player.wars.into_iter().rev().find(|w| opposed(&w.attackers, &w.defenders) || opposed(&w.defenders, &w.attackers)).map(|w| w.name))
}

/// A country of the game's other than the player's.
pub(super) fn other(m: &Memory, ctx: &Ctx, tag: Tag) -> GResult<()> {
    country(m, ctx, tag)?;
    if tag.index() == ctx.player.index() {
        return Err(checks::own_country());
    }
    Ok(())
}

pub(super) fn letters(tag: Tag) -> String {
    String::from_utf8_lossy(&tag.letters()).into_owned()
}

// ---------- sending ----------

/// An action the framework makes in its own memory, which the game's code
/// fills in and copies; destroyed, without freeing, once sent.
pub(super) struct Made<'a> {
    m: &'a Memory,
    object: Vec<u64>,
    /// Its constructor ran, so its destructor must.
    pub(super) built: bool,
}

impl<'a> Made<'a> {
    pub(super) fn new(m: &'a Memory, size: usize) -> Self {
        Self { m, object: vec![0; size.div_ceil(8)], built: false }
    }

    pub(super) fn at(&self) -> usize {
        self.object.as_ptr() as usize
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        let object: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(self.object.as_mut_ptr() as *mut u8, self.object.len() * 8) };
        object[at..at + bytes.len()].copy_from_slice(bytes);
    }
}

impl Drop for Made<'_> {
    fn drop(&mut self) {
        if !self.built {
            return;
        }
        let at = self.at();
        if let Err(e) = self.m.vslot(at, action::DESTROY).and_then(|d| self.m.call(d, &[at, 0])) {
            log!("diplomacy: destroying an action: {e}");
        }
    }
}

/// Sends the action at `at`, which the game lets the player send
/// ([`verdict`]): dates it and has it make its command, as the diplomacy
/// window does, posts the command's written form through `commands`, and
/// keeps the action for [`answer`], with the allies it calls.
pub(super) fn send(commands: Commands, m: &Memory, ctx: &Ctx, at: usize, to: Tag, called: Vec<Tag>) -> GResult<Ticket> {
    let kind = Kind::of(m, at)?;
    let answers = m.call(m.vslot(at, action::NEEDS_ANSWER)?, &[at])? as u8 != 0;
    m.write(at + action::MADE, &ctx.date.to_le_bytes())?;
    let mut command = 0usize;
    m.call(m.vslot(at, action::MAKE_COMMAND)?, &[at, &mut command as *mut usize as usize])?;
    if command == 0 || m.ptr(command)? != m.at(classes::ACTION_COMMAND) {
        return Err("the diplomatic action made no command".to_string().into());
    }
    // The command is made only to be written; the layer posts the one it
    // reads from the text.
    let text = super::commands::text(m, command);
    COMMAND_DESTROY.call(m, [command, FREE])?;
    let after = ledger(|l| l.carried_count);
    super::action::post(commands, &text?, "send the diplomatic action")?;
    // As the window, so that the panel lets no second one go the same day.
    let record = if kind.offer && RECORDED_AS_ALLIANCE.contains(&kind.token) {
        Some((world::ALLIANCE_OFFER_FROM, world::ALLIANCE_OFFER_DATE))
    } else if kind.token == PEACE_KIND {
        Some((world::PEACE_OFFER_FROM, world::PEACE_OFFER_DATE))
    } else {
        None
    };
    if let Some((from, date)) = record {
        m.write(ctx.world + from, &ctx.player.0.to_le_bytes())?;
        m.write(ctx.world + date, &ctx.date.to_le_bytes())?;
    }
    Ok(ledger(|l| l.send(kind, answers, ctx.player, to, after, crate::sessions::number(), called)))
}

/// Asks the action the checks the diplomacy panel and the window ask before
/// the player can send it; [`Status::Rejected`] with the game's reasons when
/// either says no.
pub(super) fn verdict(m: &Memory, at: usize) -> GResult<()> {
    let judged = judge(m, at)?;
    if judged.can {
        return Ok(());
    }
    fail(Status::Rejected, refusal(&judged.reasons))
}

/// The checks [`verdict`] asks: None when both let the player send it, and
/// otherwise the reasons they give, each once; empty when they give none.
pub(super) fn refusals(m: &Memory, at: usize) -> GResult<Option<Vec<String>>> {
    let judged = judge(m, at)?;
    Ok((!judged.can).then_some(judged.reasons))
}

/// What the checks the diplomacy panel and the window ask say of an action:
/// whether both let the player send it, the reasons of those that do not,
/// and the action's description, which each appends after its reasons.
struct Judged {
    can: bool,
    reasons: Vec<String>,
    description: Vec<String>,
}

fn judge(m: &Memory, at: usize) -> GResult<Judged> {
    let mut judged = Judged { can: true, reasons: Vec::new(), description: Vec::new() };
    for slot in [action::OFFERED, action::ALLOWED] {
        let allowed = m.call(m.vslot(at, slot)?, &[at, 0, 0])? as u8 != 0;
        let (reasons, description) = said(&reasons(m, at, slot)?);
        if !allowed {
            judged.can = false;
            for line in reasons {
                if !judged.reasons.contains(&line) {
                    judged.reasons.push(line);
                }
            }
        }
        if judged.description.is_empty() {
            judged.description = description;
        }
    }
    Ok(judged)
}

/// What a check appends to a `std::string`.
fn reasons(m: &Memory, at: usize, slot: usize) -> GResult<Vec<u8>> {
    let check = m.vslot(at, slot)?;
    Ok(game_string::written(m, |s| m.call(check, &[at, s, 0]))?.1)
}

/// Game text's lines as plain Unicode, a reason's bullet left out.
pub(super) fn text_lines(text: &[u8]) -> Vec<String> {
    game_text::plain(text).lines().map(|l| l.trim()).map(|l| l.strip_prefix("- ").unwrap_or(l).trim()).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// What a check wrote, as plain Unicode lines: its reasons, each a line
/// with a bullet, which is left out; and the description after them.
fn said(text: &[u8]) -> (Vec<String>, Vec<String>) {
    let (mut reasons, mut description) = (Vec::new(), Vec::new());
    for line in game_text::plain(text).lines().map(str::trim).filter(|l| !l.is_empty()) {
        match line.strip_prefix("- ").or_else(|| line.strip_prefix('－')) {
            Some(reason) => reasons.push(reason.trim().to_string()),
            None => description.push(line.to_string()),
        }
    }
    (reasons, description)
}

/// The reasons a check of the game's wrote, one a line: its bulleted lines,
/// or, when it bullets none, every line.
pub(super) fn reasons_said(text: &[u8]) -> Vec<String> {
    let (reasons, rest) = said(text);
    if reasons.is_empty() {
        rest
    } else {
        reasons
    }
}

/// The refusal's message: the reasons the checks gave.
fn refusal(reasons: &[String]) -> String {
    if reasons.is_empty() {
        "the game does not allow it, and gives no reason".into()
    } else {
        reasons.join("\n")
    }
}

// ---------- calls to arms ----------

/// Calls to arms the allies `calls` names, as the player ticking the declare
/// war window's call boxes one after another from the top calls them, and
/// answers the allies called. The window lists as allies the countries it
/// gathers on the declarer's side ([`Sides`]) but the player and `enemy`;
/// one that joins anyway has no call box to tick. Its call box asks the
/// window's check ([`MAY_CALL`]) with the ally not among those promised
/// land, as none is while the promise land boxes are left alone, and calls
/// an ally the check allows by putting it into the declaration's
/// [`action::CALLED`]. The window gathers again after each call, so this
/// does too. The check reads the window's enemies list, which [`Asking`]
/// leaves empty; [`asked_by_enemy`] stands for it.
///
/// [`Calls::All`] calls each ally the check allows, and passes over every
/// other ally the window listed at first: one the check does not allow,
/// with the check's reasons, and one the window no longer lists once others
/// are called, with that ([`dropped_out`]). [`Calls::Only`] calls those
/// named, and refuses the declaration ([`Status::Rejected`], with the
/// check's reasons) when the check does not allow one, it joins anyway, or
/// the window no longer lists it once others are called: the window would
/// not let the player tick its box. A named country the window does not
/// list among the allies is [`Status::InvalidArgument`].
fn call_allies(window: &mut impl CallBoxes, player: Tag, enemy: Tag, calls: &Calls) -> GResult<(Vec<Tag>, Vec<PassedOver>)> {
    let named = match calls {
        Calls::None => return Ok((Vec::new(), Vec::new())),
        Calls::All => None,
        Calls::Only(named) => Some(named.as_slice()),
    };
    let listed = |s: &Side| ![player.index(), enemy.index()].contains(&s.country().index());
    let first: Vec<Tag> = window.gather()?.0.iter().filter(|s| listed(s)).map(Side::country).collect();
    if let Some(stranger) = named.and_then(|named| named.iter().find(|t| !first.iter().any(|f| f.index() == t.index()))) {
        return Err(checks::not_listed(letters(*stranger)));
    }
    let mut asked: Vec<i16> = Vec::new();
    let mut called = Vec::new();
    let mut passed_over = Vec::new();
    for _ in 0..MOST_ON_A_SIDE {
        let (ours, theirs) = window.gather()?;
        let Some(ally) = ours.into_iter().filter(listed).find(|s| !asked.contains(&s.country().index())) else {
            // Each ally the window listed at first was asked, unless the
            // window listed it no more once others were called.
            let mut gone = first.iter().copied().filter(|t| !asked.contains(&t.index()));
            match named {
                Some(named) => {
                    if let Some(gone) = gone.find(|t| named.iter().any(|n| n.index() == t.index())) {
                        return fail(Status::Rejected, dropped_out(gone, &called));
                    }
                }
                None => passed_over.extend(gone.map(|country| PassedOver { country, reasons: vec![dropped_out(country, &called)] })),
            }
            return Ok((called, passed_over));
        };
        asked.push(ally.country().index());
        let country = ally.country();
        if named.is_some_and(|named| !named.iter().any(|t| t.index() == country.index())) {
            continue;
        }
        if ally.joins() {
            if named.is_some() {
                return fail(
                    Status::Rejected,
                    format!("{} joins the war without being called: the declare war window has no call box for it", letters(country)),
                );
            }
            continue;
        }
        let theirs: Vec<Side> = theirs.into_iter().filter(listed).collect();
        let enemy_asks = asked_by_enemy(window, &theirs, country)?;
        if !enemy_asks && window.may_call(&ally, true)? {
            window.call(country)?;
            called.push(country);
            continue;
        }
        let reasons = if enemy_asks { window.allied_to_enemy()? } else { window.why_not(&ally)? };
        if named.is_some() {
            return Err(checks::not_callable(letters(country), &reasons));
        }
        passed_over.push(PassedOver { country, reasons });
    }
    Err("the declare war window's allies kept growing".to_string().into())
}

/// Why an ally cannot be called that the declare war window listed at first
/// and no longer lists once `called` are called.
fn dropped_out(gone: Tag, called: &[Tag]) -> String {
    let once = match called {
        [] => String::new(),
        [one] => format!(" once {} is called to arms", letters(*one)),
        more => format!(" once {} are called to arms", more.iter().map(|&t| letters(t)).collect::<Vec<_>>().join(", ")),
    };
    format!("the declare war window no longer lists {} among the allies{once}, so the player could not call it", letters(gone))
}

/// Whether the declare war window lists `ally` among the enemies too, as a
/// country that joins that side anyway or one the check lets the other side
/// call: the window then shows it accepting there, and its check lets the
/// player call it no more (`ALLIED_TO_ENEMY`).
fn asked_by_enemy(window: &mut impl CallBoxes, theirs: &[Side], ally: Tag) -> GResult<bool> {
    for enemy in theirs.iter().filter(|s| s.country().index() == ally.index()) {
        if enemy.joins() || (window.ai_plays(ally)? && window.may_call(enemy, false)?) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What calling allies to arms asks of the declare war window: the game's
/// own code ([`Window`]), or a test's stand-in.
trait CallBoxes {
    /// The countries the window gathers on the declarer's side and on the
    /// other side.
    fn gather(&mut self) -> GResult<(Vec<Side>, Vec<Side>)>;
    /// Whether the window's checks take `country` for the AI's.
    fn ai_plays(&mut self, country: Tag) -> GResult<bool>;
    /// Whether the window's check lets the player call a country, its entry
    /// in the allies list (`ally`) or in the enemies list.
    fn may_call(&mut self, country: &Side, ally: bool) -> GResult<bool>;
    /// The reasons the check gives for not letting the player call an ally,
    /// as the window's tooltip shows them, one a line.
    fn why_not(&mut self, ally: &Side) -> GResult<Vec<String>>;
    /// The reason the check gives when the other side lists the ally too.
    fn allied_to_enemy(&mut self) -> GResult<Vec<String>>;
    /// Calls an ally to arms, as its ticked call box does.
    fn call(&mut self, ally: Tag) -> GResult<()>;
}

/// The declare war window, as the game's code answers for it, about the
/// declaration at `action`.
struct Window<'a> {
    m: &'a Memory,
    ctx: &'a Ctx,
    action: usize,
    asking: Asking<'a>,
}

impl<'a> Window<'a> {
    fn new(m: &'a Memory, ctx: &'a Ctx, action: usize) -> Self {
        Self { m, ctx, action, asking: Asking::new(m, action) }
    }
}

impl CallBoxes for Window<'_> {
    fn gather(&mut self) -> GResult<(Vec<Side>, Vec<Side>)> {
        let sides = Sides::gather(self.m, self.action)?;
        Ok((sides.side(sides::DECLARER)?, sides.side(sides::OTHER)?))
    }

    fn ai_plays(&mut self, country: Tag) -> GResult<bool> {
        ai_plays(self.m, self.ctx, country)
    }

    fn may_call(&mut self, country: &Side, ally: bool) -> GResult<bool> {
        self.asking.may_call(country, ally)
    }

    fn why_not(&mut self, ally: &Side) -> GResult<Vec<String>> {
        self.asking.why_not(ally)
    }

    fn allied_to_enemy(&mut self) -> GResult<Vec<String>> {
        Ok(text_lines(&game_string::localized(self.m, self.m.at(ALLIED_TO_ENEMY))?))
    }

    fn call(&mut self, ally: Tag) -> GResult<()> {
        push_tag(self.m, self.action + action::CALLED, ally)
    }
}

/// Whether the declare war window's checks take `country` for the AI's:
/// the world knows which countries human players play, and none plays it.
fn ai_plays(m: &Memory, ctx: &Ctx, country: Tag) -> GResult<bool> {
    let (start, end) = (m.ptr(ctx.world + world::HUMANS)?, m.ptr(ctx.world + world::HUMANS + 8)?);
    let index = country.index();
    if end <= start || index < 0 || (index as usize) >= (end - start) / 4 {
        return Ok(false);
    }
    Ok(m.i32(start + index as usize * 4)? == 0)
}

/// Puts `tag` at the end of the `std::vector` of tags at `vector`, as the
/// call box does: in place while it has room, and otherwise through the
/// game's own growing, so that the game frees it with the declaration.
fn push_tag(m: &Memory, vector: usize, tag: Tag) -> GResult<()> {
    let (end, capacity) = (m.ptr(vector + 8)?, m.ptr(vector + 16)?);
    if end != capacity {
        m.write(end, &tag.0.to_le_bytes())?;
        m.write(vector + 8, &(end as u64 + 8).to_le_bytes())?;
    } else {
        let value = tag.0;
        TAGS_GROW.call(m, [vector, end, &value as *const u64 as usize])?;
    }
    Ok(())
}

/// One country the declare war window gathers ([`side`]).
#[derive(Clone, Copy)]
struct Side([u8; side::SIZE]);

impl Side {
    fn country(&self) -> Tag {
        Tag(u64::from_le_bytes(self.0[side::COUNTRY..side::COUNTRY + 8].try_into().unwrap()))
    }

    fn joins(&self) -> bool {
        self.0[side::JOINS] != 0
    }
}

/// What the declare war window gathers about a declaration, in a [`sides`]
/// of the framework's; its lists freed when dropped.
struct Sides<'a> {
    m: &'a Memory,
    raw: [u64; sides::SIZE / 8],
}

impl<'a> Sides<'a> {
    fn gather(m: &'a Memory, action: usize) -> GResult<Self> {
        let mut sides = Self { m, raw: [0; sides::SIZE / 8] };
        GATHER.call(m, [action, sides.raw.as_mut_ptr() as usize, 1])?;
        Ok(sides)
    }

    /// The countries on one side, [`sides::DECLARER`] or [`sides::OTHER`].
    fn side(&self, which: usize) -> GResult<Vec<Side>> {
        let at = self.raw.as_ptr() as usize + which;
        let (start, end) = (self.m.ptr(at)?, self.m.ptr(at + 8)?);
        let size = end.wrapping_sub(start);
        if end < start || size % side::SIZE != 0 || size / side::SIZE > MOST_ON_A_SIDE {
            return Err("the countries gathered on a side are out of bounds".to_string().into());
        }
        let bytes = self.m.bytes(start, size)?;
        Ok(bytes.chunks_exact(side::SIZE).map(|item| Side(item.try_into().unwrap())).collect())
    }
}

impl Drop for Sides<'_> {
    fn drop(&mut self) {
        for which in [sides::DECLARER, sides::OTHER] {
            let at = self.raw.as_ptr() as usize + which;
            if let Err(e) = SIDES_FREE.call(self.m, [at]) {
                log!("diplomacy: freeing the countries gathered: {e}");
            }
        }
    }
}

/// What the declare war window's check ([`MAY_CALL`]) reads of the window,
/// made in the framework's memory: the declaration it sends, and its window,
/// whose list box of any name, the enemies' among them, has no entries.
struct Asking<'a> {
    m: &'a Memory,
    action: usize,
    window: Box<[usize; send_window::SIZE / 8]>,
    element: Box<[usize; 2]>,
    methods: Box<[usize; gui::window::FIND_LIST_BOX / 8 + 1]>,
}

/// The made-up window's methods: its list box of any name is the window
/// itself, and its entries start at its second word, null.
#[called_by_game]
extern "C" fn list_box_of(window: usize, _name: usize) -> usize {
    window
}

#[called_by_game]
extern "C" fn no_entries(window: usize) -> usize {
    window + 8
}

impl<'a> Asking<'a> {
    fn new(m: &'a Memory, action: usize) -> Self {
        let mut asking = Self {
            m,
            action,
            window: Box::new([0; send_window::SIZE / 8]),
            element: Box::new([0; 2]),
            methods: Box::new([0; gui::window::FIND_LIST_BOX / 8 + 1]),
        };
        asking.methods[gui::window::FIND_LIST_BOX / 8] = list_box_of as *const () as usize;
        asking.methods[gui::list_box::ENTRIES / 8] = no_entries as *const () as usize;
        asking.element[0] = asking.methods.as_ptr() as usize;
        asking.window[gui::dialog::WINDOW / 8] = asking.element.as_ptr() as usize;
        asking.window[send_window::ACTION / 8] = action;
        asking
    }

    /// Whether the check lets the player call `country`, its entry in the
    /// allies list (`ally`) or in the enemies list.
    fn may_call(&self, country: &Side, ally: bool) -> GResult<bool> {
        self.ask(country, ally, 0)
    }

    /// The reasons the check gives for an ally, as the window's tooltip
    /// shows them, one a line.
    fn why_not(&self, ally: &Side) -> GResult<Vec<String>> {
        Ok(text_lines(&game_string::written(self.m, |s| self.ask(ally, true, s))?.1))
    }

    /// The check, its reasons into the `std::string` at `reasons` unless 0.
    fn ask(&self, country: &Side, ally: bool, reasons: usize) -> GResult<bool> {
        let mut entry = [0u8; war_entry::SIZE];
        entry[war_entry::WINDOW..war_entry::WINDOW + 8].copy_from_slice(&(self.window.as_ptr() as u64).to_le_bytes());
        entry[war_entry::SIDE..war_entry::SIDE + side::SIZE].copy_from_slice(&country.0);
        entry[war_entry::ALLY] = ally as u8;
        entry[war_entry::PROMISE_BOX] = 1;
        let not_promised_land = 1;
        Ok(MAY_CALL.call(self.m, [entry.as_ptr() as usize, self.action, not_promised_land, reasons])? as u8 != 0)
    }
}

// ---------- the war goals offered ----------

/// The war goals the game offers, in a list of its own that owns them; freed
/// when dropped.
struct WarGoalsOffered<'a> {
    m: &'a Memory,
    list: Box<[u64; war_goals::SIZE / 8]>,
    /// Each war goal's address and what it is, the declaration without a
    /// casus belli last.
    goals: Vec<(usize, WarGoal)>,
}

impl<'a> WarGoalsOffered<'a> {
    fn list(m: &'a Memory, ctx: &Ctx, enemy: Tag) -> GResult<Self> {
        let mut list = Box::new([0u64; war_goals::SIZE / 8]);
        list[0] = m.at(classes::WAR_GOALS) as u64;
        let mut offered = Self { m, list, goals: Vec::new() };
        // An empty `std::function`: no filter.
        let filter = [0u64; 8];
        WAR_GOAL_LIST.call(m, [ctx.player.0 as usize, enemy.0 as usize, offered.at(), filter.as_ptr() as usize])?;
        let count = m.i32(offered.at() + war_goals::COUNT)?;
        if !(0..=MOST_WAR_GOALS as i32).contains(&count) {
            return Err("war goal list bound".to_string().into());
        }
        let items = m.ptr(offered.at() + war_goals::ITEMS)?;
        let mut exclusive = false;
        for i in 0..count as usize {
            let goal = m.ptr(items + i * 8)?;
            if goal == 0 {
                continue;
            }
            let (read, sole) = read_goal(m, goal)?;
            exclusive |= sole;
            offered.goals.push((goal, read));
        }
        // The window offers no declaration without one when an exclusive
        // casus belli is there.
        let none = m.ptr(m.at(globals::NO_CASUS_BELLI))?;
        if none != 0 && !exclusive {
            let (mut read, _) = read_goal(m, none)?;
            read.casus_belli = None;
            offered.goals.push((none, read));
        }
        Ok(offered)
    }

    fn at(&self) -> usize {
        self.list.as_ptr() as usize
    }
}

impl Drop for WarGoalsOffered<'_> {
    fn drop(&mut self) {
        if let Err(e) = WAR_GOAL_LIST_DESTROY.call(self.m, [self.at(), 0]) {
            log!("diplomacy: freeing the war goals: {e}");
        }
    }
}

/// A war goal, and whether its casus belli is exclusive.
fn read_goal(m: &Memory, goal: usize) -> GResult<(WarGoal, bool)> {
    let key = |object: usize, at: usize| -> GResult<Option<String>> {
        if object == 0 {
            return Ok(None);
        }
        Ok(Some(game_string::key(m, object + at)?))
    };
    let cb = m.ptr(goal + war_goal::CASUS_BELLI)?;
    let province = m.i32(goal + war_goal::PROVINCE)?;
    let target = Tag(m.u64(goal + war_goal::TARGET)?);
    let exclusive = cb != 0 && m.u8(cb + casus_belli::EXCLUSIVE)? != 0;
    Ok((
        WarGoal {
            casus_belli: key(cb, casus_belli::KEY)?,
            war_goal: key(m.ptr(goal + war_goal::TYPE)?, war_goal_type::KEY)?,
            province: (province > 0).then_some(province as u32),
            target: (m.u8(goal + war_goal::TARGET_VALID)? != 0 && target.normal()).then_some(target),
            name: goal_name(m, goal)?,
        },
        exclusive,
    ))
}

/// A war goal's name, as the game shows it.
fn goal_name(m: &Memory, goal: usize) -> GResult<String> {
    let name = m.vslot(goal, war_goal::NAME)?;
    Ok(game_text::plain(&game_string::written(m, |s| m.call(name, &[goal, s]))?.1))
}

// ---------- what came of them ----------

/// An action sent.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Sent {
    ticket: Ticket,
    kind: Kind,
    /// It needs the other country's answer.
    answers: bool,
    /// The country it was sent as.
    from: Tag,
    to: Tag,
    /// How many actions the game had carried out before.
    after: u64,
    session: u64,
    /// The allies a declaration calls.
    called: Vec<Tag>,
}

/// One of the player's actions the game carried out: a proposal or a
/// declaration ([`action::UNANSWERED`]), or an answer to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Carried {
    number: u64,
    kind: Kind,
    to: Tag,
    state: i32,
}

/// What the game did with an action sent, so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    Nothing,
    CarriedOut,
    /// The other country's answer: whether it accepted.
    Answered(bool),
}

#[derive(Default)]
struct Ledger {
    sent: VecDeque<Sent>,
    tickets: u64,
    carried: VecDeque<Carried>,
    carried_count: u64,
}

impl Ledger {
    #[expect(clippy::too_many_arguments, reason = "each is a field of the action kept")]
    fn send(&mut self, kind: Kind, answers: bool, from: Tag, to: Tag, after: u64, session: u64, called: Vec<Tag>) -> Ticket {
        self.tickets += 1;
        let ticket = Ticket(self.tickets);
        if self.sent.len() == KEPT {
            self.sent.pop_front();
        }
        self.sent.push_back(Sent { ticket, kind, answers, from, to, after, session, called });
        ticket
    }

    fn carried_out(&mut self, kind: Kind, to: Tag, state: i32) {
        self.carried_count += 1;
        if self.carried.len() == KEPT {
            self.carried.pop_front();
        }
        self.carried.push_back(Carried { number: self.carried_count, kind, to, state });
    }

    /// The action sent with `ticket`, and what the game did with it, from
    /// what it carried out of the same kind to the same country since: the
    /// first proposal or declaration, then, for a proposal, the first answer
    /// after it. An answer before that is an earlier proposal's, carried out
    /// before this one was sent.
    fn seen(&self, ticket: Ticket) -> Option<(Sent, Seen)> {
        let sent = self.sent.iter().find(|s| s.ticket == ticket)?.clone();
        let mut seen = Seen::Nothing;
        for c in self.carried.iter().filter(|c| c.number > sent.after && c.kind == sent.kind && c.to.index() == sent.to.index()) {
            match (seen, c.state) {
                (Seen::Nothing, action::UNANSWERED) => seen = Seen::CarriedOut,
                (Seen::CarriedOut, action::ACCEPTED | action::DECLINED) if sent.answers => return Some((sent, Seen::Answered(c.state == action::ACCEPTED))),
                _ => {}
            }
        }
        Some((sent, seen))
    }
}

static LEDGER: Mutex<Option<Ledger>> = Mutex::new(None);

fn ledger<T>(f: impl FnOnce(&mut Ledger) -> T) -> T {
    let mut guard = LEDGER.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Ledger::default))
}

/// Before the game carries out a diplomatic action's command: keeps the
/// player's actions.
fn carry_out_before(call: &mut Call) {
    let m = &call.memory();
    if crate::sessions::loading() {
        return;
    }
    let command = call.arg(0);
    let found = (|| -> GResult<Option<(Kind, Tag, i32)>> {
        let a = m.ptr(command + command::ACTION)?;
        let Ok(ctx) = context_any(m) else { return Ok(None) };
        if Tag(m.u64(a + action::FROM)?).index() != ctx.player.index() {
            return Ok(None);
        }
        Ok(Some((Kind::of(m, a)?, Tag(m.u64(a + action::TO)?), m.i32(a + action::STATE)?)))
    })();
    match found {
        Ok(Some((kind, to, state))) => ledger(|l| l.carried_out(kind, to, state)),
        Ok(None) => {}
        Err(e) => log!("diplomacy: reading a command carried out: {}", e.message),
    }
}

/// The functions diplomacy calls and observes; the objects are in
/// `objects::diplomacy`.
pub fn points() -> Vec<Point> {
    use classes::*;
    let method = |class: usize, slot: usize, function: usize, what: &'static str| Point::method(class, slot, function, what);
    let mut points = vec![
        Point::observe(CARRY_OUT, "48 89 5c 24 18 48 89 6c 24 20 56 57 41 56 48 83", "diplomatic action carried out", 1, Some(carry_out_before), None),
        method(ACTION_COMMAND, command::CARRY_OUT, CARRY_OUT, "diplomatic action command execution"),
        method(ACTION_COMMAND, command::DESTROY, COMMAND_DESTROY.rva(), "diplomatic action command destructor"),
        Point::keep(
            COMMAND_DESTROY,
            "48 89 5c 24 08 57 48 83 ec 20 8b fa 48 8b d9 48 8d 05 2a a0 79 01 48 89 01 48 8b 49 50",
            "diplomatic action command destructor",
        )
        .uses(&[ACTION_COMMAND, command::ACTION]),
        // Making the command, which every kind inherits.
        Point::keep(0x4e9930usize, "48 89 5c 24 08 57 48 83 ec 20 48 8b 01 48 8b fa ff 90 b0 00 00 00", "diplomatic action command maker"),
        method(ALLIANCE, action::MAKE_COMMAND, 0x4e9930, "alliance command"),
        method(DECLARE_WAR, action::MAKE_COMMAND, 0x4e9930, "declaration of war command"),
        method(PEACE, action::MAKE_COMMAND, 0x4e9930, "peace offer command"),
        // Each kind's checks and destructor.
        method(ALLIANCE, action::ALLOWED, 0x512c70, "alliance check"),
        Point::keep(0x512c70usize, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 40 48 8b da 48 8b f1 e8 76 70 fd ff", "alliance check"),
        method(ALLIANCE, action::OFFERED, 0x5129d0, "alliance panel check"),
        Point::keep(0x5129d0usize, "48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57 48 81 ec c0 00 00 00", "alliance panel check"),
        method(ALLIANCE, action::DESTROY, 0x129450, "alliance destructor"),
        Point::keep(0x129450usize, "40 53 48 83 ec 20 48 8d 05 1b 51 b1 01 48 8b d9 48 89 01 f6 c2 01", "alliance destructor"),
        method(DECLARE_WAR, action::ALLOWED, 0x50a0a0, "declaration of war check"),
        Point::keep(0x50a0a0usize, "48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 90 48 81 ec 70 01 00 00", "declaration of war check"),
        method(DECLARE_WAR, action::OFFERED, 0x509e30, "declaration of war panel check"),
        Point::keep(0x509e30usize, "48 89 5c 24 08 57 48 83 ec 40 48 8b da 48 8b 49 10 e8 aa 00 00 00", "declaration of war panel check"),
        method(DECLARE_WAR, action::DESTROY, 0x500520, "declaration of war destructor"),
        Point::keep(0x500520usize, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 2c 00 00 00 f6 c3 01", "declaration of war destructor"),
        method(PEACE, action::ALLOWED, 0x59b630, "peace offer check"),
        Point::keep(0x59b630usize, "48 89 5c 24 08 48 89 6c 24 10 56 57 41 56 48 83 ec 60 4c 8b f2 48 8b d9", "peace offer check"),
        method(PEACE, action::OFFERED, 0x59b8d0, "peace offer panel check"),
        Point::keep(0x59b8d0usize, "48 89 5c 24 08 57 48 83 ec 40 48 8b da 40 b7 01", "peace offer panel check"),
        method(PEACE, action::DESTROY, 0x598b60, "peace offer destructor"),
        Point::keep(0x598b60usize, "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 8b da 48 83 c1 48", "peace offer destructor").uses(&[action::TREATY]),
        // The descriptions the checks append last.
        Point::keep(0x512cc3usize, "48 8d 15 f6 10 77 01", "alliance description").uses(&[ALLIANCE_DESCRIPTION]),
        Point::keep(0x59b86dusize, "48 8d 15 4c f3 6e 01", "peace offer description").uses(&[PEACE_DESCRIPTION]),
        Point::keep(0x50a706usize, "48 8d 15 8b 8d 77 01", "declaration of war description").uses(&[WAR_DESCRIPTION]),
        Point::keep(0x509e9ausize, "48 8d 15 f7 95 77 01", "declaration of war panel description").uses(&[WAR_DESCRIPTION]),
        // The constructors.
        Point::keep(DECLARE_WAR_NEW, "48 89 5c 24 10 48 89 4c 24 08 57 48 83 ec 20 48 8b d9", "declaration of war constructor"),
        // Calls to arms: what the declare war window gathers and its lists'
        // destructor, the check its call box asks and the reason it gives
        // when the ally is on the other side too, and a vector of tags
        // growing.
        Point::keep(GATHER, "44 88 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53 56 57 41 54 41 55 41 56 41 57", "countries gathered"),
        Point::keep(SIDES_FREE, "40 53 48 83 ec 20 48 8b d9 48 8b 09 48 85 c9 74 54", "countries gathered freed"),
        Point::keep(MAY_CALL, "48 89 5c 24 18 48 89 54 24 10 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57", "ally check"),
        Point::keep(0xee0d44usize, "48 8d 15 95 e8 e3 00", "ally check allied to enemy").uses(&[ALLIED_TO_ENEMY]),
        Point::keep(TAGS_GROW, "48 89 5c 24 18 56 57 41 56 48 83 ec 20 48 8b 41 08 48 8b da 48 8b 11", "tags grown"),
        // Calls to arms: the call's checks, command and destructor.
        method(CALL, action::MAKE_COMMAND, 0x4e9930, "call to arms command"),
        method(CALL, action::ALLOWED, 0x5426d0, "call to arms check"),
        Point::keep(0x5426d0usize, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 57 41 56 41 57", "call to arms check"),
        method(CALL, action::OFFERED, 0x4e9b10, "call to arms panel check"),
        Point::keep(0x4e9b10usize, "48 89 5c 24 10 48 89 6c 24 18 56 57 41 56 48 81 ec 90 00 00", "call to arms panel check"),
        method(CALL, action::DESTROY, 0x506670, "call to arms destructor"),
        Point::keep(0x506670usize, "48 89 5c 24 08 57 48 83 ec 20 48 8b f9 8b da 48 83 c1 58 e8", "call to arms destructor"),
        // Whether the AI accepts an action, with its factors, which every
        // kind inherits, and the factors' destructor.
        method(PEACE, action::ACCEPTS, ACCEPTS.rva(), "peace offer acceptance"),
        method(CALL, action::ACCEPTS, ACCEPTS.rva(), "call to arms acceptance"),
        Point::keep(ACCEPTS, "48 89 5c 24 08 48 89 74 24 18 57 48 83 ec 20 48 85 d2 48 8b da 48 8b f9", "diplomatic action acceptance"),
        Point::keep(FACTORS_FREE, "48 89 5c 24 10 57 48 83 ec 20 48 8b 19 48 8b f9 48 85 db 74 7f 48 89 74", "factors freed"),
        // The war goals and their list.
        Point::keep(WAR_GOAL_LIST, "48 89 5c 24 10 4c 89 4c 24 20 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 50", "war goals offered"),
        method(WAR_GOALS, 0, WAR_GOAL_LIST_DESTROY.rva(), "war goal list destructor"),
        Point::keep(WAR_GOAL_LIST_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 bc 14 00 00", "war goal list destructor"),
    ];
    points.extend([
        Point::keep(CAN_RIVAL, "48 89 5c 24 08 48 89 54 24 10 55 56 57 48 83 ec 70 49 8b e8", "whether a country may rival another"),
        Point::keep(CAN_DROP_RIVAL, "48 89 5c 24 18 48 89 54 24 10 55 56 57 41 54 41 55 41 56 41", "whether a country may drop a rival"),
        // The commands' checks ask them, with no reasons.
        Point::keep(0x2ab7b1usize, "48 8b 51 58 45 33 c0 49 8b c9 e8 a0 6f 12 00", "rival command check"),
        Point::keep(0x2ad425usize, "45 33 c0 49 8b 51 58 e8 3f 55 12 00", "rival dropped command check"),
    ]);
    points.extend(acts::points());
    points.extend(panel::points());
    points
}

/// The refusals the framework words itself in diplomacy; a country is named
/// as the game names it, by its tag.
mod checks {
    use crate::game::{Fail, Status};

    use std::fmt::Display;

    /// The refusal of the player's own country where diplomacy takes another.
    pub fn own_country() -> Fail {
        Fail::new(Status::InvalidArgument, "the player's own country")
    }

    /// The refusal of an empty casus belli; `none` is how a declaration without
    /// one is named.
    pub fn no_casus_belli(none: &str) -> Fail {
        Fail::new(Status::InvalidArgument, format!("no casus belli: name one by its key, or {none}"))
    }

    /// The refusal of a war goal the game does not offer the player under
    /// `casus_belli` against `enemy`.
    pub fn no_such_war_goal(casus_belli: &str, enemy: impl Display) -> Fail {
        Fail::new(Status::Rejected, format!("the game offers no such war goal under {casus_belli} against {enemy}; diplomacy.war_goals lists those it offers"))
    }

    /// The refusal of a country to call to arms that the declare war window
    /// does not list among the allies.
    pub fn not_listed(ally: impl Display) -> Fail {
        Fail::new(Status::InvalidArgument, format!("{ally} is not among the allies the declare war window lists"))
    }

    /// The refusal of an ally named to call to arms that the declare war window
    /// does not let the player call, with its check's `reasons`, one a line.
    pub fn not_callable(ally: impl Display, reasons: &[String]) -> Fail {
        let call = format!("the declare war window does not let the player call {ally} to arms");
        Fail::new(Status::Rejected, if reasons.is_empty() { format!("{call}, and gives no reason") } else { format!("{call}:\n{}", reasons.join("\n")) })
    }

    /// The refusal of a ticket that names no diplomatic action kept.
    pub fn not_kept(ticket: u64) -> Fail {
        Fail::new(Status::NotFound, format!("no diplomatic action {ticket} is kept"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn an_ally_not_callable_gives_the_window_s_reasons() {
            assert_eq!(
                not_callable("CND", &["CND would not accept".into()]).message,
                "the declare war window does not let the player call CND to arms:\nCND would not accept"
            );
            assert_eq!(not_callable("DUL", &[]).message, "the declare war window does not let the player call DUL to arms, and gives no reason");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::game_tag;

    const ALLIANCE: Kind = Kind { token: ALLIANCE_KIND, offer: true };
    const WAR: Kind = Kind { token: DECLARE_WAR_KIND, offer: true };
    const PEACE: Kind = Kind { token: PEACE_KIND, offer: true };

    /// The declare war window as a test sets it up: the allies it lists, in
    /// its order, whether each joins anyway and whether its check lets the
    /// player call it; the countries on the other side and whether each
    /// joins; and which ally drops out of the list once another is called.
    #[derive(Default)]
    struct Boxes {
        ours: Vec<(Tag, bool, bool)>,
        theirs: Vec<(Tag, bool)>,
        drops: Vec<(Tag, Tag)>,
        called: Vec<Tag>,
    }

    fn side(country: Tag, joins: bool) -> Side {
        let mut bytes = [0u8; side::SIZE];
        bytes[side::COUNTRY..side::COUNTRY + 8].copy_from_slice(&country.0.to_le_bytes());
        bytes[side::JOINS] = joins as u8;
        Side(bytes)
    }

    impl CallBoxes for Boxes {
        fn gather(&mut self) -> GResult<(Vec<Side>, Vec<Side>)> {
            let gone: Vec<Tag> = self.drops.iter().filter(|(after, _)| self.called.contains(after)).map(|&(_, gone)| gone).collect();
            let ours = self.ours.iter().filter(|(t, ..)| !gone.contains(t)).map(|&(t, joins, _)| side(t, joins)).collect();
            Ok((ours, self.theirs.iter().map(|&(t, joins)| side(t, joins)).collect()))
        }

        fn ai_plays(&mut self, _: Tag) -> GResult<bool> {
            Ok(true)
        }

        fn may_call(&mut self, country: &Side, ally: bool) -> GResult<bool> {
            Ok(ally && self.ours.iter().any(|&(t, _, may)| t == country.country() && may))
        }

        fn why_not(&mut self, ally: &Side) -> GResult<Vec<String>> {
            Ok(vec![format!("{} would not accept", letters(ally.country()))])
        }

        fn allied_to_enemy(&mut self) -> GResult<Vec<String>> {
            Ok(vec!["allied to the enemy".into()])
        }

        fn call(&mut self, ally: Tag) -> GResult<()> {
            self.called.push(ally);
            Ok(())
        }
    }

    fn tags() -> (Tag, Tag, Tag, Tag, Tag) {
        (game_tag(b"TUR", 1), game_tag(b"BYZ", 33), game_tag(b"DUL", 2), game_tag(b"CND", 3), game_tag(b"GEO", 4))
    }

    #[test]
    fn every_ally_the_window_lets_the_player_call_is_called_and_the_rest_passed_over() {
        let (tur, byz, dul, cnd, geo) = tags();
        // The window lists the player and the enemy too, and an ally that
        // joins anyway, which has no call box.
        let mut boxes = Boxes { ours: vec![(tur, true, false), (dul, false, true), (cnd, false, false), (geo, true, true)], ..Default::default() };
        boxes.theirs = vec![(byz, true)];
        let passed_over = vec![PassedOver { country: cnd, reasons: vec!["CND would not accept".into()] }];
        assert_eq!(call_allies(&mut boxes, tur, byz, &Calls::All).unwrap(), (vec![dul], passed_over));
        assert_eq!(boxes.called, [dul]);
        assert_eq!(call_allies(&mut Boxes::default(), tur, byz, &Calls::None).unwrap(), (vec![], vec![]));
    }

    #[test]
    fn an_ally_named_is_called_or_the_declaration_refused_with_the_reason() {
        let (tur, byz, dul, cnd, geo) = tags();
        let window = || Boxes { ours: vec![(dul, false, true), (cnd, false, false), (geo, true, true)], ..Default::default() };
        let mut boxes = window();
        assert_eq!(call_allies(&mut boxes, tur, byz, &Calls::Only(vec![dul])).unwrap(), (vec![dul], vec![]));
        let refused = |calls: Vec<Tag>, theirs: Vec<(Tag, bool)>| {
            let mut boxes = Boxes { theirs, ..window() };
            let fail = call_allies(&mut boxes, tur, byz, &Calls::Only(calls)).unwrap_err();
            (fail.status, fail.message)
        };
        assert_eq!(refused(vec![byz], vec![]).0, Status::InvalidArgument);
        assert_eq!(
            refused(vec![dul, cnd], vec![]),
            (Status::Rejected, "the declare war window does not let the player call CND to arms:\nCND would not accept".into())
        );
        assert_eq!(refused(vec![geo], vec![]).0, Status::Rejected);
        // On the other side too.
        assert_eq!(
            refused(vec![dul], vec![(dul, true)]),
            (Status::Rejected, "the declare war window does not let the player call DUL to arms:\nallied to the enemy".into())
        );
    }

    #[test]
    fn an_ally_the_window_lists_no_more_once_another_is_called_refuses_the_declaration_naming_it_or_is_passed_over() {
        let (tur, byz, dul, cnd, _) = tags();
        let mut boxes = Boxes { ours: vec![(dul, false, true), (cnd, false, true)], drops: vec![(dul, cnd)], ..Default::default() };
        let fail = call_allies(&mut boxes, tur, byz, &Calls::Only(vec![dul, cnd])).unwrap_err();
        assert_eq!(fail.status, Status::Rejected);
        assert_eq!(fail.message, "the declare war window no longer lists CND among the allies once DUL is called to arms, so the player could not call it");
        // Under every ally, it is passed over with that reason.
        let mut boxes = Boxes { ours: vec![(dul, false, true), (cnd, false, true)], drops: vec![(dul, cnd)], ..Default::default() };
        let gone = PassedOver {
            country: cnd,
            reasons: vec!["the declare war window no longer lists CND among the allies once DUL is called to arms, so the player could not call it".into()],
        };
        assert_eq!(call_allies(&mut boxes, tur, byz, &Calls::All).unwrap(), (vec![dul], vec![gone]));
        // Calling another by name, it is only not called.
        let mut boxes = Boxes { ours: vec![(dul, false, true), (cnd, false, true)], drops: vec![(dul, cnd)], ..Default::default() };
        assert_eq!(call_allies(&mut boxes, tur, byz, &Calls::Only(vec![dul])).unwrap(), (vec![dul], vec![]));
    }

    #[test]
    fn a_check_s_reasons_are_its_bulleted_lines_and_the_description_the_rest() {
        // As the checks write them: a reason a line, colored and bulleted,
        // then the description.
        let alliance = "\u{a7}R- Cannot ally with a rival.\u{a7}!\n\u{a7}R- You have no diplomats to send.\u{a7}!\nIf a country accepts our Alliance offer it may come to our assistance during wars.";
        let (reasons, description) = said(&game_text::from_str(alliance));
        assert_eq!(reasons, ["Cannot ally with a rival.", "You have no diplomats to send."]);
        assert_eq!(description, ["If a country accepts our Alliance offer it may come to our assistance during wars."]);
        assert_eq!(refusal(&reasons), "Cannot ally with a rival.\nYou have no diplomats to send.");
        assert_eq!(refusal(&[]), "the game does not allow it, and gives no reason");
        // Icons as their names, as everywhere in game text read plain.
        assert_eq!(text_lines(&game_text::from_str("\u{a7}R- \u{a3}icon_dip\u{a3} too low\u{a7}!")), ["dip too low"]);
        assert_eq!(said(&game_text::from_str("\u{a7}R- \u{a3}icon_dip\u{a3} too low\u{a7}!")).0, ["dip too low"]);
    }

    #[test]
    fn a_choice_picks_the_first_war_goal_with_all_it_gives() {
        let goal = |cb: Option<&str>, wg: &str, province: Option<u32>| WarGoal {
            casus_belli: cb.map(str::to_string),
            war_goal: Some(wg.into()),
            province,
            target: None,
            name: String::new(),
        };
        let offered = [goal(Some("cb_claim"), "take_claim", Some(227)), goal(Some("cb_claim"), "take_claim", Some(229)), goal(None, "no_cb", None)];
        let pick = |choice: Choice| offered.iter().position(|g| choice.matches(g));
        let claim = Choice { casus_belli: Some("cb_claim".into()), ..Default::default() };
        assert_eq!(pick(claim.clone()), Some(0));
        assert_eq!(pick(Choice { province: Some(229), ..claim.clone() }), Some(1));
        assert_eq!(pick(Choice { war_goal: Some("take_core".into()), ..claim.clone() }), None);
        assert_eq!(pick(Choice { province: Some(1), ..claim }), None);
        assert_eq!(pick(Choice::default()), Some(2));
        assert_eq!(pick(Choice { target: Some(game_tag(b"BYZ", 33)), ..Choice::default() }), None);
    }

    #[test]
    fn an_action_is_answered_by_what_the_game_carries_out_after_it() {
        let (byz, ven) = (game_tag(b"BYZ", 33), game_tag(b"VEN", 40));
        let mut l = Ledger::default();
        // Carried out before it was sent: not its.
        l.carried_out(ALLIANCE, byz, action::DECLINED);
        let alliance = l.send(ALLIANCE, true, game_tag(b"TUR", 154), byz, l.carried_count, 1, Vec::new());
        let war = l.send(WAR, false, game_tag(b"TUR", 154), ven, l.carried_count, 1, Vec::new());
        assert_eq!(l.seen(alliance).unwrap().1, Seen::Nothing);
        l.carried_out(ALLIANCE, byz, action::UNANSWERED);
        l.carried_out(ALLIANCE, ven, action::ACCEPTED);
        assert_eq!(l.seen(alliance).unwrap().1, Seen::CarriedOut);
        assert_eq!(l.seen(war).unwrap().1, Seen::Nothing);
        l.carried_out(WAR, ven, action::UNANSWERED);
        l.carried_out(ALLIANCE, byz, action::ACCEPTED);
        assert_eq!(l.seen(alliance).unwrap(), (l.sent[0].clone(), Seen::Answered(true)));
        assert_eq!(l.seen(war).unwrap().1, Seen::CarriedOut);
        // A declaration has no answer.
        l.carried_out(WAR, ven, action::DECLINED);
        assert_eq!(l.seen(war).unwrap().1, Seen::CarriedOut);
        assert_eq!(l.seen(Ticket(99)), None);
    }

    #[test]
    fn an_answer_is_seen_only_while_its_country_is_acted_as() {
        let (tur, byz) = (game_tag(b"TUR", 154), game_tag(b"BYZ", 33));
        assert!(still_acting(tur, Some(tur)).is_ok());
        let other = still_acting(tur, Some(byz)).unwrap_err();
        assert_eq!((other.status, other.message.as_str()), (Status::Rejected, "it was sent as TUR, and BYZ is in view: what comes of it is no longer seen"));
        assert_eq!(still_acting(tur, None).unwrap_err().message, "it was sent as TUR, and no country is in view: what comes of it is no longer seen");
    }

    #[test]
    fn an_earlier_proposal_s_answer_is_not_a_later_one_s() {
        let byz = game_tag(b"BYZ", 33);
        let mut l = Ledger::default();
        // A proposal carried out before this one was sent, its answer after.
        l.carried_out(PEACE, byz, action::UNANSWERED);
        let peace = l.send(PEACE, true, game_tag(b"TUR", 154), byz, l.carried_count, 1, Vec::new());
        l.carried_out(PEACE, byz, action::ACCEPTED);
        assert_eq!(l.seen(peace).unwrap().1, Seen::Nothing);
        l.carried_out(PEACE, byz, action::UNANSWERED);
        assert_eq!(l.seen(peace).unwrap().1, Seen::CarriedOut);
        l.carried_out(PEACE, byz, action::DECLINED);
        assert_eq!(l.seen(peace).unwrap().1, Seen::Answered(false));
    }

    #[test]
    fn only_the_latest_actions_are_kept() {
        let mut l = Ledger::default();
        let first = l.send(PEACE, true, game_tag(b"TUR", 154), game_tag(b"BYZ", 33), 0, 1, Vec::new());
        for _ in 0..KEPT {
            l.send(PEACE, true, game_tag(b"TUR", 154), game_tag(b"BYZ", 33), 0, 1, Vec::new());
            l.carried_out(PEACE, game_tag(b"BYZ", 33), action::UNANSWERED);
        }
        assert_eq!(l.seen(first), None);
        assert_eq!((l.sent.len(), l.carried.len()), (KEPT, KEPT));
    }

    mod through_the_layer {
        use super::super::*;
        use crate::catalogue::fake::{byz, tur, wal};
        use crate::game::{
            buffers::{self, Image, Object},
            commands::record,
            Fail,
        };
        use crate::objects::campaign::{country as country_fields, globals as campaign};
        use std::cell::RefCell;

        thread_local! {
            /// What the game's rival checks answer: whether the player may,
            /// and the reason when not.
            static ANSWER: RefCell<(bool, &'static str)> = const { RefCell::new((true, "")) };
        }

        extern "C" fn may(_: usize, _: usize, reasons: usize) -> usize {
            let (may, reason) = ANSWER.with(|a| *a.borrow());
            if !may && reasons != 0 {
                buffers::write_inline(reasons, reason);
            }
            may as usize
        }

        /// Turkey, at war with Byzantium when `at_war`, among the countries
        /// Turkey and Byzantium, the game's rival checks answering as
        /// [`ANSWER`] says. Runs `action` over Turkey's session.
        fn turkey<T>(at_war: bool, action: impl FnOnce(Session) -> GResult<T>) -> GResult<T> {
            let image = Image::new();
            buffers::strings_kept(&image).function(CAN_RIVAL.rva(), may as *const ()).function(CAN_DROP_RIVAL.rva(), may as *const ());
            for (n, rva) in campaign::SPECIAL_COUNTRIES.into_iter().enumerate() {
                image.i32(rva, 1000 + n as i32);
            }
            let hostile = Object::new(country_fields::HOSTILE_SIZE).i32(0, byz().index().into());
            let count = if at_war { 1 } else { 0 };
            let turkey =
                Object::country(tur(), country_fields::HOSTILE_COUNT + 4).points_to(country_fields::HOSTILE, hostile).i32(country_fields::HOSTILE_COUNT, count);
            let byzantium = Object::country(byz(), country_fields::HOSTILE_COUNT + 4);
            let countries = Object::countries(&[&turkey, &byzantium]);
            let result = action(buffers::session(image.memory(), 0, tur(), turkey.at(), countries.at(), Caller::Remote));
            drop((turkey, byzantium, countries, image));
            result
        }

        fn refused<T: std::fmt::Debug>(result: GResult<T>) -> (Status, String) {
            let Err(Fail { status, message }) = result else { panic!("the action was carried out: {result:?}") };
            (status, message)
        }

        #[test]
        fn diplomacy_takes_another_country_of_the_game_s() {
            let own = (Status::InvalidArgument, "the player's own country".to_string());
            let war = Choice { casus_belli: Some("cb_claim".into()), ..Choice::default() };
            let toward_turkey = [
                refused(turkey(false, |s| war_goals(s, tur()))),
                refused(turkey(false, |s| declare_war(s, tur(), &war, &Calls::None))),
                refused(turkey(false, |s| offer_peace(s, tur()))),
                refused(turkey(false, |s| propose_alliance(s, tur()))),
                refused(turkey(false, |s| add_rival(s, tur()))),
                refused(turkey(false, |s| remove_rival(s, tur()))),
            ];
            assert!(toward_turkey.iter().all(|r| *r == own), "{toward_turkey:?}");
            let unknown = refused(turkey(false, |s| propose_alliance(s, wal())));
            assert_eq!(unknown.0, Status::NotFound, "{unknown:?}");
            assert!(record::posted().is_empty());
        }

        #[test]
        fn a_declaration_names_its_casus_belli_and_a_white_peace_needs_a_war() {
            let none = Choice { casus_belli: Some(String::new()), ..Choice::default() };
            assert_eq!(
                refused(turkey(false, |s| declare_war(s, byz(), &none, &Calls::All))),
                (Status::InvalidArgument, "no casus belli: name one by its key, or none".into())
            );
            assert_eq!(refused(turkey(false, |s| offer_peace(s, byz()))), (Status::Rejected, "the player is not at war with BYZ".into()));
            assert!(record::posted().is_empty());
        }

        #[test]
        fn a_rival_is_added_and_dropped_as_the_game_s_checks_let_the_player() {
            turkey(false, |s| add_rival(s, byz())).unwrap();
            turkey(false, |s| remove_rival(s, byz())).unwrap();
            assert_eq!(record::posted(), [r#"add_rival={ country="TUR" rival="BYZ" }"#, r#"remove_rival={ country="TUR" rival="BYZ" }"#]);
            ANSWER.with(|a| *a.borrow_mut() = (false, "Too far away"));
            assert_eq!(refused(turkey(false, |s| add_rival(s, byz()))), (Status::Rejected, "Too far away".into()));
            ANSWER.with(|a| *a.borrow_mut() = (false, ""));
            assert_eq!(
                refused(turkey(false, |s| remove_rival(s, byz()))),
                (Status::Rejected, "the game does not let the player drop that rival now, and gives no reason".into())
            );
            assert!(record::posted().is_empty());
        }
    }
}
