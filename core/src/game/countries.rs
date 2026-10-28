//! Countries: finding one by its tag, its tag, and whether two are hostile.
use super::{context, country, fail, game, world, Caller, GResult, Status, Tag};

/// The country whose tag is these three letters (for example `CAS`);
/// [`Status::NotFound`] when there is none.
pub fn find(_: Caller, letters: [u8; 3]) -> GResult<Tag> {
    let m = game()?;
    let ctx = context(m)?;
    match world::countries(m, &ctx)?.into_iter().find(|t| t.letters() == letters) {
        Some(tag) => Ok(tag),
        None => fail(Status::NotFound, format!("no country {}", String::from_utf8_lossy(&letters))),
    }
}

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
