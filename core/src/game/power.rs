//! Monarch points spent as the game's panels spend them: the power spending
//! the game builds for a button, its cost as the panel shows it, and the
//! command the button posts (docs/research/power-spending.md). Shared by the
//! groups whose buttons spend points: technology, stability, the economy.
//! Also what the panels' tooltips are written with: the game's localisation
//! with values, and the conditions of `common/scripted_functions` that allow
//! some of these buttons.
use super::{
    commands::{self, Refused},
    conditions, fail, GResult, Status, Tag,
};
use crate::{game_string, memory::Memory, points::Point, unicode::game_text};
use eu4_harmony_remote::choices::PowerKind;
use std::ffi::CStr;

/// A power spending's class (`powerspending.cpp`).
const SPENDING_CLASS: usize = 0x1c55480;
/// Its size, and what each field holds: the class at `+0`, `+8` the 0x165
/// every one of the game's objects of this kind starts with, the kind of
/// spending, the kind of monarch power, a first and a second value (−1 for
/// none), the object it is about (none here), the ID of none from
/// [`NO_ID`], and whether it may leave the points below zero.
const SPENDING: usize = 0x38;
const OBJECT_MARK: u32 = 0x165;
const KIND: usize = 0x10;
const SKILL: usize = 0x14;
const DATA: usize = 0x18;
const SECOND: usize = 0x1c;
const ABOUT_ID: usize = 0x28;
/// The ID of none, which the game copies into each spending it builds.
const NO_ID: usize = 0x233fea8;
/// A spending's cost in whole monarch points: `(spending, tag, text, 0)`.
const COST: usize = 0x9526d0;
/// The kinds of spending this part uses (`0x94e1d0` names them).
pub const ADVANCE_TECH: i32 = 1;
pub const BOOST_STABILITY: i32 = 2;
pub const REDUCE_INFLATION: i32 = 15;
pub const PROMOTE_MERCANTILISM: i32 = 26;

/// A country's monarch points, three i32, administrative first.
const POWER: usize = 0x260;

/// `common/scripted_functions`, the game's object that holds them (made on
/// first use by its getters; null before), and the place in it of each this
/// part reads: a vector of conditions, data at `+0x18` and count at `+0x24`,
/// each with a `potential` at `+0x30` and an `allow` at `+0xa8`.
const FUNCTIONS: usize = 0x242bdd8;
pub const CAN_REDUCE_INFLATION: usize = 0x78;
pub const CAN_INCREASE_STABILITY: usize = 0xc8;
const FUNCTION_DATA: usize = 0x18;
const FUNCTION_COUNT: usize = 0x24;
const POTENTIAL: usize = 0x30;
const ALLOW: usize = 0xa8;
/// The most conditions one function is believed to hold.
const MOST: usize = 1000;
/// A date as the game's texts write it: `(&date, out string)`.
const DATE_TEXT: usize = 0x6e5b40;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(COST, "48 8b c4 48 89 58 10 4c 89 48 20 4c 89 40 18 48 89 48 08 55 56 57 41 54 41 55 41 56 41 57", "power spending cost"),
        // The cost switches on the spending's kind.
        Point::keep(0x952711usize, "48 63 49 10 83 f9 29", "power spending cost's kinds").uses(&[KIND]),
        // The check of the spend_power command compares the cost with the
        // country's points of the spending's kind.
        Point::keep(0x94f28cusize, "4c 63 46 14 48 8b 15 39 e6 9e 01 48 8b 8a 18 01 00 00 49 8b 3c 0e 42 39 84 87 60 02 00 00", "power spending's points")
            .uses(&[SKILL, POWER]),
        // The economy view builds a spending as the framework does: its
        // class, the mark, the ID of none.
        Point::keep(
            0xf8ab35usize,
            "48 8d 05 44 a9 cc 00 48 89 85 20 01 00 00 66 0f 6f 05 f5 7b e3 00 f3 0f 7f 85 30 01 00 00 4c 89 ad 40 01 00 00 48 8b 05 47 53 3b 01",
            "stability view's spending",
        )
        .uses(&[SPENDING_CLASS, NO_ID]),
        // The technology view's: the kind 1, its category's kind of power
        // and index as skill and first value.
        Point::keep(0x1009480usize, "41 8b 40 78 c7 45 98 65 01 00 00 48 8d 0d ee bf c4 00 48 89 4d 90 44 89 65 a0 89 45 a4", "technology view's spending")
            .uses(&[SPENDING_CLASS]),
        // A scripted function's verdict walks its conditions: `potential`'s
        // children, then `allow`'s.
        Point::keep(
            0xacd17fusize,
            "4c 63 61 24 4d 85 e4 0f 8e 8c 00 00 00 45 33 f6 90 49 8b 4d 18 4a 8b 04 f1 48 8b 58 38 48 63 40 44",
            "scripted function's conditions",
        )
        .uses(&[FUNCTION_COUNT, FUNCTION_DATA]),
        Point::keep(0xacd1d1usize, "48 8b 98 b0 00 00 00 48 63 80 bc 00 00 00", "scripted function's allow"),
        Point::keep(0x3829c1usize, "48 8b 05 10 94 0a 02 48 8b 88 c8 00 00 00", "can_increase_stability").uses(&[FUNCTIONS, CAN_INCREASE_STABILITY]),
        Point::keep(0x38d689usize, "48 8b 05 48 e7 09 02 48 8b 48 78", "can_reduce_inflation").uses(&[FUNCTIONS, CAN_REDUCE_INFLATION]),
        Point::keep(DATE_TEXT, "48 89 5c 24 18 48 89 74 24 20 48 89 54 24 10 55 57 41 56 48 8d 6c 24 b9", "date text"),
    ]
}

