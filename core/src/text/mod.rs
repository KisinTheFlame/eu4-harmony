//! Game text rendering and phrasing: built-in Unicode engine (glyph pages,
//! system fonts, edit boxes, map names) and localised game wording (dates,
//! battles, names).

pub(crate) mod unicode;
pub(crate) mod wording;

use crate::native::points::Point;

pub(crate) fn points() -> Vec<Point> {
    let mut all = unicode::points();
    all.extend(wording::points());
    all
}
