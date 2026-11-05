//! A country's modifiers as the game's own getters read them: the values
//! its getters of one modifier take, and its yes-or-no modifiers. Every
//! group that asks a country's modifiers asks them here.
use super::GResult;
use crate::{memory::Memory, objects::campaign::country, points::Point};

/// `(country + MODIFIERS)`: the country's modifiers, brought up to date;
/// their values are at its answer's [`VALUES`].
const OF_COUNTRY: usize = 0x93d750;
/// `(values, modifier) -> bool`: whether a yes-or-no modifier is set.
const HAS: usize = 0x8e7b60;
/// Where the getter's answer holds the values the game's getters of one
/// modifier read.
pub const VALUES: usize = 8;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(OF_COUNTRY, "48 89 5c 24 10 48 89 7c 24 18 55 48 8d 6c 24 b0 48 81 ec 50 01 00 00 48", "country modifiers"),
        Point::keep(HAS, "48 89 5c 24 08 57 48 83 ec 20 4c 8b 0d 0f 05 a7 01 48 8b f9 8b da 4c 8d", "has a modifier"),
    ]
}

/// The values of the modifiers of the country at `country`, as the game's
/// getters of one modifier take them.
pub fn values(m: &Memory, country: usize) -> GResult<usize> {
    match m.call(m.at(OF_COUNTRY), &[country + country::MODIFIERS])? {
        0 => Err("no modifier values".to_string().into()),
        modifiers => Ok(modifiers + VALUES),
    }
}

/// Whether the yes-or-no modifier `modifier`, by its index, is set for the
/// country at `country`.
pub fn has(m: &Memory, country: usize, modifier: usize) -> GResult<bool> {
    let values = values(m, country)?;
    Ok(m.call(m.at(HAS), &[values, modifier])? & 0xff != 0)
}
