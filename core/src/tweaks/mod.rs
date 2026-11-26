//! Standalone vanilla behavior tweaks: achievement rules, music loudness and
//! volume curve, smooth camera zoom, and hidden-start window management.

pub(crate) mod achievements;
pub(crate) mod camera_zoom;
pub(crate) mod hidden;
pub(crate) mod sound;

use crate::native::points::Point;

pub(crate) fn points() -> Vec<Point> {
    let mut all = achievements::points();
    all.extend(sound::points());
    all.extend(camera_zoom::points());
    all.extend(hidden::points());
    all
}
