//! Words in the player language's order where the game fixes English order in
//! code: dates, people's names, battle names. Like the Unicode module it is
//! part of the framework itself, and invisible to extensions.
//!
//! A language words these in its own templates in the framework's
//! localisation, which only a language that orders them differently from the
//! game needs ([`TEMPLATES`]); for any other the game's own text stands.
use crate::memory::Memory;
use crate::points::Point;
use crate::unicode::text_loops;

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

/// The bytes of the `std::string` at `string`, read without trusting it.
fn read_string(m: &Memory, string: usize) -> crate::Result<Vec<u8>> {
    let (size, capacity) = (m.u64(string + 0x10)? as usize, m.u64(string + 0x18)?);
    if size > 1 << 16 {
        return Err(format!("implausible string length {size}"));
    }
    let data = if capacity < 16 { string } else { m.ptr(string)? };
    if size == 0 { Ok(Vec::new()) } else { m.bytes(data, size) }
}

/// Makes the game's `std::string` at `string` hold `bytes`, through the
/// game's own append, so the game frees it as usual.
///
/// # Safety
/// `string` is a constructed `std::string` of the game's.
unsafe fn set_string(string: *mut u8, bytes: &[u8]) {
    let data = if *(string.add(0x18) as *const usize) < 16 { string } else { *(string as *const *mut u8) };
    *data = 0;
    *(string.add(0x10) as *mut usize) = 0;
    text_loops::append(string as usize, bytes);
}

pub fn points() -> Vec<Point> {
    let mut points = dates::points();
    points.extend(names::points());
    points.extend(battles::points());
    points
}
