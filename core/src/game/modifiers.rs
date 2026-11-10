//! A country's modifiers as the game's own getters read them: the values
//! its getters of one modifier take, and its yes-or-no modifiers; and a set
//! of modifiers read pair by pair, each modifier named by its key
//! (docs/research/modifiers.md). Every group that asks a country's
//! modifiers asks them here.
use super::{thousandths, GResult};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{country, modifier_set, modifier_type, modifiers},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::countries::ModifierValue;

/// `(country + MODIFIERS)`: the country's modifiers, brought up to date;
/// their values are at its answer's [`VALUES`].
pub(super) const OF_COUNTRY: GameFunction<1> = GameFunction::at(0x93d750);
/// `(values, modifier) -> bool`: whether a yes-or-no modifier is set.
pub(super) const HAS: GameFunction<2> = GameFunction::at(0x8e7b60);
/// Where the getter's answer holds the values the game's getters of one
/// modifier read.
pub const VALUES: usize = modifiers::OWN;

/// The most kinds of modifier believed: the game builds 0x287.
const MOST_TYPES: usize = 0x1000;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(OF_COUNTRY, "48 89 5c 24 10 48 89 7c 24 18 55 48 8d 6c 24 b0 48 81 ec 50 01 00 00 48", "country modifiers"),
        Point::keep(HAS, "48 89 5c 24 08 57 48 83 ec 20 4c 8b 0d 0f 05 a7 01 48 8b f9 8b da 4c 8d", "has a modifier"),
        // The getter of one modifier's value: by the modifier's place, or
        // its pairs searched.
        Point::keep(0x8e6be2usize, "41 81 f8 86 02 00 00 7f 0e 48 8b 41 58 4d 63 c8 46 8b 54 c8 04", "modifier set by modifier").uses(&[
            modifier_set::NONE as usize,
            modifier_set::PAIRS,
            modifier_set::PAIR / 2,
        ]),
        Point::keep(0x8e6bf9usize, "48 63 41 64 45 33 d2 85 c0 7e 23", "modifier set pairs").uses(&[modifier_set::COUNT]),
        // The kinds' records as they are built: the index times 0x60 into
        // the vector, and a record's index read back.
        Point::keep(0x8f09eausize, "48 8d 0c 5b 48 c1 e1 05 48 03 0d 87 76 a6 01", "modifier types").uses(&[modifier_type::TYPES]),
        Point::keep(0x8e7b76usize, "4c 8d 04 5b 49 c1 e0 05 4b 8b 4c 08 40 47 8b 54 08 54", "modifier type's index").uses(&[modifier_type::INDEX]),
    ]
}

/// The country's modifiers, brought up to date, as the game's getter
/// answers them.
pub fn of_country(m: &Memory, country: usize) -> GResult<usize> {
    match OF_COUNTRY.call(m, [country + country::MODIFIERS])? {
        0 => Err("no modifier values".to_string().into()),
        modifiers => Ok(modifiers),
    }
}

/// The values of the modifiers of the country at `country`, as the game's
/// getters of one modifier take them.
pub fn values(m: &Memory, country: usize) -> GResult<usize> {
    Ok(of_country(m, country)? + VALUES)
}

/// Whether the yes-or-no modifier `modifier`, by its index, is set for the
/// country at `country`.
pub fn has(m: &Memory, country: usize, modifier: usize) -> GResult<bool> {
    let values = values(m, country)?;
    Ok(HAS.call(m, [values, modifier])? & 0xff != 0)
}

/// The kinds of modifier, by index: the vector's begin and how many.
fn types(m: &Memory) -> GResult<(usize, usize)> {
    let begin = m.ptr(m.at(modifier_type::TYPES))?;
    let count = crate::containers::span(begin, m.ptr(m.at(modifier_type::TYPES) + 8)?, modifier_type::SIZE, MOST_TYPES)?;
    Ok((begin, count))
}

