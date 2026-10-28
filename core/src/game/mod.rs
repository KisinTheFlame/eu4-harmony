//! The capability layer: every capability extensions and remote clients use,
//! implemented once (ADR 0015). Each capability is a function taking the
//! [`Caller`] and returning the framework's own types; the C ABI for
//! extensions (`crate::ext`) only converts to and from them. One file per
//! group: [`session`], [`countries`], [`provinces`], [`armies`],
//! [`interface`], [`map_coloring`], [`save_data`], [`texts`], [`camera`] and
//! [`screenshot`].
//!
//! Under them, reading the campaign, finding routes and giving orders, ported
//! from eu4-regional-combat's native adapter ([`world`], [`orders`], and
//! [`events`], which establishes the safe point everything here runs at);
//! addresses are RVAs in the 1.37.5 layout (see `baseline`), with the evidence
//! in docs/research/regional-combat/.
pub mod armies;
pub mod camera;
pub mod countries;
pub mod events;
pub mod interface;
pub mod map_coloring;
pub mod orders;
pub mod provinces;
pub mod save_data;
pub mod screenshot;
pub mod session;
pub mod texts;
pub mod world;

use crate::{
    memory::Memory,
    objects::campaign::{self, countries as country_list, country as country_fields, globals, map_manager, pause, tag},
    points::Point,
};
use std::sync::Mutex;

/// Who calls a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    /// The extension with this index, from one of its callbacks.
    Extension(usize),
    /// Remote control. It acts as the player: its orders are the player's.
    Remote,
}

impl Caller {
    /// The calling extension, for what only an extension has (`what`, such as
    /// "interface elements").
    pub fn extension(self, what: &str) -> GResult<usize> {
        match self {
            Caller::Extension(index) => Ok(index),
            Caller::Remote => fail(Status::Unavailable, format!("only extensions have {what}")),
        }
    }

    /// The id of the calling extension's mod.
    fn mod_id(self, what: &str) -> GResult<String> {
        let index = self.extension(what)?;
        crate::ext::mod_id(index).ok_or_else(|| format!("no extension {index}").into())
    }
}

/// Why a capability failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// No campaign is running, or one is being loaded.
    NotInGame,
    /// The game state cannot be read right now; a later frame may succeed.
    NotReady,
    NotFound,
    /// A command was refused; the message says why.
    Rejected,
    InvalidArgument,
    /// Not available in this state, to this caller, or after an unexpected
    /// game-side failure.
    Unavailable,
    /// Called off the game's main thread.
    WrongContext,
    Internal,
}

/// A failed capability call.
#[derive(Debug, Clone)]
pub struct Fail {
    pub status: Status,
    pub message: String,
}
pub type GResult<T> = std::result::Result<T, Fail>;

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self { status: Status::Internal, message }
    }
}
pub fn fail<T>(status: Status, message: impl Into<String>) -> GResult<T> {
    Err(Fail { status, message: message.into() })
}

/// The game, for a capability: on the game's main thread only, once startup
/// has found it.
fn game() -> GResult<&'static Memory> {
    if crate::thread_id() != crate::main_thread() {
        return fail(Status::WrongContext, "called off the game's main thread");
    }
    crate::memory().ok_or_else(|| Fail { status: Status::Unavailable, message: "game capabilities unavailable".into() })
}

/// What the capabilities keep between calls: the map's topology and the
/// routes extensions installed. Both belong to one session, as does the last
/// day announced.
#[derive(Default)]
struct State {
    geography: world::Geography,
    orders: orders::Orders,
    session: u64,
    day: Option<i32>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

/// What a safe frame starts from: the campaign, its session, and what
/// happened since the last one, which extensions and remote control hear in
/// this order: a new session, the changes, a new day.
pub struct Frame {
    pub ctx: Ctx,
    pub session: u64,
    /// The session differs from the last frame's: a campaign was loaded or
    /// started, and what the capabilities kept from the old one is gone.
    pub new_session: bool,
    pub visibility_current: bool,
    pub changes: Vec<events::Change>,
    /// The date differs from the last day announced in this session, and
    /// visibility reflects it.
    pub new_day: bool,
}

/// Starts a safe frame (see [`events`]); fails when this is not one, saying
/// why. At a new session, forgets the old one's map, routes and map coloring.
/// An army the player gave an order is no longer an extension's to move.
pub fn begin_frame(m: &Memory) -> GResult<Frame> {
    let state = events::begin_frame(m)?;
    let (new_session, new_day) = with_state(|s| {
        let new_session = s.session != state.session;
        if new_session {
            s.session = state.session;
            s.geography.clear();
            s.orders.clear();
            s.day = None;
        }
        for change in &state.changes {
            if let events::Change::Ordered(army) = change {
                s.orders.revoke(*army);
            }
        }
        let new_day = state.visibility_current && s.day.replace(state.ctx.date) != Some(state.ctx.date);
        (new_session, new_day)
    });
    if new_session {
        crate::map::reset_session(m, &state.ctx);
    }
    Ok(Frame { ctx: state.ctx, session: state.session, new_session, visibility_current: state.visibility_current, changes: state.changes, new_day })
}

const IS_PAUSED: usize = 0x7e01b0;

pub const MAX_PROVINCES: usize = 50_000;
pub const MAX_UNITS: usize = 100_000;

/// Game functions the game-state capabilities call or observe.
pub fn points() -> Vec<Point> {
    let mut points = vec![Point::keep(IS_PAUSED, "40 53 48 83 ec 20 80 79 48 00 48 8b d9 74 0e", "pause query")];
    points.extend(world::points());
    points.extend(orders::points());
    points.extend(events::points());
    points.extend(camera::points());
    points.extend(provinces::points());
    points
}

/// How many window pixels a GUI unit takes, (across, down); None before the
/// GUI exists (docs/research/text-input.md §3).
pub fn gui_scale(m: &Memory) -> Option<(f32, f32)> {
    use crate::objects::gui::{globals, scale};
    let gui = m.ptr(m.at(globals::SCALE)).ok().filter(|&g| g != 0)?;
    let read = |at: usize| m.u32(gui + at).ok().map(f32::from_bits).filter(|s| s.is_finite() && *s > 0.0);
    Some((read(scale::ACROSS)?, read(scale::DOWN)?))
}

/// Country tag as stored by the game (see [`tag`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tag(pub u64);
impl Tag {
    pub fn index(self) -> i16 {
        (self.0 >> tag::INDEX) as u16 as i16
    }
    pub fn kind(self) -> u8 {
        (self.0 >> tag::KIND) as u8
    }
    pub fn valid(self) -> bool {
        self.0 >> tag::VALID != 0
    }
    /// A real country (not a rebel faction or an empty tag).
    pub fn normal(self) -> bool {
        self.valid() && self.index() > 0 && self.kind() != tag::REBELS
    }
    pub fn letters(self) -> [u8; 3] {
        let b = self.0.to_le_bytes();
        [b[0], b[1], b[2]]
    }
}

/// The campaign as seen at one instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub world: usize,
    pub date: i32,
    pub player: Tag,
    pub country: usize,
    pub countries: usize,
}

