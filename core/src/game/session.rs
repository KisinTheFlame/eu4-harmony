//! The campaign being played: which session, the date, whether it is paused,
//! and its player: the country played, or in observe mode the country in
//! view, if any.
use super::{events, game, paused, player::Player, world_context, Caller, GResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    /// Increases whenever a different campaign state is loaded or started.
    pub session: u64,
    /// Game date as a monotonic number; the same value throughout one day.
    pub date: i32,
    pub paused: bool,
    /// Visibility (fog of war) reflects the current date.
    pub visibility_current: bool,
    pub player: Player,
}

pub fn session(_: Caller) -> GResult<Session> {
    let m = game()?;
    let world = world_context(m)?;
    Ok(Session {
        session: crate::sessions::number(),
        date: world.date,
        paused: paused(m, &world)?,
        visibility_current: events::visibility_current(m, &world),
        player: world.player,
    })
}
