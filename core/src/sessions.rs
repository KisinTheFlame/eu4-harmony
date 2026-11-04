//! Sessions (GLOSSARY): where the game is, whether a session is going on
//! and whether a save is being read, and what begins and ends a session,
//! told to every part of the core that keeps something of one, in one order.
//!
//! The hooks that see a session begin or end call in here, and only here:
//! the safe points (`crate::safe_point`) with each game frame and where the
//! safe point is, the save loader (`crate::saves`) as it begins and ends to
//! read a save, leaving a session (`crate::game::menu::leave`) as the session
//! ends and as the world is reset, and the game's restart
//! (`crate::game::time`). A [`Lifecycle`] keeps the state and says what
//! happened ([`Happening`]); [`tell`] hands each happening to those that hear
//! it, outside the lock, as their handlers may ask where the game is:
//!
//! - A session began, at its first safe frame: the input held for the
//!   session before is let go (`game::input`). Extensions and remote control
//!   hear it at that frame with the rest of what happened, in its order
//!   (`game::Frame::heard`).
//! - The session ended, while its state is still the game's: remote control
//!   keeps it as an event, then every extension hears it (`crate::lua`). A
//!   session ends once: a second end of the same is not told, and a session
//!   that begins while another is going on ends that one first.
//! - The campaign changes, as a save begins to be read: the capability
//!   layer forgets the old one (`game::events`), and remote control's
//!   requests sent from then on belong to the next session.
//! - The world was reset, without a save: the capability layer forgets the
//!   old session as at a load.
//! - The game restarts: the saves compressed in the background are put in
//!   their place, and remote control answers every request while it can.
use crate::game;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// Why a session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// For the main menu.
    Return,
    /// For a save loaded from the session.
    Load,
    /// For a new game started from the session.
    NewGame,
}

impl End {
    /// The word extensions hear it by.
    pub fn word(self) -> &'static str {
        match self {
            End::Return => "return",
            End::Load => "load",
            End::NewGame => "new_game",
        }
    }
}

/// Where the safe point that is about to run remote control is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// A game's frame (also while the player only observes).
    Game,
    /// The main menu's frame ran since the last frame was presented; the
    /// main menu object.
    MainMenu(usize),
    /// Neither: a game being set up or loaded, or a frame that is not ready.
    Elsewhere,
}

/// One thing that happened to sessions, as [`tell`] hands it on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Happening {
    /// A session began, with its number.
    Began(u64),
    /// The session with this number ended, for this reason.
    Ended(u64, End),
    /// A save began to be read: the next game frame is of another campaign.
    CampaignChanges,
    /// The world was put back as a new start has it, without a save.
    WorldReset,
    /// The game is about to start `eu4.exe` again, which ends this process.
    Restarting,
}

/// How long a game's frames must have failed to read, as no game the player
/// can play, before they count as such. Observing lasts; but a game's first
/// frames may fail for a moment as it settles (the player's country read
/// before its tag matches, say), and a load or start waiting for its first
/// frame must not be refused for one of those, nor an advance be ended.
pub const UNPLAYABLE_AFTER: Duration = Duration::from_secs(1);

/// What is known of sessions, and how each thing that happens to them
/// changes it.
#[derive(Debug)]
pub struct Lifecycle {
    /// The session going on, by its number.
    going_on: Option<u64>,
    at: At,
    /// How many saves are being read, one inside another or not.
    reading: usize,
    /// The saves the game began to read since it started.
    loads: usize,
    /// The game read the last save it began to read into its world.
    last_read: bool,
    /// Since when the game's frames have failed to read, and how many saves
    /// the game had begun to read then: another save loaded starts it again.
    failing: Option<(Instant, usize)>,
}

impl Lifecycle {
    pub const fn new() -> Self {
        Self { going_on: None, at: At::Elsewhere, reading: 0, loads: 0, last_read: false, failing: None }
    }

