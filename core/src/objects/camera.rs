//! The map camera, and what the camera capabilities read beside it: where
//! the map manager keeps the camera and the map's grid of province IDs
//! (docs/research/camera.md).
//!
//! The camera works in the map's world: x across the map from its left edge,
//! z up the map from its bottom edge, both in pixels of the province map, and
//! y the height above it.

/// The camera's class, by vtable RVA.
pub const CLASS: usize = 0x1d57348;

/// The map manager's camera, a pointer at this offset of the map manager
/// (`campaign::map_manager`).
pub const IN_MAP_MANAGER: usize = 0x1200;

/// The camera's own fields. A point is three f32: x, y (the height) and z.
pub mod camera {
    /// Where the camera is and the point it looks at, as drawn now.
    pub const POSITION: usize = 0x67c;
    pub const LOOK_AT: usize = 0x688;
    /// Where it is going and the point it will look at: the game moves the
    /// camera towards them a little every frame.
    pub const GOAL_POSITION: usize = 0x694;
    pub const GOAL_LOOK_AT: usize = 0x6a0;
    /// The lowest and the highest it may be (i32 each), from the defines
    /// `NFrontend.CAMERA_MIN_HEIGHT` and `CAMERA_MAX_HEIGHT`, which the
    /// constructor reads from the globals [`MIN_HEIGHT_DEFINE`] and
    /// [`MAX_HEIGHT_DEFINE`] (f32 each).
    pub const MIN_HEIGHT: usize = 0x6ec;
    pub const MAX_HEIGHT: usize = 0x6f0;
    pub const MIN_HEIGHT_DEFINE: usize = 0x233e714;
    pub const MAX_HEIGHT_DEFINE: usize = 0x233e770;
    /// Nonzero while the camera may not move (u8): the game's own moves
    /// leave it where it is.
    pub const LOCKED: usize = 0x758;
}

/// The map (`campaign::globals::MAP`): the province at each pixel.
pub mod grid {
    /// The province IDs (i16), row after row from the bottom row up, each
    /// row [`WIDTH`] long; 0 or less where there is no province.
    pub const IDS: usize = 0x10;
    /// The map's size in pixels (i32 each).
    pub const WIDTH: usize = 0x28;
    pub const HEIGHT: usize = 0x2c;
}

use super::campaign::globals::MAP;
use crate::points::Point;

pub fn points() -> Vec<Point> {
    use self::{camera::*, grid::*};
    vec![
        // The camera's constructor: its class and the heights it may take.
        Point::keep(0x109b871usize, "48 8d 05 d0 ba cb 00 48 89 01", "camera class").uses(&[CLASS]),
        Point::keep(0x109bbbausize, "f3 0f 2c 05 52 2b 2a 01 89 87 ec 06 00 00 f3 0f 2c 05 a0 2b 2a 01 89 87 f0 06 00 00", "camera heights")
            .uses(&[MIN_HEIGHT_DEFINE, MIN_HEIGHT, MAX_HEIGHT_DEFINE, MAX_HEIGHT]),
        // Keeping the camera on the map: where it is and looks now, then
        // where it is going and will look.
        Point::keep(
            0x109d0ffusize,
            "4c 8d 81 88 06 00 00 4c 8b cb 48 8d 91 7c 06 00 00 e8 4b fe ff ff 4c 8d 87 a0 06 00 00 4c 8b cb 48 8d 97 94 06 00 00",
            "camera points",
        )
        .uses(&[LOOK_AT, POSITION, GOAL_LOOK_AT, GOAL_POSITION]),
        // The map manager's camera getter.
        Point::keep(0x5c44e0usize, "48 8b 81 00 12 00 00 c3", "map manager camera").uses(&[IN_MAP_MANAGER]),
        // Finding the province at a point of the map: the map, then its size
        // and the province ID at the point.
        Point::keep(0x109315ausize, "4c 8b 3d 77 cd 2a 01", "map for the province grid").uses(&[MAP]),
        Point::keep(0x10931a5usize, "41 8b 47 28 3b c8 7d 2c 41 3b 5f 2c 7d 26 0f af d8 03 d9 8b cb 49 8b 47 10 48 0f bf 14 58", "province grid")
            .uses(&[WIDTH, HEIGHT, IDS]),
    ]
}
