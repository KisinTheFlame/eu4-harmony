//! What the game's pick of a name from a list weighed for both sexes
//! overflows, in vanilla too: the random number, which it multiplies by 1000
//! before taking it modulo the list's weight, so that about half the draws
//! come out negative and pick the list's first name. The number is taken
//! modulo the weight as it is.
use crate::points::Point;

/// Where the pick of a name from a list (`0xda9880`), weighing its male and
/// female names each by a factor, turns its random number (`esi`, 0 to
/// 2^31 - 1) into fixed point, `imul eax, esi, 1000`, before it takes it
/// modulo the list's weight in fixed point. A new heir is named by it from
/// the country's ruler names (`0xa55cf0`), unless its sex is already set;
/// the female names weigh by the country's female heir chance.
const NAME_DRAW: usize = 0xda9981;

/// The repair as the world reset sees it ([`super::REPAIRS`]): it counts
/// nothing.
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: None };

pub(super) fn points() -> Vec<Point> {
    vec![
        // The random number goes into the division as it is (`mov eax,
        // esi`); the remainder of it by the weight is where the walk down the
        // names begins, each name's weight taken off until it goes below 0.
        Point::write(NAME_DRAW, "69 c6 e8 03 00 00", "name pick's draw in fixed point", "8b c6 0f 1f 40 00"),
        Point::keep(0xda9995usize, "41 03 c9 99 4d 8b 4a 18 f7 f9 8b ca", "name pick's draw modulo the weight"),
        Point::keep(0xda99dcusize, "2b ca 78 06", "name pick's walk down the names"),
    ]
}