    /// A game's safe frame of `session` was read: a session begins when it
    /// is a new one, ending the one going on, if any.
    pub fn frame_read(&mut self, session: u64, new_session: bool) -> Vec<Happening> {
        self.failing = None;
        let mut happened = Vec::new();
        if new_session && self.going_on != Some(session) {
            if let Some(old) = self.going_on.take() {
                happened.push(Happening::Ended(old, End::Load));
            }
            self.going_on = Some(session);
            happened.push(Happening::Began(session));
        }
        happened
    }

    /// A game's frame failed to read at `now`: whether its frames have
    /// failed for long enough to count as no game the player can play.
    pub fn frame_failed(&mut self, now: Instant) -> bool {
        match self.failing {
            Some((since, loads)) if loads == self.loads => now.duration_since(since) >= UNPLAYABLE_AFTER,
            _ => {
                self.failing = Some((now, self.loads));
                false
            }
        }
    }

    /// The session going on is being left, for `end`.
    pub fn ending(&mut self, end: End) -> Vec<Happening> {
        self.going_on.take().map(|session| Happening::Ended(session, end)).into_iter().collect()
    }

    /// The game begins to read a save.
    pub fn load_began(&mut self) -> Vec<Happening> {
        self.reading += 1;
        self.loads += 1;
        self.last_read = false;
        vec![Happening::CampaignChanges]
    }

    /// The game read the save it is reading into its world, or not.
    pub fn load_read(&mut self, read: bool) {
        self.last_read = read;
    }

    /// The game is done reading a save, whatever came of it.
    pub fn load_ended(&mut self) {
        self.reading = self.reading.saturating_sub(1);
    }

    pub fn world_reset(&mut self) -> Vec<Happening> {
        vec![Happening::WorldReset]
    }

