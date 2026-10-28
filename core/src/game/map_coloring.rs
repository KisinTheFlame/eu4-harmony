//! Coloring provinces on the map (see `crate::map`). Only extensions color
//! the map.
use super::{context, game, Caller, GResult};

const COLORING: &str = "map coloring";

/// The color of one province on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapColor {
    pub province: u32,
    /// Bytes R, G, B, A.
    pub rgba: [u8; 4],
}

/// Colors provinces, replacing the caller's previous coloring. Another
/// extension painting later takes over the map.
pub fn paint(caller: Caller, colors: &[MapColor]) -> GResult<()> {
    let extension = caller.extension(COLORING)?;
    let m = game()?;
    let ctx = context(m)?;
    crate::map::paint(m, &ctx, extension, colors)
}

/// Removes the caller's coloring and restores the map mode from before.
pub fn clear(caller: Caller) -> GResult<()> {
    let extension = caller.extension(COLORING)?;
    let m = game()?;
    let ctx = context(m)?;
    crate::map::clear(m, &ctx, extension)
}
