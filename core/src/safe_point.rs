//! The safe points (GLOSSARY) where extensions and remote control run, on the
//! game's main thread:
//!
//! * In a game, after the map manager's frame, also while paused
//!   (`game::events`): extensions hear what happened, then remote control
//!   runs.
//! * Just before the main thread presents a frame (`framework_ui::hook`), when
//!   no game frame ran since the last one: in the main menu, and while a game
//!   is being set up or ended. Remote control runs there what needs no game.
//!
//! While a save loads the main thread does neither, so nothing runs.
use crate::{ext, game, objects::campaign::globals, remote};
use std::sync::atomic::{AtomicBool, Ordering};

/// A game frame ran since the last frame was presented.
static GAME_FRAME: AtomicBool = AtomicBool::new(false);

/// After the map manager's frame.
pub fn game_frame() {
    let Some(m) = crate::memory().copied() else { return };
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    GAME_FRAME.store(true, Ordering::Relaxed);
    match game::begin_frame(&m) {
        Ok(frame) => {
            ext::frame(&frame);
            remote::run_in_game(&frame);
        }
        // A frame the game is not ready in comes and goes; requests wait for
        // the next. Otherwise there is no game to play (observing, say).
        Err(fail) if fail.status == game::Status::NotReady => remote::run_outside(None),
        Err(fail) => remote::run_outside(Some(fail)),
    }
}

/// Just before the main thread presents a frame.
pub fn presented() {
    if GAME_FRAME.swap(false, Ordering::Relaxed) {
        return;
    }
    let Some(m) = crate::memory() else { return };
    // No world, no campaign (`globals::WORLD`): requests that need one are
    // answered now. With one, a game frame is about to come, or has just
    // gone; they wait.
    match m.ptr(m.at(globals::WORLD)) {
        Ok(0) => remote::run_outside(Some(game::Fail { status: game::Status::NotInGame, message: "no game is running".into() })),
        _ => remote::run_outside(None),
    }
}
