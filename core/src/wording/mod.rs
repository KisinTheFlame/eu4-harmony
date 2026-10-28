//! Words in the player language's order where the game fixes English order in
//! code: dates, people's names, battle names. Like the Unicode module it is
//! part of the framework itself, and invisible to extensions.
//!
//! A language words these in its own templates in the framework's
//! localisation, which only a language that orders them differently from the
//! game needs ([`TEMPLATES`]); for any other the game's own text stands.
use crate::points::Point;

pub mod battles;
pub mod dates;
pub mod names;

/// Every template key, with the `$NAME$` values it may use.
pub const TEMPLATES: &[(&str, &[&str])] = &[
    ("EU4HARMONY_DATE", &["YEAR", "MONTH_NUMBER", "DAY"]),
    ("EU4HARMONY_DATE_MONTH", &["YEAR", "MONTH_NUMBER"]),
    ("EU4HARMONY_DATE_YEAR", &["YEAR"]),
    ("EU4HARMONY_DATE_YEAR_BC", &["YEAR"]),
    ("EU4HARMONY_NAME_FAMILY_FIRST", &["FAMILY", "GIVEN"]),
    ("EU4HARMONY_BATTLE", &["PLACE", "KIND"]),
    ("EU4HARMONY_SIEGE", &["PLACE", "KIND"]),
    ("EU4HARMONY_OCCUPATION", &["PLACE", "KIND"]),
];

pub fn points() -> Vec<Point> {
    let mut points = dates::points();
    points.extend(names::points());
    points.extend(battles::points());
    points
}
