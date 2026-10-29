//! The application, which runs the game's main loop and switches between the
//! main menu and a game by its idler switch (docs/research/main-menu.md,
//! docs/research/notice-window.md). The main menu keeps it at
//! `gui::main_menu::APPLICATION`.

/// An idler switch is pending, u8: the application's frame switches to the
/// idler it keeps, such as a game's.
pub const SWITCH_PENDING: usize = 0x48;

use crate::points::Point;

pub fn points() -> Vec<Point> {
    // The idler switch.
    vec![Point::keep(0x14c2542usize, "c6 41 48 01", "idler switch pending").uses(&[SWITCH_PENDING])]
}
