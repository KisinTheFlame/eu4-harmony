//! The safe points (GLOSSARY) where extensions and remote control run, on the
//! game's main thread:
//!
//! * In a game, after the map manager's frame, also while paused
//!   (`game::events`): extensions hear what happened, then remote control
//!   runs.
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
//! [`UNPLAYABLE_AFTER`]; until then what needs a game waits, as at a frame
//! that is not ready.
use crate::{
    ext,
    game::{self, menu::At},
    remote,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

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
        game::menu::main_menu_frame(m, frontend);
    }
}

/// How long a game's frames must have failed to read, as no game the player
/// can play, before they count as such. Observing lasts; but a game's first
/// frames may fail for a moment as it settles (the player's country read
/// before its tag matches, say), and a load or start waiting for its first
/// frame must not be refused for one of those, nor an advance be ended.
pub const UNPLAYABLE_AFTER: Duration = Duration::from_secs(1);

/// Since when the game's frames have failed to read, and how many saves the
/// game had begun to read then: another save loaded starts it again.
#[derive(Default)]
struct Unplayable {
    since: Option<(Instant, usize)>,
}

impl Unplayable {
    /// A frame failed to read at `now`, `loads` saves read: whether the
    /// frames have failed for long enough to count as unplayable.
    fn failed(&mut self, now: Instant, loads: usize) -> bool {
        match self.since {
            Some((since, then)) if then == loads => now.duration_since(since) >= UNPLAYABLE_AFTER,
            _ => {
                self.since = Some((now, loads));
                false
            }
        }
    }

    /// A frame read: the player can play.
    fn read(&mut self) {
        self.since = None;
    }
}

static UNPLAYABLE: Mutex<Unplayable> = Mutex::new(Unplayable { since: None });

/// After the map manager's frame.
pub fn game_frame() {
    let Some(m) = crate::memory().copied() else { return };
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    let unplayable = || UNPLAYABLE.lock().unwrap_or_else(|e| e.into_inner());
    match game::begin_frame(&m) {
        Ok(frame) => {
            unplayable().read();
            ext::frame(&frame);
            game::menu::set_at(At::Game);
            remote::run_in_game(&frame);
        }
        // A frame the game is not ready in comes and goes: the frame's
        // presenting decides.
        Err(fail) if fail.status == game::Status::NotReady => return,
        // No game to play (observing, say), once it lasts.
        Err(fail) => {
            game::menu::set_at(At::Game);
            if unplayable().failed(Instant::now(), crate::saves::loads()) {
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
    game::menu::set_at(if frontend != 0 { At::MainMenu(frontend) } else { At::Elsewhere });
    let open = frontend != 0 && !game::menu::starting(m, frontend);
    remote::run_outside(open.then(|| game::Fail { status: game::Status::NotInGame, message: "the main menu is open".into() }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_that_fail_count_as_unplayable_once_they_last() {
        let start = Instant::now();
        let mut u = Unplayable::default();
        assert!(!u.failed(start, 3));
        assert!(!u.failed(start + UNPLAYABLE_AFTER / 2, 3));
        assert!(u.failed(start + UNPLAYABLE_AFTER, 3));
        // A frame that reads starts it again.
        u.read();
        assert!(!u.failed(start + UNPLAYABLE_AFTER * 2, 3));
        assert!(u.failed(start + UNPLAYABLE_AFTER * 3, 3));
        // So does another save loaded.
        assert!(!u.failed(start + UNPLAYABLE_AFTER * 4, 4));
        assert!(!u.failed(start + UNPLAYABLE_AFTER * 4 + UNPLAYABLE_AFTER / 2, 4));
        assert!(u.failed(start + UNPLAYABLE_AFTER * 5, 4));
    }
}
