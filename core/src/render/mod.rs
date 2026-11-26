//! Presentation and rendering infrastructure: Direct3D 9 device primitives,
//! the separate drawing thread (presentation lock, crossings, deferred calls),
//! `Present`/`Reset` orchestration, virtual input playback, GPU screenshot
//! readback, and the Win32 `WndProc` hook.

pub(crate) mod d3d9;
pub(crate) mod drawing;
pub(crate) mod input;
pub(crate) mod present;
pub(crate) mod screenshot;
pub(crate) mod window_input;

use crate::native::points::Point;

pub(crate) fn points() -> Vec<Point> {
    let mut all = present::points();
    all.extend(input::points());
    all
}