/// Current campaign, refusing while the daily update is running.
pub fn context(m: &Memory) -> GResult<Ctx> {
    context_impl(m, true)
}

/// Also during the daily update; only for boundary observers.
pub fn context_any(m: &Memory) -> GResult<Ctx> {
    context_impl(m, false)
}

fn context_impl(m: &Memory, require_stable: bool) -> GResult<Ctx> {
    let world = m.ptr(m.at(globals::WORLD))?;
    if world == 0 {
        return fail(Status::NotInGame, "no world");
    }
    if require_stable && m.u8(world + campaign::world::DAILY_UPDATE)? != 0 {
        return fail(Status::NotReady, "daily update in progress");
    }
    let switched = m.u8(world + campaign::world::PLAYER_MODE)? == campaign::world::SWITCHED && m.u8(world + campaign::world::PLAYER_SWITCHED)? != 0;
    let player = Tag(m.u64(world + if switched { campaign::world::SWITCHED_PLAYER } else { campaign::world::PLAYER })?);
    if !player.normal() {
        return fail(Status::NotInGame, "no controllable player");
    }
    let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
    let countries = m.ptr(manager + country_list::LIST)?;
    let country = m.ptr(countries + player.index() as usize * 8)?;
    if m.u64(country + country_fields::TAG)? != player.0 {
        return fail(Status::NotInGame, "player country does not match its tag");
    }
    Ok(Ctx { world, date: m.i32(world + campaign::world::DATE)?, player, country, countries })
}

/// Country object for a tag, verified against the tag it stores.
pub fn country(m: &Memory, ctx: &Ctx, tag: Tag) -> GResult<usize> {
    if !tag.valid() || tag.index() < 0 {
        return fail(Status::NotFound, "invalid country");
    }
    let p = m.ptr(ctx.countries + tag.index() as usize * 8)?;
    if p == 0 || m.u64(p + country_fields::TAG)? != tag.0 {
        return fail(Status::NotFound, "country does not match its tag");
    }
    Ok(p)
}

pub fn special_countries(m: &Memory) -> GResult<[i32; 3]> {
    let mut out = [0; 3];
    for (slot, rva) in out.iter_mut().zip(globals::SPECIAL_COUNTRIES) {
        *slot = m.i32(m.at(rva))?;
        if !(0..=32767).contains(slot) {
            return Err(format!("invalid special country index {slot}").into());
        }
    }
    Ok(out)
}

/// Whether the game is paused (the map manager's pause owner says so).
pub fn paused(m: &Memory, ctx: &Ctx) -> GResult<bool> {
    let manager = map_manager(m, ctx)?;
    let pause = m.ptr(manager + map_manager::PAUSE)?;
    if m.ptr(pause + pause::MANAGER)? != manager {
        return Err("unexpected pause owner".to_string().into());
    }
    Ok(m.call(m.at(IS_PAUSED), &[pause])? as u8 != 0)
}

/// The world's map manager, checked by its class.
pub fn map_manager(m: &Memory, ctx: &Ctx) -> GResult<usize> {
    let manager = m.ptr(ctx.world + campaign::world::MAP_MANAGER)?;
    if m.ptr(manager)? != m.at(campaign::classes::MAP_MANAGER) {
        return Err("unexpected map manager".to_string().into());
    }
    Ok(manager)
}

/// Bounded element count of a `[begin, end)` vector of `stride`-byte items.
pub fn span(begin: usize, end: usize, stride: usize, maximum: usize) -> GResult<usize> {
    let bytes = end.checked_sub(begin).ok_or_else(|| "reversed vector".to_string())?;
    if bytes % stride != 0 || bytes / stride > maximum {
        return Err("vector bound or alignment".to_string().into());
    }
    Ok(bytes / stride)
}

#[cfg(test)]
mod tests {
    use super::Tag;

    #[test]
    fn tag_fields() {
        let t = Tag(0x0100_0001_0053_4143);
        assert_eq!(t.letters(), *b"CAS");
        assert_eq!(t.index(), 1);
        assert!(t.valid() && t.normal());
        assert!(!Tag(0x0107_0005_0042_4552).normal());
        assert!(!Tag(0).valid());
    }
}
