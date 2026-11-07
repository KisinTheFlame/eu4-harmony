//! The game being played.
use super::{tag, Convert};
use crate::game::{self};
use eu4_harmony_remote::{
    session::{Session, SessionAnswer, SessionRequest},
    CountryTag, Date,
};

layer! {
    run SessionRequest => SessionAnswer;
    Get => fn session() -> game::session::Session = caller game::session::session;
    View => fn view(country: CountryTag) -> game::session::Session = world game::session::view;
    LeaveView => fn leave_view() -> game::session::Session = world game::session::leave_view;
}

/// The session as remote clients read it.
impl Convert<Session> for game::session::Session {
    fn convert(self) -> Session {
        Session { number: self.session, player: self.player.acting().map(tag), observing: self.player.observing(), date: Date(self.date) }
    }
}
