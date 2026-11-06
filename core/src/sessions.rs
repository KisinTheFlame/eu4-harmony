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
//! - A game frame shows another session than the last frame did: a save
//!   was read, the world reset, or the frame shows another world or player
//!   than the last. Sessions are numbered here, and only here. What the
//!   capability layer keeps of the last frame's session, its map, routes,
//!   last day and wars, and the changes it collected for it, is let go
//!   (`game`, `game::events`), and the map's coloring put back (`crate::map`),
//!   before anything hears that session ended.
//! - A session began, at its first safe frame: the input held for the
//!   session before is let go (`game::input`). Extensions and remote control
//!   hear it at that frame with the rest of what happened, in its order
//!   (`game::Frame::heard`).
//! - The session ended, while its state is still the game's: remote control
//!   keeps it as an event, and its requests sent from then on belong to the
//!   next session, then every extension hears it (`crate::lua`). A
//!   session ends once: a second end of the same is not told, and a session
//!   that begins while another is going on ends that one first, for another
//!   country when nothing was read or reset since (the game console's `tag`).
//! - The campaign changes, as a save begins to be read: the capability
//!   layer forgets the old one (`game::events`), routes turned off for the
//!   old world come back (`game::orders`), and remote control's requests
//!   sent from then on belong to the next session.
//! - The world was reset, without a save: the capability layer forgets the
//!   old session and routes come back, as at a load, and what the game's
//!   history leaves of the old session is put right (`crate::repairs`).
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
    /// The player's own country changed, on the same world: the game
    /// console's `tag` or `observe`, say, but not a change of the country in
    /// view. The next session begins at once.
    Country,
}

impl End {
    /// What remote control and extensions are told the session ended for.
    pub fn reason(self) -> eu4_harmony_remote::events::SessionEnd {
        use eu4_harmony_remote::events::SessionEnd;
        match self {
            End::Return => SessionEnd::Return,
            End::Load => SessionEnd::Load,
            End::NewGame => SessionEnd::NewGame,
            End::Country => SessionEnd::Country,
        }
    }

    /// The event extensions and remote control hear it by.
    pub fn event(self) -> eu4_harmony_remote::events::Event {
        eu4_harmony_remote::events::Event::SessionEnded { reason: self.reason() }
    }
}

/// Where the safe point that is about to run remote control is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// A game's frame (also in observe mode).
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
    /// A game frame shows the session with this number, not the last
    /// frame's: what was kept of that one is let go.
    Forgotten(u64),
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
/// can play, before they count as such. A game's first frames may fail for a
/// moment as it settles (the player's country read
/// before its tag matches, say), and a load or start waiting for its first
/// frame must not be refused for one of those, nor an advance be ended.
pub const UNPLAYABLE_AFTER: Duration = Duration::from_secs(1);

/// What a game's safe frame shows of sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The session's number.
    pub session: u64,
    /// The session differs from the last frame's.
    pub new_session: bool,
}

/// What is known of sessions, and how each thing that happens to them
/// changes it.
#[derive(Debug)]
pub struct Lifecycle {
    /// The number of the session the game is in or comes to next: a save
    /// read, a world reset, or a frame of another world or player than the
    /// last begins another.
    number: u64,
    /// The world and the player's own tag the last game frame showed.
    identity: Option<(usize, u64)>,
    /// The number of the session the last game frame showed.
    framed: u64,
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
    /// Whether a save was begun to be read or the world reset since the
    /// session going on began.
    replaced: bool,
}

impl Lifecycle {
    pub const fn new() -> Self {
        Self { number: 0, identity: None, framed: 0, going_on: None, at: At::Elsewhere, reading: 0, loads: 0, last_read: false, failing: None, replaced: false }
    }

    /// A game's safe frame of the world and the player's own tag `identity`
    /// was read: what it shows of sessions. A session begins when it is a
    /// new one, what was kept of the last frame's let go first and the one
    /// going on ended, if any. That one was not left by a way out, which
    /// would have ended it: with nothing read or reset since it began, the
    /// player's country changed, and otherwise the game's world was
    /// replaced as at a load.
    pub fn frame_read(&mut self, identity: (usize, u64)) -> (Seen, Vec<Happening>) {
        self.failing = None;
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.number += 1;
        }
        let (session, new_session) = (self.number, self.framed != self.number);
        self.framed = session;
        let mut happened = Vec::new();
        if new_session {
            happened.push(Happening::Forgotten(session));
            if let Some(old) = self.going_on.take() {
                happened.push(Happening::Ended(old, if self.replaced { End::Load } else { End::Country }));
            }
            self.going_on = Some(session);
            self.replaced = false;
            happened.push(Happening::Began(session));
        }
        (Seen { session, new_session }, happened)
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
        self.number += 1;
        self.reading += 1;
        self.loads += 1;
        self.last_read = false;
        self.replaced = true;
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
        self.number += 1;
        self.replaced = true;
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
            Happening::Forgotten(_) => {
                game::session_forgotten();
                game::events::session_forgotten();
                crate::map::reset_session();
            }
            Happening::Began(_) => game::input::session_began(),
            Happening::Ended(session, end) => {
                log!("session {session} ending ({end:?})");
                game::event_costs::log_session();
                crate::remote::session_ended(end);
                crate::lua::session_ended(end);
            }
            Happening::CampaignChanges => {
                game::events::load_begin();
                game::orders::world_replaced();
                crate::remote::campaign_changes();
            }
            Happening::WorldReset => {
                game::events::world_reset();
                game::orders::world_replaced();
            }
            Happening::Restarting => {
                crate::saves::background::wait();
                crate::remote::restarting();
            }
        }
    }
}

