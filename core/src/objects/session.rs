//! The game's session (Clausewitz's `CSession`, `session.cpp`), which takes
//! the commands the player gives and carries them out at its next update
//! (docs/research/recruiting.md, docs/research/commands.md).

/// The session's class, by vtable RVA.
pub const CLASS: usize = 0x1d8c900;

/// The map manager's method that answers its session (its vtable's
/// [`map_manager::SESSION`]): `[[map manager + 0x330] + 0x370]`.
pub const OF_MAP_MANAGER: usize = 0x811aa0;

/// Its fields, as its update `0x14ca360`, its post `0x14ca2d0` and its
/// execution `0x14ca870` read them.
pub mod fields {
    /// Its state, u32: the update runs its queued commands in
    /// [`super::RUNS_QUEUE`]'s states and in [`super::RUNNING`].
    pub const STATE: usize = 0x48;
    /// Its mode, u32: commands' steps before and after they execute are
    /// called in modes 4, 6 and 7.
    pub const MODE: usize = 0x50;
    /// The tick (i32) and the sender (i32) its post stamps a command with.
    pub const TICK: usize = 0x68;
    pub const SENDER: usize = 0x8c;
    /// Set while a command executes.
    pub const EXECUTING: usize = 0x7d4;
    /// Set when the update has to start the session's next stage first, in
    /// [`super::RUNNING`].
    pub const STARTING: usize = 0x840;
    /// Set while commands are held back, as during a hot join.
    pub const HELD: usize = 0x841;
}

/// The states in which the update runs the queued commands, one bit each;
/// and the one state in which it runs them otherwise, once
/// [`fields::STARTING`] is clear.
pub const RUNS_QUEUE: u32 = 0xc014;
pub const RUNNING: u32 = 6;

use super::campaign::{classes::MAP_MANAGER, map_manager};
use crate::points::Point;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x14c841cusize, "48 8d 05 dd 44 8c 00 48 89 01", "session class").uses(&[CLASS]),
        Point::method(MAP_MANAGER, map_manager::SESSION, OF_MAP_MANAGER, "map manager session"),
        Point::keep(OF_MAP_MANAGER, "48 8b 81 30 03 00 00 48 8b 80 70 03 00 00 c3", "map manager session"),
    ]
}
