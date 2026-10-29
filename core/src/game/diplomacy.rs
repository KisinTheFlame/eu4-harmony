//! Diplomacy: the player's diplomatic actions, each sent as the player's
//! click in the diplomacy panel sends it (docs/research/diplomacy.md).
//!
//! The framework makes the game's own diplomatic action, from the player to
//! the other country, and asks it the checks the diplomacy panel and the
//! window that sends it ask; a refusal comes back with the game's reasons.
//! Then it has the action make the command that carries it and posts the
//! command to the game's session, as the window does. The game carries the
//! command out at its next frame, with its own checks, costs, AI answers and
//! multiplayer rules: an AI country answers a proposal there, and its answer
//! comes back as a command of its own, carried out at a frame after. Each
//! action sent is a [`Ticket`]; [`answer`] says what came of it, from the
//! commands the game carried out, which an observer of the game's own
//! function sees.
use super::{context, context_any, countries, country, events, fail, game, map_manager, Caller, Ctx, GResult, Status, Tag};
use crate::{
    game_string,
    hooks::HookFrame,
    memory::Memory,
    objects::{
        diplomacy::{action, casus_belli, classes, command, globals, send_window, side, sides, treaty, war_entry, war_goal, war_goal_type, war_goals, world},
        gui, session,
    },
    points::Point,
    unicode::game_text,
};
use std::collections::VecDeque;
use std::sync::Mutex;

/// The constructor of a declaration of war: (action, from, to, flag, date,
/// war goal), the war goal copied; the flag 0 as the diplomacy panel's.
const DECLARE_WAR_NEW: usize = 0x500480;
/// The constructor of a peace offer: (action, from, to, demand, treaty,
/// date), the treaty copied.
const PEACE_NEW: usize = 0x598af0;
/// A peace treaty's constructor and destructor: (treaty, actor, other,
/// demand), with no terms; (treaty).
const TREATY_NEW: usize = 0x975d40;
const TREATY_DESTROY: usize = 0x9761c0;
/// The war goals the game offers a country against another: (from, to,
/// list, filter), into a [`classes::WAR_GOALS`] list; the filter an empty
/// `std::function`.
const WAR_GOAL_LIST: usize = 0xd9eb40;
/// The list's destructor, its vtable's first method: (list, flags), which
/// destroys the war goals.
const WAR_GOAL_LIST_DESTROY: usize = 0x114b80;
/// Carrying out a diplomatic action's command, its vtable's
/// [`command::CARRY_OUT`]: (command).
const CARRY_OUT: usize = 0x4e7c50;
/// The command's destructor, its vtable's [`command::DESTROY`]: (command,
/// flags), flag 1 freeing it.
const COMMAND_DESTROY: usize = 0x4e7470;
const FREE: usize = 1;
/// The keys of the descriptions the checks append after their reasons.
const ALLIANCE_DESCRIPTION: usize = 0x1c83dc0;
const PEACE_DESCRIPTION: usize = 0x1c8abc0;
const WAR_DESCRIPTION: usize = 0x1c83498;

/// What the declare war window gathers about a declaration of war: (action,
/// out [`sides`], 1 as the window passes it).
const GATHER: usize = 0x503e70;
/// Frees a `std::vector` of [`side`] items: (vector).
const SIDES_FREE: usize = 0x124150;
/// The declare war window's check whether the player may call the country
/// of one of its entries ([`war_entry`]): (entry, action, whether the
/// country is not among those promised land, reasons), the reasons a
/// `std::string` or null. Its call box asks it before it calls an ally.
const MAY_CALL: usize = 0xee03f0;
/// A full `std::vector` of tags growing to take one more, as the call box
/// grows the allies called: (vector, where, the tag's address).
const TAGS_GROW: usize = 0xdfde0;

/// The most war goals the game is believed to offer.
const MOST_WAR_GOALS: usize = 10_000;
/// The most countries the game is believed to gather on one side of a war.
const MOST_ON_A_SIDE: usize = 1_000;
/// How many of the player's actions sent, and carried out, are kept.
const KEPT: usize = 256;

/// The player's diplomatic actions the framework sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    DeclareWar,
    OfferPeace,
    ProposeAlliance,
}