/// The key of the modifier of index `index`, such as `discipline`.
pub fn key(m: &Memory, index: i32) -> GResult<String> {
    let (begin, count) = types(m)?;
    if !(0..count as i32).contains(&index) {
        return Err(format!("modifier {index} of {count}").into());
    }
    let record = begin + index as usize * modifier_type::SIZE;
    if m.i32(record + modifier_type::INDEX)? != index {
        return Err(format!("modifier {index}'s record holds another").into());
    }
    Ok(game_string::key(m, record + modifier_type::KEY)?)
}

/// The modifiers the set at `set` holds, each with what it gives, in the
/// set's order; a place kept by modifier that holds none, and a value of
/// nothing, are left out.
pub fn pairs(m: &Memory, set: usize) -> GResult<Vec<(i32, i32)>> {
    let count = m.i32(set + modifier_set::COUNT)?;
    if !(0..=MOST_TYPES as i32).contains(&count) {
        return Err(format!("a modifier set of {count} pairs").into());
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let bytes = m.bytes(m.ptr(set + modifier_set::PAIRS)?, count as usize * modifier_set::PAIR)?;
    Ok(bytes
        .chunks_exact(modifier_set::PAIR)
        .map(|pair| (i32::from_le_bytes(pair[..4].try_into().unwrap()), i32::from_le_bytes(pair[4..].try_into().unwrap())))
        .filter(|&(modifier, value)| modifier != modifier_set::NONE && value != 0)
        .collect())
}

/// What the set at `set` gives, each modifier by its key.
pub fn named(m: &Memory, set: usize) -> GResult<Vec<ModifierValue>> {
    pairs(m, set)?.into_iter().map(|(modifier, value)| Ok(ModifierValue { modifier: key(m, modifier)?, value: thousandths(value) })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::buffers::{Image, Object};

    /// A set kept as pairs and one kept by modifier, with an empty place and
    /// a value of nothing: each read as the modifiers it gives, by key.
    #[test]
    fn a_set_s_modifiers_are_read_by_their_keys() {
        let image = Image::new();
        let m = image.memory();
        let mut table = Object::new(modifier_type::SIZE * 3);
        for (i, key) in ["discipline", "land_morale", "prestige"].into_iter().enumerate() {
            let record = i * modifier_type::SIZE;
            table = table.string(record + modifier_type::KEY, key).i32(record + modifier_type::INDEX, i as i32);
        }
        image.u64(modifier_type::TYPES, table.at() as u64).u64(modifier_type::TYPES + 8, (table.at() + modifier_type::SIZE * 3) as u64);
        let pairs = |pairs: &[(i32, i32)]| {
            let array = Object::new(pairs.len().max(1) * modifier_set::PAIR);
            for (i, (modifier, value)) in pairs.iter().enumerate() {
                array.set(i * modifier_set::PAIR, &modifier.to_le_bytes());
                array.set(i * modifier_set::PAIR + 4, &value.to_le_bytes());
            }
            Object::new(modifier_set::COUNT + 4).points_to(modifier_set::PAIRS, array).i32(modifier_set::COUNT, pairs.len() as i32)
        };
        let as_pairs = pairs(&[(2, 500), (0, 50), (1, 0)]);
        assert_eq!(
            named(m, as_pairs.at()).unwrap(),
            [ModifierValue { modifier: "prestige".into(), value: 0.5 }, ModifierValue { modifier: "discipline".into(), value: 0.05 }]
        );
        let by_modifier = pairs(&[(0, -100), (modifier_set::NONE, 0), (2, 1000)]).u8(modifier_set::BY_MODIFIER, 1);
        assert_eq!(
            named(m, by_modifier.at()).unwrap(),
            [ModifierValue { modifier: "discipline".into(), value: -0.1 }, ModifierValue { modifier: "prestige".into(), value: 1.0 }]
        );
        assert!(key(m, 3).is_err());
    }
}