/// A game's safe frame of the game `world` was read, before what happened
/// in it is collected: what it shows of sessions. A session is the world and
/// the player's own tag: a change of the country in view leaves it going on
/// (ADR observe-mode-acts-as-the-country-in-view).
pub fn frame_read(world: &game::World) -> Seen {
    let (seen, happened) = lifecycle(|l| l.frame_read((world.world, world.player.own().0)));
    tell(happened);
    seen
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
/// it is a new session, though the game reads nothing for it. What the
/// history leaves of the old session is put right with it
/// (`repairs::after_history`), so that no reset is told without it.
pub fn world_reset(m: &crate::memory::Memory) {
    tell(lifecycle(Lifecycle::world_reset));
    crate::repairs::after_history(m);
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

/// The number of the session the game is in, or comes to next once a save
/// began to be read or the world was reset.
pub fn number() -> u64 {
    lifecycle(|l| l.number)
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

    /// A world and the player's own tag, as a frame shows them.
    const SPAIN: (usize, u64) = (0x1000, 1);
    const FRANCE: (usize, u64) = (0x1000, 2);
    const OTHER_WORLD: (usize, u64) = (0x2000, 1);

    fn began(l: &mut Lifecycle, identity: (usize, u64)) -> Vec<Happening> {
        l.frame_read(identity).1
    }

    #[test]
    fn a_session_begins_at_its_first_frame_and_ends_once() {
        let mut l = Lifecycle::new();
        assert_eq!(l.load_began(), vec![CampaignChanges]);
        l.load_read(true);
        l.load_ended();
        assert_eq!((l.reading, l.loads, l.last_read), (0, 1, true));
        assert_eq!(l.frame_read(SPAIN), (Seen { session: 2, new_session: true }, vec![Forgotten(2), Began(2)]));
        assert_eq!(l.frame_read(SPAIN), (Seen { session: 2, new_session: false }, vec![]));
        assert_eq!(l.ending(End::Return), vec![Ended(2, End::Return)]);
        // Leaving again, as by a second way out within one frame: the
        // session was told it ended already. (An ironman exit save that
        // fails comes before the end, which is then not told at all.)
        assert_eq!(l.ending(End::Load), vec![]);
        assert_eq!(l.world_reset(), vec![WorldReset]);
        assert_eq!(began(&mut l, SPAIN), vec![Forgotten(3), Began(3)]);
    }

    #[test]
    fn a_session_that_begins_while_another_goes_on_ends_that_one_first() {
        let mut l = Lifecycle::new();
        began(&mut l, SPAIN);
        l.load_began();
        l.load_ended();
        assert_eq!(began(&mut l, SPAIN), vec![Forgotten(2), Ended(1, End::Load), Began(2)]);
        l.world_reset();
        assert_eq!(began(&mut l, OTHER_WORLD), vec![Forgotten(4), Ended(2, End::Load), Began(4)]);
    }

    #[test]
    fn a_session_that_begins_with_nothing_read_or_reset_ends_the_last_for_another_country() {
        // The game console's tag: the same world, the player's country changed.
        let mut l = Lifecycle::new();
        l.load_began();
        l.load_ended();
        began(&mut l, SPAIN);
        assert_eq!(began(&mut l, FRANCE), vec![Forgotten(3), Ended(2, End::Country), Began(3)]);
        assert_eq!(serde_json::to_value(End::Country.reason()).unwrap(), "country");
    }

    #[test]
    fn sessions_are_numbered_by_the_saves_read_the_resets_and_what_the_frames_show() {
        let mut l = Lifecycle::new();
        assert_eq!(l.frame_read(SPAIN).0, Seen { session: 1, new_session: true });
        // A save begins to be read: the next session's number at once.
        l.load_began();
        assert_eq!(l.number, 2);
        l.load_ended();
        // Read into the same world for the same country: that session.
        assert_eq!(l.frame_read(SPAIN).0, Seen { session: 2, new_session: true });
        // Into another world: one more.
        l.load_began();
        l.load_ended();
        assert_eq!(l.frame_read(OTHER_WORLD).0, Seen { session: 4, new_session: true });
        l.world_reset();
        assert_eq!(l.number, 5);
        assert_eq!(l.frame_read(OTHER_WORLD).0, Seen { session: 5, new_session: true });
        assert_eq!(l.frame_read(OTHER_WORLD).0, Seen { session: 5, new_session: false });
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
        l.frame_read(SPAIN);
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