impl Action {
    /// Its class's vtable RVA.
    fn class(self) -> usize {
        match self {
            Self::DeclareWar => classes::DECLARE_WAR,
            Self::OfferPeace => classes::PEACE,
            Self::ProposeAlliance => classes::ALLIANCE,
        }
    }

    /// The key of the description its checks append after their reasons.
    fn description(self) -> usize {
        match self {
            Self::DeclareWar => WAR_DESCRIPTION,
            Self::OfferPeace => PEACE_DESCRIPTION,
            Self::ProposeAlliance => ALLIANCE_DESCRIPTION,
        }
    }

    /// Whether the other country answers it.
    fn proposal(self) -> bool {
        self != Self::DeclareWar
    }
}

/// How remote clients and extensions name a declaration of war without a
/// casus belli, where they name a casus belli by its key.
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

/// A declaration of war sent: its ticket, and the allies it calls to arms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub ticket: Ticket,
    pub called: Vec<Tag>,
}

/// What came of an action sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
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
pub fn war_goals(_: Caller, enemy: Tag) -> GResult<Vec<WarGoal>> {
    let m = game()?;
    let ctx = context(m)?;
    other(m, &ctx, enemy)?;
    let offered = Offered::list(m, &ctx, enemy)?;
    Ok(offered.goals.iter().map(|(_, goal)| goal.clone()).collect())
}

/// Declares war on `enemy` for the war goal `choice` picks among those the
/// game offers, calling to arms, when `call_allies`, each ally the declare
/// war window would let the player call ([`call_allies`]), and otherwise
/// none. [`Status::InvalidArgument`] for an empty casus belli;
/// [`Status::Rejected`] when the game offers no such war goal, or does not
/// let the player declare war now, with its reasons.
pub fn declare_war(_: Caller, enemy: Tag, choice: &Choice, call_allies: bool) -> GResult<Declaration> {
    let m = game()?;
    let ctx = context(m)?;
    other(m, &ctx, enemy)?;
    if choice.casus_belli.as_deref() == Some("") {
        return fail(Status::InvalidArgument, format!("no casus belli: name one by its key, or {NO_CASUS_BELLI}"));
    }
    let offered = Offered::list(m, &ctx, enemy)?;
    let Some(&(goal, _)) = offered.goals.iter().find(|(_, g)| choice.matches(g)) else {
        let cb = choice.casus_belli.as_deref().unwrap_or(NO_CASUS_BELLI);
        return fail(Status::Rejected, format!("the game offers no such war goal under {cb} against {}; diplomacy.war_goals lists those it offers", letters(enemy)));
    };
    let mut made = Made::new(m, action::DECLARE_WAR_SIZE);
    m.call(m.at(DECLARE_WAR_NEW), &[made.at(), ctx.player.0 as usize, enemy.0 as usize, 0, ctx.date as u32 as usize, goal])?;
    made.built = true;
    verdict(m, made.at(), Action::DeclareWar)?;
    let called = if call_allies { self::call_allies(m, &ctx, made.at(), enemy)? } else { Vec::new() };
    let ticket = send(m, &ctx, made.at(), Action::DeclareWar, enemy, called.clone())?;
    Ok(Declaration { ticket, called })
}

/// Offers `enemy` a white peace: a peace treaty with no terms, demanded by
/// the player, in the war between them, as the peace view sends one.
pub fn offer_peace(_: Caller, enemy: Tag) -> GResult<Ticket> {
    let m = game()?;
    let ctx = context(m)?;
    other(m, &ctx, enemy)?;
    // The game's check refuses an offer to a country the player is not at
    // war with without a reason: it looks for the war between them first.
    if !super::world::hostile(m, &ctx, ctx.player, enemy)? {
        return fail(Status::Rejected, format!("the player is not at war with {}", letters(enemy)));
    }
    let treaty = vec![0u64; treaty::SIZE / 8];
    let at = treaty.as_ptr() as usize;
    m.call(m.at(TREATY_NEW), &[at, ctx.player.0 as usize, enemy.0 as usize, 1])?;
    let mut made = Made::new(m, action::PEACE_SIZE);
    let built = m.call(m.at(PEACE_NEW), &[made.at(), ctx.player.0 as usize, enemy.0 as usize, 1, at, ctx.date as u32 as usize]);
    // The action holds a copy of the treaty.
    m.call(m.at(TREATY_DESTROY), &[at])?;
    built?;
    made.built = true;
    verdict(m, made.at(), Action::OfferPeace)?;
    send(m, &ctx, made.at(), Action::OfferPeace, enemy, Vec::new())
}

