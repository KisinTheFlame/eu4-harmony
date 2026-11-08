//! Pop-ups: the extensions' pop-ups open in the game, and pressing one of
//! their buttons as the player does (docs/plans/extension-windows.md). What they
//! are is `crate::windows`'; a pop-up is the extensions' and the player's
//! alike, so remote control's view leaves nothing of them out.
use super::{GResult, WorldSession};
use eu4_harmony_remote::popups::{args, Popup};

/// The refusals of pressing a pop-up's button, in the framework's words.
mod checks {
    use crate::game::{Fail, Status};

    pub fn not_open(popup: u32) -> Fail {
        Fail { status: Status::NotFound, message: format!("no pop-up {popup} is open") }
    }

    pub fn no_button(popup: u32, count: usize) -> Fail {
        Fail { status: Status::InvalidArgument, message: format!("pop-up {popup} has buttons 1 to {count}") }
    }
}

/// The pop-ups open, oldest first.
pub fn list(_: WorldSession) -> GResult<Vec<Popup>> {
    Ok(crate::windows::popups())
}

/// Presses a pop-up's button, as the player does: the pop-up closes, and its
/// extension has the answer at the next frame.
pub fn answer(_: WorldSession, args::Answer { popup, button }: args::Answer) -> GResult<()> {
    let open = crate::windows::popups();
    let Some(found) = open.iter().find(|p| p.popup == popup) else { return Err(checks::not_open(popup)) };
    if button == 0 || button as usize > found.buttons.len() {
        return Err(checks::no_button(popup, found.buttons.len()));
    }
    crate::windows::press_popup(popup, button).map_err(|_| checks::not_open(popup))
}
