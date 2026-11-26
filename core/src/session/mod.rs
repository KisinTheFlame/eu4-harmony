//! Session lifecycle state machine and extension save-data persistence hooks.

pub(crate) mod saves;
pub(crate) mod sessions;

use crate::native::points::Point;

pub(crate) fn points() -> Vec<Point> {
    let mut all = sessions::points();
    all.extend(saves::points());
    all
}