/// Proposes an alliance to `ally`.
pub fn propose_alliance(_: Caller, ally: Tag) -> GResult<Ticket> {
    let m = game()?;
    let ctx = context(m)?;
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
    verdict(m, made.at(), Action::ProposeAlliance)?;
    send(m, &ctx, made.at(), Action::ProposeAlliance, ally, Vec::new())
}

/// What came of an action sent. [`Status::NotFound`] for a ticket that is
/// no longer kept; [`Status::NotInGame`] once the game it was sent in has
/// ended; [`Status::Rejected`] for a declaration of war the game carried
/// out without the war beginning.
pub fn answer(caller: Caller, ticket: Ticket) -> GResult<Answer> {
    let m = game()?;
    let ctx = context(m)?;
    let (sent, seen) = kept(ticket)?;
    Ok(match seen {
        Seen::Answered(true) => Answer::Accepted,
        Seen::Answered(false) => Answer::Declined,
        // A declaration has no answer: it is done once carried out.
        Seen::CarriedOut if !sent.action.proposal() => match war_with(caller, &ctx, sent.to)? {
            Some(name) => Answer::Declared(name),
            None => return fail(Status::Rejected, format!("the game carried the declaration out, but no war with {} began", letters(sent.to))),
        },
        Seen::CarriedOut | Seen::Nothing => Answer::Pending,
    })
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
    let (sent, seen) = ledger(|l| l.seen(ticket)).ok_or_else(|| super::Fail { status: Status::NotFound, message: format!("no diplomatic action {} is kept", ticket.0) })?;
    if sent.session != events::session() {
        return fail(Status::NotInGame, "the game the diplomatic action was sent in has ended");
    }
    Ok((sent, seen))
}

/// The name of the player's latest war with `enemy` on the other side.
fn war_with(caller: Caller, ctx: &Ctx, enemy: Tag) -> GResult<Option<String>> {
    let player = countries::snapshot(caller, ctx.player)?;
    let opposed = |ours: &[Tag], theirs: &[Tag]| ours.iter().any(|t| t.index() == ctx.player.index()) && theirs.iter().any(|t| t.index() == enemy.index());
    Ok(player.wars.into_iter().rev().find(|w| opposed(&w.attackers, &w.defenders) || opposed(&w.defenders, &w.attackers)).map(|w| w.name))
}

/// A country of the game's other than the player's.
fn other(m: &Memory, ctx: &Ctx, tag: Tag) -> GResult<()> {
    country(m, ctx, tag)?;
    if tag.index() == ctx.player.index() {
        return fail(Status::InvalidArgument, "the player's own country");
    }
    Ok(())
}

fn letters(tag: Tag) -> String {
    String::from_utf8_lossy(&tag.letters()).into_owned()
}

// ---------- sending ----------

/// An action the framework makes in its own memory, which the game's code
/// fills in and copies; destroyed, without freeing, once sent.
struct Made<'a> {
    m: &'a Memory,
    object: Vec<u64>,
    /// Its constructor ran, so its destructor must.
    built: bool,
}

impl<'a> Made<'a> {
    fn new(m: &'a Memory, size: usize) -> Self {
        Self { m, object: vec![0; size.div_ceil(8)], built: false }
    }

