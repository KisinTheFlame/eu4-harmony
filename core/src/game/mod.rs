//! The capability layer: every capability extensions and remote clients use,
//! implemented once (ADR remote-control-uses-capabilities). Each capability is
//! a function taking first the [`Session`] it reads, which the catalogue opens
//! for each request, or, for one that runs without a session or checks
//! something of its own before, the [`Caller`], and returning the framework's
//! own types; the capability catalogue (`crate::catalogue`), which remote
//! control and the Lua host both run their requests through, only converts to
//! and from them (ADR one-capability-catalogue). One file per group: [`session`],
//! [`countries`], [`provinces`], [`armies`], [`interface`], [`map_coloring`],
//! [`texts`], [`camera`], [`screenshot`], [`console`], [`time`],
//! [`recruiting`], [`menu`] and [`diplomacy`]; the game's questions and offers waiting for the player's
//! answer, [`questions`] and [`offers`].
//!
//! Under them, reading the session, finding routes and giving orders, ported
//! from eu4-regional-combat's native adapter ([`world`], [`orders`], and
//! [`signals`], which establishes the safe point everything here runs at);
//! addresses are RVAs in the 1.37.5 layout (see `baseline`), with the evidence
//! in docs/research/ (provinces.md, armies.md, routes.md, army-orders.md,
//! defeat.md, time.md, fog-of-war.md).
pub mod action;
pub mod administration;
pub mod army;
#[cfg(test)]
pub mod buffers;
pub mod commands;
pub mod conditions;
pub mod content;
pub mod envoys;
pub mod frame;
pub mod interface;
pub mod map_coloring;
pub mod modifiers;
pub mod orders;
pub mod player;
pub mod power;
pub mod signal_costs;
pub mod signals;
pub mod units;
pub mod wars;
pub mod world;

