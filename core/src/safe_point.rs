//! The safe points (GLOSSARY) where extensions and remote control run, on the
//! game's main thread, and what they hear there, in one order:
//!
//! * In a game, after the map manager's frame, also while paused
//!   (`game::events`): extensions hear what happened, then remote control
//!   keeps it and runs.
//! * Just before the main thread presents a frame (`framework_ui::hook`), when
//!   remote control did not run at a game frame since the last one. It runs
//!   there what needs no game. In the main menu, every page of it (`ui`'s
//!   main menu frame), it answers what needs a game, unless a game is being
//!   started from it (`game::menu::starting`): a save read, the single
//!   player setup's start made, the game's start accepted, its idler switch
//!   pending. Then, and elsewhere, such as while a game is being set up or
//!   not ready, that waits for a game frame.
//!
//! While a save loads the main thread does neither, so nothing runs. The
//! world (`objects::campaign::globals::WORLD`) is no sign of a game: the main
//! menu has one too.
//!
//! A game's frame that cannot be read counts as one the player cannot play
//! (observing, say) only once the game's frames have been so for
//! `sessions::UNPLAYABLE_AFTER`; until then what needs a game waits, as at a
//! frame that is not ready. What begins and ends a session, and where the
//! safe point is, is told to the sessions (`crate::sessions`).
//!
//! The extensions (`crate::lua`) and remote control (`crate::remote`) are the
//! two [`Listener`]s of [`Listeners`], which says in one place what they hear
//! and in what order:
//!
//! - A session's end, as the sessions tell it, while its state is still the
//!   game's: remote control first, which keeps it as an event, and its
//!   requests sent from then on belong to the next session; then the
//!   extensions, whose handlers may change the game. A listener that cannot
//!   hear it then, an extension inside a call of its own (a console command
//!   it ran loaded a save, say), hears it at the next point it is called from,
//!   the start or the end of its part of a game's safe frame: so a session
//!   always ends before the next begins.
//! - A game's safe frame: what happened since the last one, in its order
//!   (`game::Frame::heard`), first to the extensions, then to remote control,
//!   which keeps what they just heard and runs.
use crate::{
    game::{self, Heard},
    lua, remote, repairs,
    sessions::{self, At, End},
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

/// What hears the sessions' ends and the game's safe frames: the extensions
/// and remote control, each through an adapter of its own (`lua::Extensions`,
/// `remote::Remote`).
pub trait Listener {
    /// Named in the log.
    fn name(&self) -> &'static str;

    /// Whether it can hear a session's end now: not while it is inside a
    /// call of its own, which would run inside itself.
    fn can_hear(&self) -> bool {
        true
    }

    /// The session going on ended, for `end`.
    fn ended(&self, end: End);

    /// A game's safe frame of the session `session`: what happened since the
    /// last one, in order.
    fn frame(&self, session: u64, heard: &[Heard]);
}

/// One listener, with the ends it could not hear when they came.
struct Ear<L> {
    listener: L,
    missed: Mutex<Vec<End>>,
}

impl<L: Listener> Ear<L> {
    const fn new(listener: L) -> Self {
        Self { listener, missed: Mutex::new(Vec::new()) }
    }

    fn ended(&self, end: End) {
        if !self.listener.can_hear() {
            log!("{}: the session's end ({end:?}) is heard once the call it ended in is done", self.listener.name());
            self.missed.lock().unwrap_or_else(|e| e.into_inner()).push(end);
            return;
        }
        self.catch_up();
        self.listener.ended(end);
    }

    /// Tells the ends it could not hear, in order. The lock is let go first:
    /// what the listener does may end another session.
    fn catch_up(&self) {
        let missed = std::mem::take(&mut *self.missed.lock().unwrap_or_else(|e| e.into_inner()));
        for end in missed {
            self.listener.ended(end);
        }
    }

    fn frame(&self, session: u64, heard: &[Heard]) {
        self.catch_up();
        self.listener.frame(session, heard);
        self.catch_up();
    }
}

/// The extensions and remote control, as they hear what the safe points and
/// the sessions tell.
pub struct Listeners<E, R> {
    extensions: Ear<E>,
    remote: Ear<R>,
}

impl<E: Listener, R: Listener> Listeners<E, R> {
    pub const fn new(extensions: E, remote: R) -> Self {
        Self { extensions: Ear::new(extensions), remote: Ear::new(remote) }
    }

    /// The session going on ended, for `end`: remote control hears it first.
    pub fn ended(&self, end: End) {
        self.remote.ended(end);
        self.extensions.ended(end);
    }

    /// A game's safe frame of `session`: the extensions hear what happened,
    /// then remote control.
    pub fn frame(&self, session: u64, heard: &[Heard]) {
        self.extensions.frame(session, heard);
        self.remote.frame(session, heard);
    }
}

static LISTENERS: Listeners<lua::Extensions, remote::Remote> = Listeners::new(lua::Extensions, remote::Remote);

/// The session going on ended, for `end` (`crate::sessions`).
pub(crate) fn session_ended(end: End) {
    LISTENERS.ended(end);
}

/// Remote control ran at a game frame since the last frame was presented.
static GAME_FRAME: AtomicBool = AtomicBool::new(false);
/// The main menu object, when the main menu ran since the last frame was
/// presented; 0 otherwise.
static MAIN_MENU: AtomicUsize = AtomicUsize::new(0);

/// The end of the main menu's frame, on any of its pages: `frontend` is the
/// main menu object. A load or start remote control asked for runs here,
/// where the game's own clicks run.
pub fn main_menu_frame(frontend: usize) {
    MAIN_MENU.store(frontend, Ordering::Relaxed);
    if let Some(m) = crate::memory() {
        sessions::frame_ran();
        repairs::main_menu_shown(m);
        game::menu::main_menu_frame(m, frontend);
    }
}

/// After the map manager's frame.
pub fn game_frame() {
    let Some(m) = crate::memory().copied() else { return };
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    // An in-game load remote control asked for takes the frame: the next
    // one is the new session's.
    if game::menu::game_frame(&m) {
        GAME_FRAME.store(true, Ordering::Relaxed);
        return;
    }
    match game::begin_frame(&m) {
        Ok(frame) => {
            sessions::frame_ran();
            #[cfg(feature = "dev-test-switches")]
            if frame.new_session() {
                game::commands::post_test_commands();
                if test_switch!(grow_pages) {
                    log!("test switch grow-pages: every font page grows");
                    crate::unicode::fill::want_room_everywhere();
                }
                if test_switch!(crash) {
                    log!("test switch crash: crashing the game");
                    // An access violation, which the game's crash handling takes.
                    unsafe { std::ptr::read_volatile(8 as *const u8) };
                }
                if test_switch!(panic) {
                    log!("test switch panic: panicking in framework code");
                    panic!("the test switch panic is on");
                }
            }
            let mut heard = game::event_costs::EVENTS.time(|| frame.heard());
            // The pop-ups extensions showed since the last frame.
            heard.extend(
                crate::windows::take_shown()
                    .into_iter()
                    .map(|(popup, from)| game::Heard { event: eu4_harmony_remote::events::Event::Popup { popup, from }, seen: true }),
            );
            LISTENERS.frame(frame.session(), &heard);
        }
        // A frame the game is not ready in comes and goes: the frame's
        // presenting decides.
        Err(fail) if fail.status == game::Status::NotReady => return,
        // No game to play (observing, say), once it lasts.
        Err(fail) => {
            sessions::set_at(At::Game);
            if sessions::frame_failed() {
                remote::run_unplayable(fail);
            } else {
                remote::run_outside(None);
            }
        }
    }
    GAME_FRAME.store(true, Ordering::Relaxed);
}

/// Just before the main thread presents a frame.
pub fn presented() {
    let frontend = MAIN_MENU.swap(0, Ordering::Relaxed);
    let Some(m) = crate::memory() else { return };
    if GAME_FRAME.swap(false, Ordering::Relaxed) {
        return;
    }
    sessions::set_at(if frontend != 0 { At::MainMenu(frontend) } else { At::Elsewhere });
    let open = frontend != 0 && !game::menu::starting(m, frontend);
    remote::run_outside(open.then(|| game::Fail { status: game::Status::NotInGame, message: "the main menu is open".into() }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_remote::events::Event;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    type Log = Rc<RefCell<Vec<String>>>;

    /// A listener that writes down what it hears; while `busy`, it is inside
    /// a call of its own; `during_frame` runs inside its frame, as a handler.
    struct Recorded {
        name: &'static str,
        log: Log,
        busy: Rc<Cell<bool>>,
        during_frame: RefCell<Option<Box<dyn Fn()>>>,
    }

    impl Recorded {
        fn new(name: &'static str, log: &Log) -> Self {
            Self { name, log: log.clone(), busy: Rc::default(), during_frame: RefCell::new(None) }
        }
    }

    impl Listener for Recorded {
        fn name(&self) -> &'static str {
            self.name
        }

        fn can_hear(&self) -> bool {
            !self.busy.get()
        }

        fn ended(&self, end: End) {
            self.log.borrow_mut().push(format!("{} ended {end:?}", self.name));
        }

        fn frame(&self, session: u64, heard: &[Heard]) {
            let events: Vec<String> = heard.iter().map(|h| h.event.kind().to_string()).collect();
            self.log.borrow_mut().push(format!("{} frame {session} [{}]", self.name, events.join(" ")));
            if let Some(during) = self.during_frame.borrow().as_ref() {
                during();
            }
        }
    }

    fn listeners(log: &Log) -> &'static Listeners<Recorded, Recorded> {
        Box::leak(Box::new(Listeners::new(Recorded::new("extensions", log), Recorded::new("remote", log))))
    }

    fn began(session: u64) -> Vec<Heard> {
        vec![Heard { event: Event::Session { session }, seen: true }]
    }

    #[test]
    fn remote_control_hears_an_end_first_and_the_extensions_a_frame_first() {
        let log = Log::default();
        let l = listeners(&log);
        l.ended(End::Return);
        l.frame(2, &began(2));
        assert_eq!(*log.borrow(), ["remote ended Return", "extensions ended Return", "extensions frame 2 [session]", "remote frame 2 [session]"]);
    }

    #[test]
    fn an_end_inside_an_extensions_call_is_heard_before_the_next_session_begins() {
        let log = Log::default();
        let l = listeners(&log);
        // A console command an extension ran loaded a save.
        l.extensions.listener.busy.set(true);
        l.ended(End::Load);
        l.extensions.listener.busy.set(false);
        assert_eq!(*log.borrow(), ["remote ended Load"]);
        l.frame(3, &began(3));
        assert_eq!(*log.borrow(), ["remote ended Load", "extensions ended Load", "extensions frame 3 [session]", "remote frame 3 [session]"]);
    }

    #[test]
    fn an_end_inside_the_extensions_frame_is_heard_at_its_end() {
        let log = Log::default();
        let l = listeners(&log);
        // A handler of the frame left the session, as `on_frame` running the
        // game console's `tag` would.
        let busy = l.extensions.listener.busy.clone();
        *l.extensions.listener.during_frame.borrow_mut() = Some(Box::new(move || {
            busy.set(true);
            l.ended(End::Country);
            busy.set(false);
        }));
        l.frame(4, &[]);
        assert_eq!(*log.borrow(), ["extensions frame 4 []", "remote ended Country", "extensions ended Country", "remote frame 4 []"]);
    }

    #[test]
    fn ends_missed_are_heard_in_order_before_the_next() {
        let log = Log::default();
        let l = listeners(&log);
        l.extensions.listener.busy.set(true);
        l.ended(End::Load);
        l.extensions.listener.busy.set(false);
        l.ended(End::Return);
        assert_eq!(*log.borrow(), ["remote ended Load", "remote ended Return", "extensions ended Load", "extensions ended Return"]);
    }
}
