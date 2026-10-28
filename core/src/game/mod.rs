//! Game-state capabilities: reading the campaign, finding routes and giving
//! orders. Ported from eu4-regional-combat's native adapter; addresses are
//! RVAs in the 1.37.5 layout (see `baseline`), with the evidence in
//! docs/research/regional-combat/. Everything here runs on the game thread at
//! a safe point, which `events` establishes.
pub mod events;
pub mod orders;
pub mod world;

use crate::{
    memory::Memory,
    objects::campaign::{self, countries, country, globals, map_manager, pause, tag},
    points::Point,
};
use eu4_harmony_abi as abi;

/// A failed capability call: an ABI status code and a message for `last_error`.
#[derive(Debug, Clone)]
pub struct Fail {
    pub code: i32,
    pub message: String,
}
pub type GResult<T> = std::result::Result<T, Fail>;

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self { code: abi::INTERNAL, message }
    }
}
pub fn fail<T>(code: i32, message: impl Into<String>) -> GResult<T> {
    Err(Fail { code, message: message.into() })
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        return fail(abi::NOT_IN_GAME, "no world");
    }
    if require_stable && m.u8(world + campaign::world::DAILY_UPDATE)? != 0 {
        return fail(abi::NOT_READY, "daily update in progress");
    }
    let switched = m.u8(world + campaign::world::PLAYER_MODE)? == campaign::world::SWITCHED && m.u8(world + campaign::world::PLAYER_SWITCHED)? != 0;
    let player = Tag(m.u64(world + if switched { campaign::world::SWITCHED_PLAYER } else { campaign::world::PLAYER })?);
    if !player.normal() {
        return fail(abi::NOT_IN_GAME, "no controllable player");
    }
    let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
    let countries = m.ptr(manager + countries::LIST)?;
    let country = m.ptr(countries + player.index() as usize * 8)?;
    if m.u64(country + country::TAG)? != player.0 {
        return fail(abi::NOT_IN_GAME, "player country does not match its tag");
    }
    Ok(Ctx { world, date: m.i32(world + campaign::world::DATE)?, player, country, countries })
}

/// Country object for a tag, verified against the tag it stores.
pub fn country(m: &Memory, ctx: &Ctx, tag: Tag) -> GResult<usize> {
    if !tag.valid() || tag.index() < 0 {
        return fail(abi::NOT_FOUND, "invalid country");
    }
    let p = m.ptr(ctx.countries + tag.index() as usize * 8)?;
    if p == 0 || m.u64(p + country::TAG)? != tag.0 {
        return fail(abi::NOT_FOUND, "country does not match its tag");
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