/// Declares each capability group's module of the layer, and collects the
/// groups' dependency points ([`group_points`]), from the catalogue's list
/// of the groups (`crate::catalogue::capability_groups`).
macro_rules! layer_modules {
    (@munch [$($modules:tt)*] $(#[$attr:meta])* $Group:ident($module:ident $(: Layer)?) $(=> $outcome:expr)?, $($rest:tt)*) => {
        layer_modules!(@munch [$($modules)* { $(#[$attr])* $module }] $($rest)*);
    };
    (@munch $modules:tt $(#[$attr:meta])* $Group:ident => $outcome:expr, $($rest:tt)*) => {
        layer_modules!(@munch $modules $($rest)*);
    };
    (@munch [$({ $(#[$attr:meta])* $module:ident })*]) => {
        $( $(#[$attr])* pub mod $module; )*

        /// The capability groups' dependency points.
        fn group_points() -> Vec<Point> {
            let mut points = Vec::new();
            $( $(#[$attr])* points.extend($module::points()); )*
            points
        }
    };
    (fn dispatch($($name:ident),*); $($lines:tt)*) => {
        layer_modules!(@munch [] $($lines)*);
    };
}

crate::catalogue::capability_groups!(layer_modules);

use crate::{
    memory::Memory,
    objects::campaign::{self, countries as country_list, country as country_fields, globals, map_manager, pause, tag},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::signals::Kind;
use std::sync::Mutex;

pub use frame::{Frame, Heard};

/// How much of the game a read gives (GLOSSARY: Player's view, Full view;
/// ADR requests-act-as-the-player-or-as-the-game).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// What the player's interface shows: the map as fog of war leaves it,
    /// and of other countries what their panels show.
    Player,
    /// Everything the game keeps.
    Full,
}

/// The view `caller` reads: the full view for an extension and a request as
/// the game, the player's view for a request as the player.
pub fn view(caller: Caller) -> View {
    match caller {
        Caller::Extension(_) | Caller::Remote(As::Game) => View::Full,
        Caller::Remote(As::Player) => View::Player,
    }
}

pub use eu4_harmony_remote::As;

/// Who calls a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller {
    /// The extension with this index, from one of its callbacks. It may do
    /// what the game does too (ADR requests-act-as-the-player-or-as-the-game).
    Extension(usize),
    /// A remote client's request, as the player or as the game. Its orders
    /// are the player's.
    Remote(As),
}

impl Caller {
    /// The calling extension, for what only an extension has (`what`, such as
    /// "interface elements").
    pub fn extension(self, what: &str) -> GResult<usize> {
        match self {
            Caller::Extension(index) => Ok(index),
            Caller::Remote(_) => fail(Status::Unavailable, format!("only extensions have {what}")),
        }
    }
}

/// Why a capability failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// No campaign is running, or one is being loaded.
    NotInGame,
    /// The player observes with no country in view, so there is no country
    /// to act as (ADR observe-mode-acts-as-the-country-in-view).
    NoCountry,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail {
    pub status: Status,
    pub message: String,
}
pub type GResult<T> = std::result::Result<T, Fail>;

impl Fail {
    /// A failure with `status`, in the words `message`: a refusal the
    /// capability words itself.
    pub fn new(status: Status, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self { status: Status::Internal, message }
    }
}

/// A refusal of the framework's own checks (`eu4_harmony_checks`).
impl From<eu4_harmony_checks::Refusal> for Fail {
    fn from(refusal: eu4_harmony_checks::Refusal) -> Self {
        use eu4_harmony_checks::Status as Checked;
        let status = match refusal.status {
            Checked::NotReady => Status::NotReady,
            Checked::NotFound => Status::NotFound,
            Checked::Rejected => Status::Rejected,
            Checked::InvalidArgument => Status::InvalidArgument,
        };
        Self { status, message: refusal.message }
    }
}
pub fn fail<T>(status: Status, message: impl Into<String>) -> GResult<T> {
    Err(Fail { status, message: message.into() })
}

/// A fixed-point number of the game's, kept in thousandths (a treasury, a
/// development), as the number it stands for: 9500 is 9.5. Extensions and
/// remote clients get it so (ADR snapshots-cross-the-abi-as-json).
pub fn thousandths(value: i32) -> f64 {
    value as f64 / 1000.0
}

/// The game, for a capability: on the game's main thread only, once startup
/// has found it.
fn game() -> GResult<&'static Memory> {
    if crate::thread_id() != crate::main_thread() {
        return fail(Status::WrongContext, "called off the game's main thread");
    }
    crate::memory().ok_or_else(|| Fail { status: Status::Unavailable, message: "game capabilities unavailable".into() })
}

/// The session a capability reads, as its caller reads it: the game, the
/// session at this instant ([`Ctx`]), how much of it the caller reads, the
/// caller, and where its commands go. The catalogue opens it once for each
/// request (`crate::catalogue::Live`) and hands it to the capability, which
/// reaches the game through it alone; a test builds one over objects of its
/// own (`buffers`), whose commands are recorded.
#[derive(Clone, Copy, Debug)]
pub struct Session {
    pub m: &'static Memory,
    pub ctx: Ctx,
    pub view: View,
    pub caller: Caller,
    pub commands: commands::Commands,
}

impl Session {
    /// Opens the session for `caller`, deciding in one place what a
    /// capability that reads it fails with otherwise:
    /// [`Status::WrongContext`] off the game's main thread,
    /// [`Status::Unavailable`] before startup found the game,
    /// [`Status::NotInGame`] with no session, [`Status::NoCountry`] with no
    /// country to act as, [`Status::NotReady`] while the daily update runs.
    pub fn open(caller: Caller) -> GResult<Self> {
        let m = game()?;
        let ctx = context(m)?;
        Ok(Self { m, ctx, view: view(caller), caller, commands: commands::Commands::Game })
    }

    /// The same session, as a capability that acts as no country reads it.
    pub fn world(self) -> GResult<WorldSession> {
        Ok(WorldSession { m: self.m, world: world_context(self.m)?, view: self.view, caller: self.caller, commands: self.commands })
    }
}

/// The session as a capability that acts as no country reads it: the game,
/// the world at this instant ([`World`]) with its player, also while the
/// player observes with no country in view, how much of it the caller reads,
/// the caller, and where its commands go. The catalogue opens it for each
/// request of such a capability, as it opens [`Session`] for the others.
#[derive(Clone, Copy, Debug)]
pub struct WorldSession {
    pub m: &'static Memory,
    pub world: World,
    pub view: View,
    pub caller: Caller,
    pub commands: commands::Commands,
}

impl WorldSession {
    /// Opens it for `caller`: failing as [`Session::open`] does, but never
    /// for want of a country to act as.
    pub fn open(caller: Caller) -> GResult<Self> {
        let m = game()?;
        Ok(Self { m, world: world_context(m)?, view: view(caller), caller, commands: commands::Commands::Game })
    }
}

/// What the capabilities keep between calls: the map's topology, the routes
/// extensions installed, and the wars and the provinces' owners of the day
/// before. All belong to one session; the sessions let them go
/// ([`session_forgotten`]).
#[derive(Default)]
struct State {
    geography: world::Geography,
    orders: orders::Orders,
    wars: wars::Watch,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

/// What the session's frames showed, which the next is heard against.
static RECALL: Mutex<frame::Recall> = Mutex::new(frame::Recall::new());

/// A game frame shows another session than the last frame did: what the
/// capabilities kept of that one goes. Told by the sessions
/// (`crate::sessions`).
pub fn session_forgotten() {
    with_state(|s| {
        s.geography.clear();
        s.orders.clear();
        s.wars.clear();
    });
}

/// Starts a safe frame (see [`signals`]); fails when this is not one, saying
/// why. The sessions hear of it first, which let what was kept of another
/// session go (`crate::sessions`). An army the player gave an order is no
/// longer an extension's to move. What happened since the last frame is
/// [`frame`]'s to decide.
pub fn begin_frame(m: &Memory) -> GResult<Frame> {
    let (world, fog) = signals::frame_campaign(m)?;
    let seen = crate::sessions::frame_read(&world);
    let (visibility_current, changes) = signals::take_frame(&world, fog);
    with_state(|s| {
        for change in &changes {
            if let signals::Change::Ordered(army) = change {
                s.orders.revoke(*army);
            }
        }
    });
    let observed = frame::Observed { world, seen, visibility_current, changes };
    let mut recall = RECALL.lock().unwrap_or_else(|e| e.into_inner());
    Ok(recall.next(observed, signals::listened, |world| watch_wars(m, world)))
}

/// What the wars and the provinces' owners show happened since the day
/// before, of the kinds someone listens to; nothing when they cannot be
/// read, which the log says.
fn watch_wars(m: &Memory, ctx: &World) -> Vec<wars::Happened> {
    let war = signals::listened(Kind::WarDeclared) || signals::listened(Kind::PeaceMade);
    let owners = signals::listened(Kind::ProvinceOwnerChanged);
    let seen = with_state(|s| {
        let mut happened = Vec::new();
        match war {
            true => happened.extend(signal_costs::WARS.time(|| s.wars.wars(m, ctx))?),
            false => s.wars.forget_wars(),
        }
        match owners {
            true => happened.extend(signal_costs::OWNERS.time(|| Ok::<_, Fail>(s.wars.owners(s.geography.owners(m, ctx)?)))?),
            false => s.wars.forget_owners(),
        }
        Ok::<_, Fail>(happened)
    });
    seen.unwrap_or_else(|e| {
        log!("wars: {}", e.message);
        Vec::new()
    })
}

/// A country, as signals name it.
fn named(tag: Tag) -> eu4_harmony_remote::CountryTag {
    eu4_harmony_remote::CountryTag(tag.letters())
}

const IS_PAUSED: GameFunction<1> = GameFunction::at(0x7e01b0);

pub const MAX_PROVINCES: usize = 50_000;
pub const MAX_UNITS: usize = 100_000;

/// Game functions the game-state capabilities call or observe.
pub fn points() -> Vec<Point> {
    let mut points = vec![Point::keep(IS_PAUSED, "40 53 48 83 ec 20 80 79 48 00 48 8b d9 74 0e", "pause query")];
    points.extend(group_points());
    points.extend(commands::points());
    points.extend(world::points());
    points.extend(army::points());
    points.extend(orders::points());
    points.extend(signals::points());
    points.extend(player::points());
    points.extend(conditions::points());
    points.extend(power::points());
    points.extend(modifiers::points());
    points.extend(administration::points());
    points.extend(units::points());
    points.extend(envoys::points());
    points
}

/// A tooltip's reasons, as the game wrote them into a string: plain lines,
/// without color codes, the bullet a line may start with, or empty lines
/// (docs/plans/player-actions.md, "Choices, reasons, costs").
pub fn reasons(text: &[u8]) -> Vec<String> {
    crate::unicode::game_text::plain(text).lines().map(|l| unbulleted(l).trim_end()).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// A reason's line without the bullets it starts with: a check's own `- `
/// and the full-width dash a localisation text may start with after it.
pub fn unbulleted(line: &str) -> &str {
    line.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '－' | '•' | '·'))
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

/// The session as seen at one instant: its world, its date, and the country
/// acted as, by its tag and its object, among the countries: the one the
/// player plays, or in observe mode the country in view
/// ([`player::Player::acting`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub world: usize,
    pub date: i32,
    pub player: Tag,
    pub country: usize,
    pub countries: usize,
}

/// The game going on as seen at one instant, whether there is a country to
/// act as or not: its world, its date, its countries and its player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct World {
    pub world: usize,
    pub date: i32,
    pub countries: usize,
    pub player: player::Player,
}

/// What holds the game's world and its countries: [`Ctx`], with a country to
/// act as, or [`World`], with or without one.
pub trait InWorld {
    fn world(&self) -> usize;
    fn countries(&self) -> usize;
}

impl InWorld for Ctx {
    fn world(&self) -> usize {
        self.world
    }
    fn countries(&self) -> usize {
        self.countries
    }
}

impl InWorld for World {
    fn world(&self) -> usize {
        self.world
    }
    fn countries(&self) -> usize {
        self.countries
    }
}

/// The session going on, refusing while the daily update is running.
pub fn context(m: &Memory) -> GResult<Ctx> {
    context_impl(m, true)
}

/// Also during the daily update; only for boundary observers.
pub fn context_any(m: &Memory) -> GResult<Ctx> {
    context_impl(m, false)
}

/// The game going on, also with no country to act as, refusing while the
/// daily update is running.
pub fn world_context(m: &Memory) -> GResult<World> {
    world_impl(m, true)
}

/// [`world_context`], also during the daily update; only for boundary
/// observers.
pub fn world_context_any(m: &Memory) -> GResult<World> {
    world_impl(m, false)
}

fn world_impl(m: &Memory, require_stable: bool) -> GResult<World> {
    let world = m.ptr(m.at(globals::WORLD))?;
    if world == 0 {
        return fail(Status::NotInGame, "no world");
    }
    if require_stable && m.u8(world + campaign::world::DAILY_UPDATE)? != 0 {
        return fail(Status::NotReady, "daily update in progress");
    }
    let player = player::read(m, world)?;
    let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
    let countries = m.ptr(manager + country_list::LIST)?;
    Ok(World { world, date: m.i32(world + campaign::world::DATE)?, countries, player })
}

fn context_impl(m: &Memory, require_stable: bool) -> GResult<Ctx> {
    let World { world, date, countries, player } = world_impl(m, require_stable)?;
    let Some(acting) = player.acting() else {
        return fail(Status::NoCountry, "the player observes with no country in view");
    };
    let country = m.ptr(countries + acting.index() as usize * 8)?;
    if m.u64(country + country_fields::TAG)? != acting.0 {
        return fail(Status::NotInGame, "the player's country does not match its tag");
    }
    Ok(Ctx { world, date, player: acting, country, countries })
}

/// Country object for a tag, verified against the tag it stores.
pub fn country(m: &Memory, ctx: &impl InWorld, tag: Tag) -> GResult<usize> {
    if !tag.valid() || tag.index() < 0 {
        return fail(Status::NotFound, "invalid country");
    }
    let p = m.ptr(ctx.countries() + tag.index() as usize * 8)?;
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
pub fn paused(m: &Memory, ctx: &impl InWorld) -> GResult<bool> {
    let manager = map_manager(m, ctx)?;
    let pause = m.ptr(manager + map_manager::PAUSE)?;
    if m.ptr(pause + pause::MANAGER)? != manager {
        return Err("unexpected pause owner".to_string().into());
    }
    Ok(IS_PAUSED.call(m, [pause])? as u8 != 0)
}

/// The world's map manager, checked by its class.
pub fn map_manager(m: &Memory, ctx: &impl InWorld) -> GResult<usize> {
    let manager = m.ptr(ctx.world() + campaign::world::MAP_MANAGER)?;
    if m.ptr(manager)? != m.at(campaign::classes::MAP_MANAGER) {
        return Err("unexpected map manager".to_string().into());
    }
    Ok(manager)
}

/// The game's session, which takes the player's commands, as the player's
/// buttons find it: the map manager's method answers it
/// (`objects::session`).
pub fn command_session(m: &Memory, ctx: &impl InWorld) -> GResult<usize> {
    let session = crate::objects::session::OF_MAP_MANAGER.call(m, [map_manager(m, ctx)?])?;
    if session == 0 || m.ptr(session)? != m.at(crate::objects::session::CLASS) {
        return Err("the map manager has no session".to_string().into());
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::{view, As, Caller, Tag, View};

    #[test]
    fn a_reason_loses_the_check_s_bullet_and_the_text_s_own_dash() {
        assert_eq!(super::unbulleted("- －提升政府等级你至少需要 50 威望。"), "提升政府等级你至少需要 50 威望。");
        assert_eq!(super::unbulleted("• 你已经借贷了56笔贷款"), "你已经借贷了56笔贷款");
        assert_eq!(super::unbulleted("你没有外交官可供派遣。"), "你没有外交官可供派遣。");
    }

    #[test]
    fn extensions_and_requests_as_the_game_read_the_full_view_and_as_the_player_the_players() {
        assert_eq!(view(Caller::Extension(0)), View::Full);
        assert_eq!(view(Caller::Remote(As::Player)), View::Player);
        assert_eq!(view(Caller::Remote(As::Game)), View::Full);
    }

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
