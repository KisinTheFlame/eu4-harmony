//! Provinces and their land neighbors.
use super::{context, events, game, with_state, Caller, GResult, Tag};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Province {
    pub id: u32,
    pub land: bool,
    pub lake: bool,
    pub impassable: bool,
    /// Not under fog of war; false while visibility is not current.
    pub visible: bool,
    pub owner: Option<Tag>,
    pub controller: Option<Tag>,
}

/// A province reachable over land, possibly across a strait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Neighbor {
    pub to: u32,
    pub strait: bool,
}

/// Every province, indexed by ID; index 0 is a placeholder with nothing set.
pub fn provinces(_: Caller) -> GResult<Vec<Province>> {
    let m = game()?;
    let ctx = context(m)?;
    let visible = events::visibility_current(m, &ctx);
    with_state(|s| s.geography.provinces(m, &ctx, visible))
}

/// The passable land provinces next to a province; none for a province an
/// army cannot stand on.
pub fn neighbors(_: Caller, province: u32) -> GResult<Vec<Neighbor>> {
    let m = game()?;
    let ctx = context(m)?;
    with_state(|s| s.geography.neighbors(m, &ctx, province))
}