    pub fn restarting(&mut self) -> Vec<Happening> {
        vec![Happening::Restarting]
    }
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

static LIFECYCLE: Mutex<Lifecycle> = Mutex::new(Lifecycle::new());

fn lifecycle<T>(f: impl FnOnce(&mut Lifecycle) -> T) -> T {
    f(&mut LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Hands each happening to those that hear it, in order.
fn tell(happened: Vec<Happening>) {
    for happening in happened {
        match happening {
            Happening::Began(_) => game::input::session_began(),
            Happening::Ended(session, end) => {
                log!("session {session} ending ({})", end.word());
                crate::remote::session_ended(end);
                crate::lua::session_ended(end);
            }
            Happening::CampaignChanges => {
                game::events::load_begin();
                crate::remote::campaign_changes();
            }
            Happening::WorldReset => game::events::world_reset(),
            Happening::Restarting => {
                crate::saves::background::wait();
                crate::remote::restarting();
            }
        }
    }
}

/// A game's safe frame was read, before extensions and remote control hear
/// what happened in it.
pub fn frame_read(frame: &game::Frame) {
    tell(lifecycle(|l| l.frame_read(frame.session, frame.new_session)));
}

/// A game's frame failed to read: whether its frames have failed for long
/// enough to count as no game the player can play ([`UNPLAYABLE_AFTER`]).
pub fn frame_failed() -> bool {
    lifecycle(|l| l.frame_failed(Instant::now()))
}

/// The session going on is being left, for `end`.
pub fn ending(end: End) {
    tell(lifecycle(|l| l.ending(end)));
}

/// The game begins to read a save.
pub fn load_began() {
    tell(lifecycle(Lifecycle::load_began));
}

/// The game read the save it is reading into its world, or not.
pub fn load_read(read: bool) {
    lifecycle(|l| l.load_read(read));
}

/// The game is done reading a save, whatever came of it.
pub fn load_ended() {
    lifecycle(Lifecycle::load_ended);
}

/// The world was put back as a new start has it, with its history (an
/// in-process return, the single player setup left): the next game begun on
/// it is a new session, though the game reads nothing for it.
pub fn world_reset() {
    tell(lifecycle(Lifecycle::world_reset));
}

/// Before the game starts `eu4.exe` again, whoever asked for it.
pub fn restarting() {
    tell(lifecycle(Lifecycle::restarting));
}

/// Says where the safe point that is about to run remote control is.
pub fn set_at(at: At) {
    lifecycle(|l| l.at = at);
}

/// Where the last safe point was.
pub fn at() -> At {
    lifecycle(|l| l.at)
}

/// Whether a session is going on.
pub fn going_on() -> bool {
    lifecycle(|l| l.going_on.is_some())
}

/// Whether the game is reading a save.
pub fn loading() -> bool {
    lifecycle(|l| l.reading != 0)
}

/// How many saves the game began to read since it started.
pub fn loads() -> usize {
    lifecycle(|l| l.loads)
}

/// Whether the game read the last save it began to read: its loader said
/// so, for the world it read into, outside a daily update.
pub fn last_load_read() -> bool {
    lifecycle(|l| l.last_read)
}

/// Sets the session going on, for a test of what depends on it.
#[cfg(test)]
pub(crate) fn set_going_on(session: Option<u64>) {
    lifecycle(|l| l.going_on = session);
}

#[cfg(test)]
mod tests {
    use super::*;
    use Happening::*;

    #[test]
    fn a_session_begins_at_its_first_frame_and_ends_once() {
        let mut l = Lifecycle::new();
        assert_eq!(l.load_began(), vec![CampaignChanges]);
        l.load_read(true);
        l.load_ended();
        assert_eq!((l.reading, l.loads, l.last_read), (0, 1, true));
        assert_eq!(l.frame_read(1, true), vec![Began(1)]);
        assert_eq!(l.frame_read(1, false), vec![]);
        assert_eq!(l.ending(End::Return), vec![Ended(1, End::Return)]);
        // Leaving again, as when an ironman exit save failed and the player
        // tries once more: the session was told it ended already.
        assert_eq!(l.ending(End::Load), vec![]);
        assert_eq!(l.world_reset(), vec![WorldReset]);
        assert_eq!(l.frame_read(2, true), vec![Began(2)]);
    }

    #[test]
    fn a_session_that_begins_while_another_goes_on_ends_that_one_first() {
        let mut l = Lifecycle::new();
        l.frame_read(4, true);
        assert_eq!(l.frame_read(5, true), vec![Ended(4, End::Load), Began(5)]);
    }

    #[test]
    fn a_save_read_inside_another_is_done_when_both_are() {
        let mut l = Lifecycle::new();
        l.load_began();
        l.load_began();
        l.load_ended();
        assert_eq!(l.reading, 1);
        l.load_ended();
        l.load_ended();
        assert_eq!((l.reading, l.loads, l.last_read), (0, 2, false));
    }

    #[test]
    fn frames_that_fail_count_as_unplayable_once_they_last() {
        let start = Instant::now();
        let mut l = Lifecycle::new();
        l.loads = 3;
        assert!(!l.frame_failed(start));
        assert!(!l.frame_failed(start + UNPLAYABLE_AFTER / 2));
        assert!(l.frame_failed(start + UNPLAYABLE_AFTER));
        // A frame that reads starts it again.
        l.frame_read(1, false);
        assert!(!l.frame_failed(start + UNPLAYABLE_AFTER * 2));
        assert!(l.frame_failed(start + UNPLAYABLE_AFTER * 3));
        // So does another save loaded.
        l.load_began();
        l.load_ended();
        assert!(!l.frame_failed(start + UNPLAYABLE_AFTER * 4));
        assert!(!l.frame_failed(start + UNPLAYABLE_AFTER * 4 + UNPLAYABLE_AFTER / 2));
        assert!(l.frame_failed(start + UNPLAYABLE_AFTER * 5));
    }
}
