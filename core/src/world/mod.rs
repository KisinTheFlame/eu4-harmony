//! Game-world read and command primitives shared by `capabilities` and `dev`:
//! world/country context, caller/view/result types, command posting, player
//! state, script conditions, content registries, modifiers, envoys, and power
//! spendings. Depends only on `base`, `native`, and `text`.

pub(crate) mod action;
pub(crate) mod commands;
pub(crate) mod conditions;
pub(crate) mod content;
pub(crate) mod envoys;
pub(crate) mod modifiers;
pub(crate) mod player;
pub(crate) mod power;

use crate::native::{
    containers,
    memory::Memory,
    objects::campaign::{self, countries as country_list, country as country_fields, globals, map, map_manager, pause, province, tag},
    points::{GameFunction, Point},
};

pub(crate) use eu4_harmony_remote::As;

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
pub(crate) fn game() -> GResult<&'static Memory> {
    if crate::base::thread_id() != crate::base::main_thread() {
        return fail(Status::WrongContext, "called off the game's main thread");
    }
    crate::native::memory().ok_or_else(|| Fail { status: Status::Unavailable, message: "game capabilities unavailable".into() })
}

/// A tooltip's reasons, as the game wrote them into a string: plain lines,
/// without color codes, the bullet a line may start with, or empty lines
/// (docs/adding-a-capability.md, "Choices, reasons, costs").
pub fn reasons(text: &[u8]) -> Vec<String> {
    crate::native::game_text::plain(text).lines().map(|l| unbulleted(l).trim_end()).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// A reason's line without the bullets it starts with: a check's own `- `
/// and the full-width dash a localisation text may start with after it.
pub fn unbulleted(line: &str) -> &str {
    line.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '－' | '•' | '·'))
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
    let session = crate::native::objects::session::OF_MAP_MANAGER.call(m, [map_manager(m, ctx)?])?;
    if session == 0 || m.ptr(session)? != m.at(crate::native::objects::session::CLASS) {
        return Err("the map manager has no session".to_string().into());
    }
    Ok(session)
}

pub(crate) const IS_PAUSED: GameFunction<1> = GameFunction::at(0x7e01b0);

pub(crate) fn named(tag: Tag) -> eu4_harmony_remote::CountryTag {
    eu4_harmony_remote::CountryTag(tag.letters())
}

/// First province and count, cross-checked between the map and `world`.
pub(crate) fn province_array(m: &Memory, world: usize) -> GResult<(usize, usize)> {
    let the_map = m.ptr(m.at(globals::MAP))?;
    let first = m.ptr(the_map + map::PROVINCES)?;
    let end = m.ptr(the_map + map::PROVINCES + 8)?;
    let count = containers::span(first, end, province::SIZE, MAX_PROVINCES)?;
    if count == 0 || m.ptr(world + campaign::world::PROVINCES)? != first {
        return Err("world and map province arrays differ".to_string().into());
    }
    Ok((first, count))
}

/// How many provinces `owner` owns in `world`, which need not have a player.
pub(crate) fn owned_provinces(m: &Memory, world: usize, owner: Tag) -> GResult<usize> {
    let (first, count) = province_array(m, world)?;
    let mut owned = 0;
    for index in 0..count {
        if m.u64(first + index * province::SIZE + province::OWNER)? == owner.0 {
            owned += 1;
        }
    }
    Ok(owned)
}

/// The provinces `owner` owns in `world`, their addresses in array order.
pub(crate) fn provinces_owned_by(m: &Memory, world: usize, owner: Tag) -> GResult<Vec<usize>> {
    let (first, count) = province_array(m, world)?;
    let mut owned = Vec::new();
    for index in 0..count {
        let addr = first + index * province::SIZE;
        if m.u64(addr + province::OWNER)? == owner.0 {
            owned.push(addr);
        }
    }
    Ok(owned)
}

pub const MAX_PROVINCES: usize = 50_000;
pub const MAX_UNITS: usize = 100_000;

/// Dependency points owned by the shared game-world primitives.
pub(crate) fn points() -> Vec<Point> {
    let mut points = vec![Point::keep(IS_PAUSED, "40 53 48 83 ec 20 80 79 48 00 48 8b d9 74 0e", "pause query")];
    points.extend(commands::points());
    points.extend(player::points());
    points.extend(conditions::points());
    points.extend(power::points());
    points.extend(modifiers::points());
    points.extend(envoys::points());
    points
}

#[cfg(test)]
mod tests {
    use super::{unbulleted, view, As, Caller, View};

    #[test]
    fn a_reason_loses_the_check_s_bullet_and_the_text_s_own_dash() {
        assert_eq!(unbulleted("- －提升政府等级你至少需要 50 威望。"), "提升政府等级你至少需要 50 威望。");
        assert_eq!(unbulleted("• 你已经借贷了56笔贷款"), "你已经借贷了56笔贷款");
        assert_eq!(unbulleted("你没有外交官可供派遣。"), "你没有外交官可供派遣。");
    }

    #[test]
    fn extensions_and_requests_as_the_game_read_the_full_view_and_as_the_player_the_players() {
        assert_eq!(view(Caller::Extension(0)), View::Full);
        assert_eq!(view(Caller::Remote(As::Player)), View::Player);
        assert_eq!(view(Caller::Remote(As::Game)), View::Full);
    }
}
