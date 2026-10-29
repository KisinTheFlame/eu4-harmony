//! The single player setup, the main menu's page 3 (`gamesetup.cpp`), and
//! the text the `-start_tag` launch option leaves for it
//! (docs/research/main-menu.md).

/// The setup page's class, by vtable RVA.
pub const CLASS: usize = 0x1d5e278;
/// Its interface, which the main menu's pages hold
/// (`gui::main_menu::PAGES`), this many bytes into the page; and that
/// interface's class.
pub const INTERFACE: usize = 0x28;
pub const INTERFACE_CLASS: usize = 0x1d5e320;
/// The main menu it belongs to.
pub const MAIN_MENU: usize = 0x30;
/// The save chosen in it, a `std::string` holding its PhysFS path; whether
/// that save is in the cloud, u8; and the save list's selection (i32, -1 for
/// none).
pub const SAVE_PATH: usize = 0x338;
pub const CLOUD_SAVE: usize = 0x358;
pub const SAVE_INDEX: usize = 0x35c;
/// The country the save chosen is played as, a tag.
pub const SAVE_PLAYER: usize = 0xc48;
/// It started a game, u8: set by its start paths, cleared only by its
/// constructor.
pub const STARTED: usize = 0x2cb0;

/// The country the single player setup selects and starts as when it is
/// entered, a `std::string` global (`-start_tag`); the setup empties it.
pub const START_TAG: usize = 0x1fd39a8;

use crate::{game_string, points::Point};

pub fn points() -> Vec<Point> {
    vec![
        // The constructor: its main menu, its classes and its interface.
        Point::keep(0x110878busize, "49 89 74 24 30", "single player setup main menu").uses(&[MAIN_MENU]),
        Point::keep(
            0x11087b0usize,
            "48 8d 05 c1 5a c5 00 49 89 04 24 48 8d 05 d6 5a c5 00 48 89 03 48 8d 05 54 5b c5 00 49 89 44 24 28",
            "single player setup classes",
        )
        .uses(&[CLASS, INTERFACE_CLASS, INTERFACE]),
        // The continue box writing the save it continues into the setup.
        Point::keep(0x10ef790usize, "41 89 b4 24 5c 03 00 00", "continue box save index").uses(&[SAVE_INDEX]),
        Point::keep(0x10ef7afusize, "49 8d 8c 24 38 03 00 00 4c 8b c6 e8 51 59 fa fe 41 88 bc 24 58 03 00 00", "continue box save path")
            .uses(&[SAVE_PATH, CLOUD_SAVE]),
        Point::keep(0x10ef7e3usize, "49 89 9c 24 48 0c 00 00 49 8b cc e8 4d 17 04 00", "continue box save player").uses(&[SAVE_PLAYER]),
        // Its start.
        Point::keep(0x11202a5usize, "c6 81 b0 2c 00 00 01", "single player setup started").uses(&[STARTED]),
        // Entering the setup: whether `-start_tag` left a country, and its
        // text.
        Point::keep(0x110e7b4usize, "83 3d fd 51 ec 00 00", "start tag set").uses(&[START_TAG + game_string::SIZE]),
        Point::keep(
            0x110e7e6usize,
            "48 8d 3d bb 51 ec 00 4c 8b c7 48 83 3d c8 51 ec 00 10 4c 0f 43 05 a8 51 ec 00",
            "start tag text",
        )
        .uses(&[START_TAG, START_TAG + game_string::CAPACITY]),
    ]
}
