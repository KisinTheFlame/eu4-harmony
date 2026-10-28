//! The safe points (GLOSSARY) where extensions run, on the game's main
//! thread: in a game, after the map manager's frame, also while paused
//! (`game::events`). While a save loads the main thread does not get there,
//! so nothing runs.
use crate::{ext, game};

/// After the map manager's frame.
pub fn game_frame() {
    let Some(m) = crate::memory().copied() else { return };
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    if let Ok(frame) = game::begin_frame(&m) {
        ext::frame(&frame);
    }
}
