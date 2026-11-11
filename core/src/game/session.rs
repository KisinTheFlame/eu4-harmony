//! The campaign being played: which session, the date, whether it is paused,
//! and its player: the country played, or in observe mode the country in
//! view, if any.
use super::{countries, signals, game, paused, player, player::Player, world_context, Caller, GResult, WorldSession};
use eu4_harmony_remote::session::args;

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

/// Puts `country` in view in observe mode, as the player does (GLOSSARY:
/// Country in view), and answers the session as it is then.
pub fn view(s: WorldSession, args::View { country }: args::View) -> GResult<Session> {
    let tag = countries::named(s.m, &s.world, country)?;
    player::put_in_view(s.m, &s.world, tag)?;
    session(s.caller)
}

/// Takes the country in view out, and answers the session as it is then.
pub fn leave_view(s: WorldSession) -> GResult<Session> {
    player::leave_view(s.m, &s.world)?;
    session(s.caller)
}

pub fn session(_: Caller) -> GResult<Session> {
    let m = game()?;
    let world = world_context(m)?;
    Ok(Session {
        session: crate::sessions::number(),
        date: world.date,
        paused: paused(m, &world)?,
        visibility_current: signals::visibility_current(m, &world),
        player: world.player,
    })
}

/// The group's dependency points: none of its own.
pub fn points() -> Vec<crate::points::Point> {
    Vec::new()
}
