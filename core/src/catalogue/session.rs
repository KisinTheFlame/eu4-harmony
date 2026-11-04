//! The game being played.
use super::tag;
use crate::game::{self};
use eu4_harmony_remote::{
    session::{Session, SessionAnswer, SessionRequest},
    Date, Error,
};

layer! {
    fn session() -> game::session::Session = game::session::session;
}

pub fn run(request: SessionRequest, game: &mut (impl Layer + ?Sized)) -> Result<SessionAnswer, Error> {
    match request {
        SessionRequest::Get {} => Ok(SessionAnswer::Get(answer(game.session()?))),
    }
}

/// The session as remote clients read it.
pub fn answer(s: game::session::Session) -> Session {
    Session { number: s.session, player: tag(s.player), date: Date(s.date) }
}
