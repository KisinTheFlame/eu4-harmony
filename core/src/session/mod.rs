//! Session lifecycle state machine, session transitions (in-game load,
//! in-process return, main-menu launch, restart), and extension save-data
//! persistence hooks.

pub(crate) mod saves;
pub(crate) mod sessions;
pub(crate) mod transition;

use crate::native::points::Point;

pub(crate) fn points() -> Vec<Point> {
    let mut all = sessions::points();
    all.extend(saves::points());
    all.extend(transition::points());
    all
}
