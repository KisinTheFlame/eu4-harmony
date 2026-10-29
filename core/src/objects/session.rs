//! The game's session (Clausewitz's `CSession`, `session.cpp`), which takes
//! the commands the player gives and carries them out at the next map
//! manager's frame (docs/research/recruiting.md).

/// The session's class, by vtable RVA.
pub const CLASS: usize = 0x1d8c900;

/// The map manager's method that answers its session (its vtable's `+0x80`):
/// `[[map manager + 0x330] + 0x370]`.
pub const OF_MAP_MANAGER: usize = 0x811aa0;
pub const MAP_MANAGER_SLOT: usize = 0x80;

use super::campaign::classes::MAP_MANAGER;
use crate::points::Point;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x14c841cusize, "48 8d 05 dd 44 8c 00 48 89 01", "session class").uses(&[CLASS]),
        Point::method(MAP_MANAGER, MAP_MANAGER_SLOT, OF_MAP_MANAGER, "map manager session"),
        Point::keep(OF_MAP_MANAGER, "48 8b 81 30 03 00 00 48 8b 80 70 03 00 00 c3", "map manager session"),
    ]
}
