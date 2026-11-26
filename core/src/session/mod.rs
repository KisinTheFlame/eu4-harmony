//! Session lifecycle state machine, extension save-data persistence hooks, and
//! crash record capture.

pub(crate) mod crash;
pub(crate) mod saves;
pub(crate) mod sessions;

use crate::native::points::Point;

pub(crate) fn points() -> Vec<Point> {
    let mut all = sessions::points();
    all.extend(saves::points());
    all.extend(crash::points());
    all
}