    fn at(&self) -> usize {
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
/// ([`verdict`]): posts its command to the session as the diplomacy window
/// does, and keeps it for [`answer`], with the allies it calls.
fn send(m: &Memory, ctx: &Ctx, at: usize, kind: Action, to: Tag, called: Vec<Tag>) -> GResult<Ticket> {
    // The session before the command: once the command is made, only a call
    // into the game can fail before it is posted or destroyed. The session
    // owns the command once it took it; until then, it is the framework's to
    // free.
    let session = session_of(m, ctx)?;
    let mut command = 0usize;
    m.call(m.vslot(at, action::MAKE_COMMAND)?, &[at, &mut command as *mut usize as usize])?;
    if command == 0 || m.ptr(command)? != m.at(classes::ACTION_COMMAND) {
        return Err("the diplomatic action made no command".to_string().into());
    }
    let after = ledger(|l| l.carried_count);
    let taken = m.call(m.at(events::DISPATCH), &[session, command])? as u8 != 0;
    if !taken {
        m.call(m.at(COMMAND_DESTROY), &[command, FREE])?;
        return fail(Status::Rejected, "the game takes no orders from the player now");
    }
    // As the window, so that the panel lets no second one go the same day.
    let record = match kind {
        Action::ProposeAlliance => Some((world::ALLIANCE_OFFER_FROM, world::ALLIANCE_OFFER_DATE)),
        Action::OfferPeace => Some((world::PEACE_OFFER_FROM, world::PEACE_OFFER_DATE)),
        Action::DeclareWar => None,
    };
    if let Some((from, date)) = record {
        m.write(ctx.world + from, &ctx.player.0.to_le_bytes())?;
        m.write(ctx.world + date, &ctx.date.to_le_bytes())?;
    }
    Ok(ledger(|l| l.send(kind, to, after, events::session(), called)))
}

/// The map manager's session, which takes the player's commands.
fn session_of(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    let s = m.call(m.at(session::OF_MAP_MANAGER), &[map_manager(m, ctx)?])?;
    if s == 0 || m.ptr(s)? != m.at(session::CLASS) {
        return Err("the map manager has no session".to_string().into());
    }
    Ok(s)
}

/// Asks the action the checks the diplomacy panel and the window ask before
/// the player can send it; [`Status::Rejected`] with the game's reasons when
/// either says no.
fn verdict(m: &Memory, at: usize, kind: Action) -> GResult<()> {
    let ask = |slot: usize| -> GResult<bool> { Ok(m.call(m.vslot(at, slot)?, &[at, 0, 0])? as u8 != 0) };
    let checks = [(action::OFFERED, ask(action::OFFERED)?), (action::ALLOWED, ask(action::ALLOWED)?)];
    if checks.iter().all(|&(_, allowed)| allowed) {
        return Ok(());
    }
    let mut said = Vec::new();
    for (slot, allowed) in checks {
        if !allowed {
            said.extend(text_lines(&reasons(m, at, slot)?));
        }
    }
    let description = text_lines(&game_string::localized(m, m.at(kind.description()))?);
    fail(Status::Rejected, refusal(said, &description))
}

/// What a check appends to a `std::string`.
fn reasons(m: &Memory, at: usize, slot: usize) -> GResult<Vec<u8>> {
    let mut string = game_string::empty();
    let s = string.as_mut_ptr() as usize;
    let called = m.call(m.vslot(at, slot)?, &[at, s, 0]);
    let text = game_string::read(m, s);
    game_string::destroy_checked(m, s)?;
    called?;
    Ok(text?)
}

/// Game text's lines as plain Unicode, a reason's bullet left out.
fn text_lines(text: &[u8]) -> Vec<String> {
    game_text::plain(text).lines().map(|l| l.trim()).map(|l| l.strip_prefix("- ").unwrap_or(l).trim()).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// The refusal's message: the reasons the checks gave, each once, without
/// the action's description, which they append after them.
fn refusal(said: Vec<String>, description: &[String]) -> String {
    let mut reasons: Vec<String> = Vec::new();
    for line in said {
        if !description.contains(&line) && !reasons.contains(&line) {
            reasons.push(line);
        }
    }
    if reasons.is_empty() {
        "the game does not allow it, and gives no reason".into()
    } else {
        reasons.join("\n")
    }
}

// ---------- calls to arms ----------

/// Calls each ally the declare war window would let the player call to
/// arms, as the player ticking its call boxes one after another from the top
/// calls them, and answers the allies called. The window lists as allies
/// the countries it gathers on the declarer's side ([`Sides`]) but the
/// player and `enemy`; one that joins anyway has no call box to tick. Its
/// call box asks the window's check ([`MAY_CALL`]) with the ally not among
/// those promised land, as none is while the promise land boxes are left
/// alone, and calls an ally the check allows by putting it into the
/// declaration's [`action::CALLED`]. The window gathers again after each
/// call, so this does too. The check reads the window's enemies list,
/// which [`Asking`] leaves empty; [`asked_by_enemy`] stands for it.
fn call_allies(m: &Memory, ctx: &Ctx, action: usize, enemy: Tag) -> GResult<Vec<Tag>> {
    let asking = Asking::new(m, action);
    let mut asked: Vec<i16> = Vec::new();
    let mut called = Vec::new();
    for _ in 0..MOST_ON_A_SIDE {
        let sides = Sides::gather(m, action)?;
        let listed = |s: &Side| ![ctx.player.index(), enemy.index()].contains(&s.country().index());
        let ours = sides.side(sides::DECLARER)?;
        let Some(ally) = ours.into_iter().filter(listed).find(|s| !asked.contains(&s.country().index())) else {
            return Ok(called);
        };
        asked.push(ally.country().index());
        if ally.joins() {
            continue;
        }
        let theirs: Vec<Side> = sides.side(sides::OTHER)?.into_iter().filter(listed).collect();
        drop(sides);
        if !asked_by_enemy(m, ctx, &asking, &theirs, ally.country())? && asking.may_call(&ally, true)? {
            push_tag(m, action + action::CALLED, ally.country())?;
            called.push(ally.country());
        }
    }
    Err("the declare war window's allies kept growing".to_string().into())
}

/// Whether the declare war window lists `ally` among the enemies too, as a
/// country that joins that side anyway or one the check lets the other side
/// call: the window then shows it accepting there, and its check lets the
/// player call it no more (`ALLIED_TO_ENEMY`).
fn asked_by_enemy(m: &Memory, ctx: &Ctx, asking: &Asking, theirs: &[Side], ally: Tag) -> GResult<bool> {
    for enemy in theirs.iter().filter(|s| s.country().index() == ally.index()) {
        if enemy.joins() || (ai_plays(m, ctx, ally)? && asking.may_call(enemy, false)?) {
            return Ok(true);
        }
    }
    Ok(false)
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
        m.call(m.at(TAGS_GROW), &[vector, end, &value as *const u64 as usize])?;
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
        m.call(m.at(GATHER), &[action, sides.raw.as_mut_ptr() as usize, 1])?;
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
            if let Err(e) = self.m.call(self.m.at(SIDES_FREE), &[at]) {
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
extern "C" fn list_box_of(window: usize, _name: usize) -> usize {
    window
}

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
        let mut entry = [0u8; war_entry::SIZE];
        entry[war_entry::WINDOW..war_entry::WINDOW + 8].copy_from_slice(&(self.window.as_ptr() as u64).to_le_bytes());
        entry[war_entry::SIDE..war_entry::SIDE + side::SIZE].copy_from_slice(&country.0);
        entry[war_entry::ALLY] = ally as u8;
        entry[war_entry::PROMISE_BOX] = 1;
        let not_promised_land = 1;
        Ok(self.m.call(self.m.at(MAY_CALL), &[entry.as_ptr() as usize, self.action, not_promised_land, 0])? as u8 != 0)
    }
}

// ---------- the war goals offered ----------

/// The war goals the game offers, in a list of its own that owns them; freed
/// when dropped.
struct Offered<'a> {
    m: &'a Memory,
    list: Box<[u64; war_goals::SIZE / 8]>,
    /// Each war goal's address and what it is, the declaration without a
    /// casus belli last.
    goals: Vec<(usize, WarGoal)>,
}

impl<'a> Offered<'a> {
    fn list(m: &'a Memory, ctx: &Ctx, enemy: Tag) -> GResult<Self> {
        let mut list = Box::new([0u64; war_goals::SIZE / 8]);
        list[0] = m.at(classes::WAR_GOALS) as u64;
        let mut offered = Self { m, list, goals: Vec::new() };
        // An empty `std::function`: no filter.
        let filter = [0u64; 8];
        m.call(m.at(WAR_GOAL_LIST), &[ctx.player.0 as usize, enemy.0 as usize, offered.at(), filter.as_ptr() as usize])?;
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

impl Drop for Offered<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.m.call(self.m.at(WAR_GOAL_LIST_DESTROY), &[self.at(), 0]) {
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
        Ok(Some(game_text::to_utf8(&game_string::read(m, object + at)?)))
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
    let mut string = game_string::empty();
    let s = string.as_mut_ptr() as usize;
    m.call(m.vslot(goal, war_goal::NAME)?, &[goal, s])?;
    let text = game_string::read(m, s);
    game_string::destroy_checked(m, s)?;
    Ok(game_text::plain(&text?))
}

// ---------- what came of them ----------

/// An action sent.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Sent {
    ticket: Ticket,
    action: Action,
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
    action: Action,
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
    fn send(&mut self, action: Action, to: Tag, after: u64, session: u64, called: Vec<Tag>) -> Ticket {
        self.tickets += 1;
        let ticket = Ticket(self.tickets);
        if self.sent.len() == KEPT {
            self.sent.pop_front();
        }
        self.sent.push_back(Sent { ticket, action, to, after, session, called });
        ticket
    }

    fn carried_out(&mut self, action: Action, to: Tag, state: i32) {
        self.carried_count += 1;
        if self.carried.len() == KEPT {
            self.carried.pop_front();
        }
        self.carried.push_back(Carried { number: self.carried_count, action, to, state });
    }

    /// The action sent with `ticket`, and what the game did with it, from
    /// what it carried out of the same kind to the same country since: the
    /// first proposal or declaration, then, for a proposal, the first answer
    /// after it. An answer before that is an earlier proposal's, carried out
    /// before this one was sent.
    fn seen(&self, ticket: Ticket) -> Option<(Sent, Seen)> {
        let sent = self.sent.iter().find(|s| s.ticket == ticket)?.clone();
        let mut seen = Seen::Nothing;
        for c in self.carried.iter().filter(|c| c.number > sent.after && c.action == sent.action && c.to.index() == sent.to.index()) {
            match (seen, c.state) {
                (Seen::Nothing, action::UNANSWERED) => seen = Seen::CarriedOut,
                (Seen::CarriedOut, action::ACCEPTED | action::DECLINED) if sent.action.proposal() => return Some((sent, Seen::Answered(c.state == action::ACCEPTED))),
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
/// player's actions of the kinds the framework sends.
extern "C" fn carry_out_before(frame: *mut HookFrame) {
    crate::guarded("diplomatic action", || {
        let Some(m) = crate::memory() else { return };
        if crate::saves::load_in_progress() {
            return;
        }
        let command = unsafe { (*frame).args[0] };
        let found = (|| -> crate::Result<Option<(Action, Tag, i32)>> {
            let a = m.ptr(command + command::ACTION)?;
            let class = m.ptr(a)?;
            let Some(kind) = [Action::DeclareWar, Action::OfferPeace, Action::ProposeAlliance].into_iter().find(|k| m.at(k.class()) == class) else {
                return Ok(None);
            };
            let Ok(ctx) = context_any(m) else { return Ok(None) };
            if Tag(m.u64(a + action::FROM)?).index() != ctx.player.index() {
                return Ok(None);
            }
            Ok(Some((kind, Tag(m.u64(a + action::TO)?), m.i32(a + action::STATE)?)))
        })();
        match found {
            Ok(Some((kind, to, state))) => ledger(|l| l.carried_out(kind, to, state)),
            Ok(None) => {}
            Err(e) => log!("diplomacy: reading a command carried out: {e}"),
        }
    });
}

/// The functions diplomacy calls and observes; the objects are in
/// `objects::diplomacy`.
pub fn points() -> Vec<Point> {
    use classes::*;
    let method = |class: usize, slot: usize, function: usize, what: &'static str| Point::method(class, slot, function, what);
    vec![
        Point::observe(CARRY_OUT, "48 89 5c 24 18 48 89 6c 24 20 56 57 41 56 48 83", "diplomatic action carried out", 1, Some(carry_out_before), None),
        method(ACTION_COMMAND, command::CARRY_OUT, CARRY_OUT, "diplomatic action command execution"),
        method(ACTION_COMMAND, command::DESTROY, COMMAND_DESTROY, "diplomatic action command destructor"),
        Point::keep(COMMAND_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 8b fa 48 8b d9 48 8d 05 2a a0 79 01 48 89 01 48 8b 49 50", "diplomatic action command destructor")
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
        Point::keep(PEACE_NEW, "48 89 4c 24 08 53 48 83 ec 20 48 8b d9", "peace offer constructor"),
        Point::keep(TREATY_NEW, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48 89 4c 24 08 57 41 56 41 57 48 83 ec 20", "peace treaty constructor"),
        Point::keep(TREATY_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 48 8d 05 e7 16 2e 01", "peace treaty destructor"),
        // The war goals and their list.
        Point::keep(WAR_GOAL_LIST, "48 89 5c 24 10 4c 89 4c 24 20 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 50", "war goals offered"),
        method(WAR_GOALS, 0, WAR_GOAL_LIST_DESTROY, "war goal list destructor"),
        Point::keep(WAR_GOAL_LIST_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 bc 14 00 00", "war goal list destructor"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::fake::game_tag;

    #[test]
    fn a_refusal_gives_each_reason_once_without_the_description() {
        // As the checks write them: a reason a line, colored and bulleted,
        // then the description.
        let alliance = "\u{a7}R- Cannot ally with a rival.\u{a7}!\n\u{a7}R- You have no diplomats to send.\u{a7}!\nIf a country accepts our Alliance offer it may come to our assistance during wars.";
        let panel = "\u{a7}R- Cannot ally with a rival.\u{a7}!\n";
        let description = text_lines(&game_text::from_str("If a country accepts our Alliance offer it may come to our assistance during wars."));
        let said: Vec<String> = text_lines(&game_text::from_str(panel)).into_iter().chain(text_lines(&game_text::from_str(alliance))).collect();
        assert_eq!(refusal(said, &description), "Cannot ally with a rival.\nYou have no diplomats to send.");
        assert_eq!(refusal(description.clone(), &description), "the game does not allow it, and gives no reason");
        // Icons as their names, as everywhere in game text read plain.
        assert_eq!(text_lines(&game_text::from_str("\u{a7}R- \u{a3}icon_dip\u{a3} too low\u{a7}!")), ["dip too low"]);
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
        l.carried_out(Action::ProposeAlliance, byz, action::DECLINED);
        let alliance = l.send(Action::ProposeAlliance, byz, l.carried_count, 1, Vec::new());
        let war = l.send(Action::DeclareWar, ven, l.carried_count, 1, Vec::new());
        assert_eq!(l.seen(alliance).unwrap().1, Seen::Nothing);
        l.carried_out(Action::ProposeAlliance, byz, action::UNANSWERED);
        l.carried_out(Action::ProposeAlliance, ven, action::ACCEPTED);
        assert_eq!(l.seen(alliance).unwrap().1, Seen::CarriedOut);
        assert_eq!(l.seen(war).unwrap().1, Seen::Nothing);
        l.carried_out(Action::DeclareWar, ven, action::UNANSWERED);
        l.carried_out(Action::ProposeAlliance, byz, action::ACCEPTED);
        assert_eq!(l.seen(alliance).unwrap(), (l.sent[0].clone(), Seen::Answered(true)));
        assert_eq!(l.seen(war).unwrap().1, Seen::CarriedOut);
        // A declaration has no answer.
        l.carried_out(Action::DeclareWar, ven, action::DECLINED);
        assert_eq!(l.seen(war).unwrap().1, Seen::CarriedOut);
        assert_eq!(l.seen(Ticket(99)), None);
    }

    #[test]
    fn an_earlier_proposal_s_answer_is_not_a_later_one_s() {
        let byz = game_tag(b"BYZ", 33);
        let mut l = Ledger::default();
        // A proposal carried out before this one was sent, its answer after.
        l.carried_out(Action::OfferPeace, byz, action::UNANSWERED);
        let peace = l.send(Action::OfferPeace, byz, l.carried_count, 1, Vec::new());
        l.carried_out(Action::OfferPeace, byz, action::ACCEPTED);
        assert_eq!(l.seen(peace).unwrap().1, Seen::Nothing);
        l.carried_out(Action::OfferPeace, byz, action::UNANSWERED);
        assert_eq!(l.seen(peace).unwrap().1, Seen::CarriedOut);
        l.carried_out(Action::OfferPeace, byz, action::DECLINED);
        assert_eq!(l.seen(peace).unwrap().1, Seen::Answered(false));
    }

    #[test]
    fn only_the_latest_actions_are_kept() {
        let mut l = Ledger::default();
        let first = l.send(Action::OfferPeace, game_tag(b"BYZ", 33), 0, 1, Vec::new());
        for _ in 0..KEPT {
            l.send(Action::OfferPeace, game_tag(b"BYZ", 33), 0, 1, Vec::new());
            l.carried_out(Action::OfferPeace, game_tag(b"BYZ", 33), action::UNANSWERED);
        }
        assert_eq!(l.seen(first), None);
        assert_eq!((l.sent.len(), l.carried.len()), (KEPT, KEPT));
    }
}
