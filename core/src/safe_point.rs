//! The safe points (GLOSSARY) where extensions and remote control run, on the
//! game's main thread:
//!
//! * In a game, after the map manager's frame, also while paused
//!   (`game::events`): extensions hear what happened, then remote control
//!   runs.
//! * Just before the main thread presents a frame (`framework_ui::hook`), when
//!   remote control did not run at a game frame since the last one. It runs
//!   there what needs no game. In the main menu, every page of it (`ui`'s
//!   main menu frame), it answers what needs a game; elsewhere, such as while
//!   a game is being set up or not ready, that waits for a game frame.
//!
//! While a save loads the main thread does neither, so nothing runs. The
//! world (`objects::campaign::globals::WORLD`) is no sign of a game: the main
//! menu has one too.
use crate::{ext, game, remote};
use std::sync::atomic::{AtomicBool, Ordering};

/// Remote control ran at a game frame since the last frame was presented.
static GAME_FRAME: AtomicBool = AtomicBool::new(false);
/// The main menu ran since the last frame was presented.
static MAIN_MENU: AtomicBool = AtomicBool::new(false);

/// The main menu's frame, on any of its pages.
pub fn main_menu_frame() {
    MAIN_MENU.store(true, Ordering::Relaxed);
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
            remote::run_in_game(&frame);
        }
        // A frame the game is not ready in comes and goes: the frame's
        // presenting decides.
        Err(fail) if fail.status == game::Status::NotReady => return,
        // No game to play (observing, say).
        Err(fail) => remote::run_outside(Some(fail)),
    }
    GAME_FRAME.store(true, Ordering::Relaxed);
}

/// Just before the main thread presents a frame.
pub fn presented() {
    let main_menu = MAIN_MENU.swap(false, Ordering::Relaxed);
    if GAME_FRAME.swap(false, Ordering::Relaxed) || crate::memory().is_none() {
        return;
    }
    remote::run_outside(main_menu.then(|| game::Fail { status: game::Status::NotInGame, message: "the main menu is open".into() }));
}