/// A power spending as the game's buttons build one, in the framework's
/// memory: the game reads only its fields.
#[repr(C)]
struct Spending([u8; SPENDING]);

impl Spending {
    fn new(m: &Memory, kind: i32, skill: PowerKind, data: i32) -> GResult<Self> {
        let mut bytes = [0u8; SPENDING];
        let mut put = |at: usize, value: &[u8]| bytes[at..at + value.len()].copy_from_slice(value);
        put(0, &(m.at(SPENDING_CLASS) as u64).to_le_bytes());
        put(8, &OBJECT_MARK.to_le_bytes());
        put(KIND, &kind.to_le_bytes());
        put(SKILL, &(skill as i32).to_le_bytes());
        put(DATA, &data.to_le_bytes());
        put(SECOND, &(-1i32).to_le_bytes());
        put(ABOUT_ID, &m.u64(m.at(NO_ID))?.to_le_bytes());
        Ok(Self(bytes))
    }
}

/// What a spending of `kind` in `skill` points, with `data` its first value,
/// costs `player` now, in whole points, as the panel shows it.
pub fn cost(m: &Memory, player: Tag, kind: i32, skill: PowerKind, data: i32) -> GResult<i32> {
    let mut spending = Spending::new(m, kind, skill, data)?;
    Ok(m.call(m.at(COST), &[spending.0.as_mut_ptr() as usize, player.0 as usize, 0, 0])? as i32)
}

/// The monarch points of `skill` the country at `country` has.
pub fn points_of(m: &Memory, country: usize, skill: PowerKind) -> GResult<i32> {
    Ok(m.i32(country + POWER + 4 * skill as usize)?)
}

/// The spend_power command in its written form, as the game's buttons make
/// it: the country by its tag, the spending's kind as `category`, the kind
/// of power as `skill`, and its first value as `data`, which the game
/// writes only when it is not 0.
pub fn written(country: [u8; 3], kind: i32, skill: PowerKind, data: i32) -> String {
    let data = if data == 0 { String::new() } else { format!("data={data} ") };
    format!("spend_power_command={{ country=\"{}\" power={{ {data}category={kind} skill={} }} }}", String::from_utf8_lossy(&country), skill as i32)
}

