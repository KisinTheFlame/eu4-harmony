//! Game-state capabilities: reading the campaign, finding routes and giving
//! orders. Ported from eu4-regional-combat's native adapter; addresses are
//! RVAs in the 1.37.5 layout (see `baseline`), with the evidence in
//! docs/research/regional-combat/. Everything here runs on the game thread at
//! a safe point, which `events` establishes.
pub mod events;
pub mod orders;
pub mod world;

use crate::{memory::Memory, points::Point};
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

pub const WORLD: usize = 0x233fe78;
pub const COUNTRY_MANAGER: usize = 0x233d8d0;
pub const MAP: usize = 0x233fed8;
/// Indices of the rebel-like special countries that are hostile to everyone.
pub const SPECIAL_COUNTRIES: [usize; 3] = [0x233d8a4, 0x233d8d8, 0x233d8dc];
pub const ARMY_VT: usize = 0x1c42540;
pub const FLEET_VT: usize = 0x1cca7e0;
pub const MAP_MANAGER_VT: usize = 0x1cb8460;
const IS_PAUSED: usize = 0x7e01b0;

pub const PROVINCE_STRIDE: usize = 0x2e10;
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

/// Country tag as stored by the game: three ASCII bytes, then index, kind and a
/// validity byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag(pub u64);
impl Tag {
    pub fn index(self) -> i16 {
        (self.0 >> 32) as u16 as i16
    }
    pub fn kind(self) -> u8 {
        (self.0 >> 48) as u8
    }
    pub fn valid(self) -> bool {
        self.0 >> 56 != 0
    }
    /// A real country (not a rebel faction or an empty tag).
    pub fn normal(self) -> bool {
        self.valid() && self.index() > 0 && self.kind() != 7
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
    let world = m.ptr(m.at(WORLD))?;
    if world == 0 {
        return fail(abi::NOT_IN_GAME, "no world");
    }
    if require_stable && m.u8(world + 0x1e10)? != 0 {
        return fail(abi::NOT_READY, "daily update in progress");
    }
    let offset = if m.u8(world + 0x1e66)? == 7 && m.u8(world + 0x1e6f)? != 0 { 0x1e68 } else { 0x1e60 };
    let player = Tag(m.u64(world + offset)?);
    if !player.normal() {
        return fail(abi::NOT_IN_GAME, "no controllable player");
    }
    let manager = m.ptr(m.at(COUNTRY_MANAGER))?;
    let countries = m.ptr(manager + 0x118)?;
    let country = m.ptr(countries + player.index() as usize * 8)?;
    if m.u64(country + 0x20)? != player.0 {
        return fail(abi::NOT_IN_GAME, "player country does not match its tag");
    }
    Ok(Ctx { world, date: m.i32(world + 0x1dd0)?, player, country, countries })
}

/// Country object for a tag, verified against the tag it stores.
pub fn country(m: &Memory, ctx: &Ctx, tag: Tag) -> GResult<usize> {
    if !tag.valid() || tag.index() < 0 {
        return fail(abi::NOT_FOUND, "invalid country");
    }
    let p = m.ptr(ctx.countries + tag.index() as usize * 8)?;
    if p == 0 || m.u64(p + 0x20)? != tag.0 {
        return fail(abi::NOT_FOUND, "country does not match its tag");
    }
    Ok(p)
}

pub fn special_countries(m: &Memory) -> GResult<[i32; 3]> {
    let mut out = [0; 3];
    for (slot, rva) in out.iter_mut().zip(SPECIAL_COUNTRIES) {
        *slot = m.i32(m.at(rva))?;
        if !(0..=32767).contains(slot) {
            return Err(format!("invalid special country index {slot}").into());
        }
    }
    Ok(out)
}

/// Whether the game is paused (the map manager's pause owner says so).
pub fn paused(m: &Memory, ctx: &Ctx) -> GResult<bool> {
    let manager = m.ptr(ctx.world + 0x1e00)?;
    if m.ptr(manager)? != m.at(MAP_MANAGER_VT) {
        return Err("unexpected map manager".to_string().into());
    }
    let pause = m.ptr(manager + 0x418)?;
    if m.ptr(pause + 0x50)? != manager {
        return Err("unexpected pause owner".to_string().into());
    }
    Ok(m.call(m.at(IS_PAUSED), &[pause])? as u8 != 0)
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
