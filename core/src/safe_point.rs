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
use crate::{
    ext,
    game::{self, menu::At},
    remote,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// After the map manager's frame.
pub fn game_frame() {
    let Some(m) = crate::memory().copied() else { return };
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    match game::begin_frame(&m) {
        Ok(frame) => {
            ext::frame(&frame);
            game::menu::set_at(At::Game);
            remote::run_in_game(&frame);
        }
        // A frame the game is not ready in comes and goes: the frame's
        // presenting decides.
        Err(fail) if fail.status == game::Status::NotReady => return,
        // No game to play (observing, say).
        Err(fail) => {
            game::menu::set_at(At::Game);
            remote::run_unplayable(fail);
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
