//! Countries: their tags and whether two are hostile.
use super::{context, country, game, world, Caller, GResult, Tag};

/// The three letters of a country's tag (for example `CAS`).
pub fn tag(_: Caller, country_id: Tag) -> GResult<[u8; 3]> {
    let m = game()?;
    let ctx = context(m)?;
    country(m, &ctx, country_id)?;
    Ok(country_id.letters())
}

/// Whether the two countries are at war, or either is a rebel faction.
pub fn hostile(_: Caller, a: Tag, b: Tag) -> GResult<bool> {
    let m = game()?;
    let ctx = context(m)?;
    world::hostile(m, &ctx, a, b)
}
