//! The campaign being played: which session, the date, whether it is paused,
//! and the player's country.
use super::{context, events, game, paused, Caller, GResult, Tag};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// Increases whenever a different campaign state is loaded or started.
    pub session: u64,
    /// Game date as a monotonic number; the same value throughout one day.
    pub date: i32,
    pub paused: bool,
    /// Visibility (fog of war) reflects the current date.
    pub visibility_current: bool,
    pub player: Tag,
}

pub fn session(_: Caller) -> GResult<Session> {
    let m = game()?;
    let ctx = context(m)?;
    Ok(Session {
        session: events::session(),
        date: ctx.date,
        paused: paused(m, &ctx)?,
        visibility_current: events::visibility_current(m, &ctx),
        player: ctx.player,
    })
}