/// Spends `player`'s points as the button does: posts the command, which
/// the game checks and carries out at once. `doing` names the action for
/// the refusals.
pub fn spend(player: Tag, kind: i32, skill: PowerKind, data: i32, doing: &str) -> GResult<()> {
    post(&written(player.letters(), kind, skill, data), doing)
}

/// Posts a command of the player's, as a button does; `doing` names the
/// action for the refusals.
pub fn post(text: &str, doing: &str) -> GResult<()> {
    match commands::post(text) {
        Ok(()) => Ok(()),
        Err(Refused::Check) => Err(eu4_harmony_checks::choices::command_refused(doing).into()),
        Err(Refused::NotTaken) => fail(Status::Rejected, "the game takes no orders from the player now"),
        Err(Refused::Unread(how)) => fail(Status::Internal, format!("the game read the command otherwise: {how}")),
        Err(Refused::Game(fail)) => Err(fail),
    }
}

/// The game's text for the localisation key `key`, with `values` put in
/// place of `$NAME$`, as plain Unicode lines: a panel's tooltip line.
pub fn tooltip(m: &Memory, key: &CStr, values: &[(&CStr, String)]) -> GResult<Vec<String>> {
    let values: Vec<(&CStr, &[u8])> = values.iter().map(|(name, text)| (*name, text.as_bytes())).collect();
    let text = game_string::localized_with(m, key.as_ptr() as usize, &values)?;
    Ok(plain_lines(&text))
}

/// Game text as plain Unicode lines, the empty ones left out.
pub fn plain_lines(text: &[u8]) -> Vec<String> {
    game_text::plain(text).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// Whether the scripted function at `function` in the game's
/// `common/scripted_functions` lets `player` do it, and when not, the lines
/// of its conditions that do not hold: for each of its conditions whose
/// `potential` holds, `allow`'s failing lines.
pub fn scripted(m: &Memory, function: usize, player: Tag) -> GResult<(bool, Vec<String>)> {
    let functions = m.ptr(m.at(FUNCTIONS))?;
    if functions == 0 {
        return Ok((true, Vec::new()));
    }
    let entry = m.ptr(functions + function)?;
    if entry == 0 {
        return Ok((true, Vec::new()));
    }
    let count = m.i32(entry + FUNCTION_COUNT)?;
    if !(0..=MOST as i32).contains(&count) {
        return Err(format!("a scripted function with {count} conditions").into());
    }
    let data = m.ptr(entry + FUNCTION_DATA)?;
    let (mut allowed, mut reasons) = (true, Vec::new());
    for i in 0..count as usize {
        let condition = m.ptr(data + i * 8)?;
        if conditions::holds(m, condition + POTENTIAL, player, 0)? && !conditions::holds(m, condition + ALLOW, player, 0)? {
            allowed = false;
            reasons.extend(conditions::failing(m, condition + ALLOW, player, 0)?);
        }
    }
    Ok((allowed, reasons))
}

/// The game's date `date` as its texts write it, in the player language.
pub fn date_text(m: &Memory, date: i32) -> GResult<String> {
    let (_, text) = game_string::written(m, |out| m.call(m.at(DATE_TEXT), &[&date as *const i32 as usize, out]))?;
    Ok(game_text::to_utf8(&text))
}

/// A whole number of points or ducats as the game's tooltips write one.
pub fn whole(value: i32) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use eu4_harmony_remote::choices::PowerKind;

    #[test]
    fn the_command_carries_the_spending_and_leaves_out_a_first_value_of_0() {
        assert_eq!(super::written(*b"FRA", 2, PowerKind::Adm, 0), r#"spend_power_command={ country="FRA" power={ category=2 skill=0 } }"#);
        assert_eq!(super::written(*b"FRA", 1, PowerKind::Mil, 2), r#"spend_power_command={ country="FRA" power={ data=2 category=1 skill=2 } }"#);
    }

    #[test]
    fn a_tooltip_s_lines_lose_their_codes() {
        let text = crate::unicode::game_text::from_str("§RYou can only change it every §Y5§! years.§!\n\n");
        assert_eq!(super::plain_lines(&text), ["You can only change it every 5 years."]);
    }
}
