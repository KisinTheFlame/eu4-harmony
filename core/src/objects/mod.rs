//! Where the game keeps what the framework reads and writes inside its
//! objects. Each field, global and class is named once here, and the
//! modules' points keep an instruction of the game's own that uses it, so a
//! build that moved one fails the startup check instead of the framework
//! reading the wrong place. The points say which numbers they use, and a
//! test checks that their bytes carry them.
pub mod application;
pub mod camera;
pub mod campaign;
pub mod diplomacy;
pub mod gui;
pub mod session;
pub mod setup;
pub mod text;

/// The game's doubly linked lists: a header with the first and the last
/// node, their count (i32) and a byte set while the list is being walked,
/// then nodes of [`NODE_SIZE`] bytes with the item, the previous and the
/// next node, and a byte set on a node removed meanwhile.
pub mod list {
    pub const HEAD: usize = 0;
    pub const TAIL: usize = 8;
    pub const COUNT: usize = 0x10;
    pub const CHANGING: usize = 0x14;
    pub const SIZE: usize = 0x18;
    pub const NODE_SIZE: usize = 0x20;
    pub const ITEM: usize = 0;
    pub const PREVIOUS: usize = 8;
    pub const NEXT: usize = 0x10;
    pub const REMOVED: usize = 0x18;
}

use crate::points::Point;

pub fn points() -> Vec<Point> {
    let mut all = campaign::points();
    all.extend(gui::points());
    all.extend(text::points());
    all.extend(camera::points());
    all.extend(session::points());
    all.extend(setup::points());
    all.extend(application::points());
    all.extend(diplomacy::points());
    all
}
