//! Where the game keeps what the framework reads and writes inside its
//! objects. Each field, global and class is named once here, and the
//! modules' points keep an instruction of the game's own that uses it, so a
//! build that moved one fails the startup check instead of the framework
//! reading the wrong place. The points say which numbers they use, and a
//! test checks that their bytes carry them.
pub mod campaign;

use crate::points::Point;

pub fn points() -> Vec<Point> {
    campaign::points()
}
