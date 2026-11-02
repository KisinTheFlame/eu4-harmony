//! The application, which runs the game's main loop and switches between the
//! main menu and a game by its idler switch (docs/research/main-menu.md,
//! docs/research/notice-window.md). The main menu keeps it at
//! `gui::main_menu::APPLICATION`.

/// An idler switch is pending, u8: the application's frame switches to the
/// idler it keeps, such as a game's.
pub const SWITCH_PENDING: usize = 0x48;
/// What the main menu is made with, besides the application: startup hands
/// these two to its constructor.
pub const MAIN_MENU_ARGS: (usize, usize) = (0x350, 0x358);
/// The continue box's information (`0x210a80` copies it), and a task of
/// startup's that fills it in, which whatever touches it waits for first.
pub const CONTINUE: usize = 0x5d8;
pub const CONTINUE_TASK: usize = 0x5d0;

use crate::points::Point;

pub fn points() -> Vec<Point> {
    vec![
        // The idler switch.
        Point::keep(0x14c2542usize, "c6 41 48 01", "idler switch pending").uses(&[SWITCH_PENDING]),
        // Startup making the main menu.
        Point::keep(0x205841usize, "4d 8b cf 4d 8b 87 58 03 00 00 49 8b 97 50 03 00 00", "main menu made with").uses(&[MAIN_MENU_ARGS.0, MAIN_MENU_ARGS.1]),
        // Setting the continue information, after its task.
        Point::keep(0x2110d0usize, "48 8b 89 d0 05 00 00 48 85 c9 74 05 e8 df 28 50 01", "continue task").uses(&[CONTINUE_TASK]),
        Point::keep(0x210aa8usize, "48 8b b6 d8 05 00 00", "continue information").uses(&[CONTINUE]),
    ]
}
