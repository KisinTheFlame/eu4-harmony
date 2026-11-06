//! The game being played.
use super::{tag, Convert};
use crate::game::{self};
use eu4_harmony_remote::{
    session::{Session, SessionAnswer, SessionRequest},
    Date,
};

layer! {
    run SessionRequest => SessionAnswer;
    Get => fn session() -> game::session::Session = caller game::session::session;
}

/// The session as remote clients read it.
impl Convert<Session> for game::session::Session {
    fn convert(self) -> Session {
        Session { number: self.session, player: tag(self.player), date: Date(self.date) }
    }
}
