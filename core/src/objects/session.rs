//! The game's session (Clausewitz's `CSession`, `session.cpp`), which takes
//! the commands the player gives and carries them out at the map manager's
//! frame (docs/research/recruiting.md).

/// The session's class, by vtable RVA.
pub const CLASS: usize = 0x1d8c900;

/// The map manager's method that answers its session (its vtable's `+0x80`):
/// `[[map manager + 0x330] + 0x370]`.
pub const OF_MAP_MANAGER: usize = 0x811aa0;
pub const MAP_MANAGER_SLOT: usize = 0x80;

/// The session's own fields.
pub mod session {
    /// Set while the map manager's frame runs (u8), except in the player
    /// mode `campaign::world::SWITCHED` outside the session's own update.
    /// Outside it, a session not joined to another as a client takes only
    /// the commands that say they may come at any time, which the recruit
    /// command does not.
    pub const IN_FRAME: usize = 0x80d;
}

use super::campaign::{classes::MAP_MANAGER, globals::WORLD, world};
use crate::points::Point;

pub fn points() -> Vec<Point> {
    use self::session::*;
    vec![
        Point::keep(0x14c841cusize, "48 8d 05 dd 44 8c 00 48 89 01", "session class").uses(&[CLASS]),
        Point::method(MAP_MANAGER, MAP_MANAGER_SLOT, OF_MAP_MANAGER, "map manager session"),
        Point::keep(OF_MAP_MANAGER, "48 8b 81 30 03 00 00 48 8b 80 70 03 00 00 c3", "map manager session"),
        // The map manager's frame: before the session's update the flag is
        // set; after it, in the switched player mode, cleared again.
        Point::keep(0x813f80usize, "49 8b 85 30 03 00 00 48 8b 88 70 03 00 00 c6 81 0d 08 00 00 01", "session in frame").uses(&[IN_FRAME]),
        Point::keep(
            0x813ff4usize,
            "48 8b 05 7d be b2 01 80 b8 66 1e 00 00 07 75 15 49 8b 85 30 03 00 00 48 8b 88 70 03 00 00 c6 81 0d 08 00 00 00",
            "session in frame, switched",
        )
        .uses(&[WORLD, world::PLAYER_MODE, world::SWITCHED as usize, IN_FRAME]),
    ]
}
