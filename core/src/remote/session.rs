//! The campaign being played.
use super::{process, tag, Live};
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    session::{Session, SessionAnswer, SessionRequest},
    Date, Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn session(&mut self) -> GResult<game::session::Session>;
}

impl Layer for Live {
    fn session(&mut self) -> GResult<game::session::Session> {
        game::session::session(Caller::Remote)
    }
}

pub fn run(request: SessionRequest, game: &mut (impl Layer + ?Sized)) -> Result<SessionAnswer, Error> {
    match request {
        SessionRequest::Get {} => Ok(SessionAnswer::Get(answer(game.session()?))),
    }
}

/// The session as remote clients read it.
pub fn answer(s: game::session::Session) -> Session {
    Session { process: process(), session: s.session, date: Date(s.date), paused: s.paused, visibility_current: s.visibility_current, player: tag(s.player) }
}
