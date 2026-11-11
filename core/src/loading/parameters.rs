//! A parameterized scripted trigger's or effect's use given its arguments in
//! one pass (docs/research/parameterized-script.md, docs/plans/game-speed.md,
//! S12).
//!
//! `0xdb0860` makes the text of one use of a definition with parameters:
//! the definition's text, its blocks resolved (`0xdb1410`), then each
//! `$name$` given the use's value by `0xdb1800(definition, text, arguments,
//! parameters)`. That puts the values in from the last parameter to the
//! first, and for each one builds the whole text again: the part before the
//! parameter, its value and the part after, each copied and cut at a NUL,
//! about five copies of the text for every parameter. A start makes about
//! 320,000 uses, 1.2 s of the LoadingThread in this function.
//!
//! The framework puts every value in with one pass from the left instead,
//! when the parameters are in order and apart, which their finder
//! (`0xdb0d60`) makes them, and the text has no NUL: then each step of the
//! game's leaves the text before its parameter as it was, and the text it
//! ends with is the original's pieces between the parameters with each
//! parameter's value in its place, which the pass makes. A value is the
//! first argument of the parameter's name, cut at a NUL, or nothing. When
//! the parameters or the text are otherwise, the game's function runs as it
//! is. It is the speed-up `script-parameters` (`crate::speed`): checked, the
//! game's function makes the text too and the two are compared; left to
//! the game, nothing is installed.
use crate::called_by_game;
use crate::game_string;
use crate::points::{Function, Point};
use crate::speed::{self, SpeedUp};
use std::cell::RefCell;

/// `0xdb1800(definition, text, arguments, parameters)`.
const SUBSTITUTE: usize = 0xdb1800;
/// A list of the game's kind: its items and their count (i32).
const ITEMS: usize = 0x08;
const COUNT: usize = 0x14;
/// A parameter found in the text: its name (a `std::string`), where its
/// first `$` is and one past its second (i32 each).
const PARAMETER: usize = 0x28;
const START: usize = 0x20;
const END: usize = 0x24;
/// An argument of the use: its name and its value (`std::string`s).
const ARGUMENT: usize = 0x40;
const VALUE: usize = 0x20;

type Substitute = unsafe extern "C" fn(usize, usize, usize, usize);
static ORIGINAL: Function<Substitute> = Function::new();

/// A parameter: its name and the two places.
#[derive(Clone, Copy, Debug)]
struct Parameter<'a> {
    name: &'a [u8],
    start: i32,
    end: i32,
}

/// The text up to its first NUL, as the game's copies of a C string cut it.
fn to_nul(text: &[u8]) -> &[u8] {
    text.iter().position(|&b| b == 0).map_or(text, |nul| &text[..nul])
}

/// The value of the parameter `name`: the first argument of that name, up
/// to its first NUL; nothing when no argument has it.
fn value<'a>(arguments: &[(&'a [u8], &'a [u8])], name: &[u8]) -> &'a [u8] {
    arguments.iter().find(|(n, _)| *n == name).map_or(&[], |(_, v)| to_nul(v))
}

/// Writes into `out` the text the game's substitution gives, in one pass;
/// false, leaving `out` as it may be, when the parameters are not in order
/// and apart within the text or the text has a NUL, where the game's steps
/// do not come to that.
fn substitute(text: &[u8], parameters: &[Parameter], arguments: &[(&[u8], &[u8])], out: &mut Vec<u8>) -> bool {
    if text.contains(&0) {
        return false;
    }
    out.clear();
    let mut at = 0usize;
    for p in parameters {
        let (Ok(start), Ok(end)) = (usize::try_from(p.start), usize::try_from(p.end)) else { return false };
        if start < at || end < start || end > text.len() {
            return false;
        }
        out.extend_from_slice(&text[at..start]);
        out.extend_from_slice(value(arguments, p.name));
        at = end;
    }
    out.extend_from_slice(&text[at..]);
    true
}

thread_local! {
    /// The text being made, kept between uses for its room.
    static MADE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// The parameters and arguments of a use, read from the game's lists.
///
/// # Safety
/// `list` is a live list of the game's kind of `size`-byte items.
unsafe fn items(list: usize, size: usize) -> impl Iterator<Item = usize> {
    let (items, count) = unsafe { (*((list + ITEMS) as *const usize), *((list + COUNT) as *const i32)) };
    (0..count.max(0) as usize).map(move |i| items + i * size)
}

/// Gives the use's text its values in one pass: 0 when done, or the game's
/// function to run instead.
#[called_by_game]
extern "C" fn substituted(definition: usize, text: usize, arguments: usize, parameters: usize) -> usize {
    let original = ORIGINAL.address();
    let checked = speed::checked(&SPEED_UP);
    MADE.with(|made| {
        let mut made = made.borrow_mut();
        let made = &mut *made;
        unsafe {
            let given: Vec<(&[u8], &[u8])> =
                items(arguments, ARGUMENT).map(|a| (game_string::text(a as *const u8), game_string::text((a + VALUE) as *const u8))).collect();
            let found: Vec<Parameter> = items(parameters, PARAMETER)
                .map(|p| Parameter { name: game_string::text(p as *const u8), start: *((p + START) as *const i32), end: *((p + END) as *const i32) })
                .collect();
            if !substitute(game_string::text(text as *const u8), &found, &given, made) {
                return original;
            }
            if checked {
                // The game's own text is the one kept.
                ORIGINAL.get()(definition, text, arguments, parameters);
                let game = game_string::text(text as *const u8);
                speed::compared(&SPEED_UP, game == made.as_slice(), || {
                    format!("{} parameters: the pass made {:?}, the game {:?}", found.len(), String::from_utf8_lossy(made), String::from_utf8_lossy(game))
                });
                return 0;
            }
            game_string::truncate(text as *mut u8, 0);
            game_string::append(text, made);
        }
        0
    })
}

/// Replaces `0xdb1800`: its arguments go to [`substituted`] as they came,
/// and the game's function runs when that answers it.
#[unsafe(naked)]
unsafe extern "C" fn substitute_stub(_: usize, _: usize, _: usize, _: usize) {
    std::arch::naked_asm!(
        crate::call_rust!(call substituted, "mov r11, [rsp + 0x48]", "test r11, r11"),
        "jz 2f",
        "jmp r11",
        "2:",
        "ret",
        substituted = sym substituted,
    );
}

/// Checked, the game's function makes each use's text too, which is kept,
/// and the two are compared byte for byte.
pub const SPEED_UP: SpeedUp = SpeedUp { name: "script-parameters", group: "loading", points, checkable: true };

fn points() -> Vec<Point> {
    use game_string::{CAPACITY, SIZE};
    vec![
        Point::replace(
            SUBSTITUTE,
            "4c 89 4c 24 20 4c 89 44 24 18 48 89 54 24 10 48 89 4c 24 08 55 53 56 57 41 54 41 55 41 56 41 57",
            "script parameters' substitution",
            substitute_stub as Substitute,
            &ORIGINAL,
        ),
        // From the last parameter to the first, 0x28 bytes each.
        Point::keep(0xdb1844usize, "41 8b 41 14 83 e8 01 0f 88 b0 03 00 00 4c 63 e0", "script parameters counted").uses(&[COUNT]),
        Point::keep(0xdb1858usize, "4f 8d 2c a4 49 c1 e5 03", "script parameter's size"),
        Point::keep(0xdb1870usize, "49 8b 71 08 49 03 f5", "script parameters' items").uses(&[ITEMS]),
        // Each argument, 0x40 bytes, compared by its name's size and bytes;
        // the first that matches gives its value, cut at a NUL.
        Point::keep(0xdb188fusize, "49 63 40 14 85 c0 0f 8e 91 00 00 00 4d 8b 68 08 4c 8b 7e 18 4c 8b f0 4c 8b 66 10 49 8b dd", "script arguments walked")
            .uses(&[COUNT, ITEMS, SIZE, CAPACITY]),
        Point::keep(0xdb18bcusize, "4c 8b 43 10", "script argument's name size").uses(&[SIZE]),
        Point::keep(0xdb18d2usize, "e8 79 3d c8 00 85 c0", "script argument's name compared"),
        Point::keep(0xdb18deusize, "48 83 c3 40", "script argument's size"),
        Point::keep(0xdb18e9usize, "48 c1 e7 06 49 8d 55 20 48 03 d7 48 83 7a 18 10 72 03 48 8b 12", "script argument's value").uses(&[VALUE, CAPACITY]),
        // The text before the parameter, to its start, and after it, from
        // its end.
        Point::keep(0xdb1948usize, "39 4e 20 0f 4e 4e 20", "script parameter's start").uses(&[START]),
        Point::keep(0xdb1a52usize, "8b 4e 24", "script parameter's end").uses(&[END]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's substitution as `0xdb1800` makes it: from the last
    /// parameter to the first, the text before it to its start (clamped as
    /// the game clamps it), its value and the text from its end, each cut
    /// at a NUL; `None` where the game throws (an end past the text).
    fn game(text: &[u8], parameters: &[Parameter], arguments: &[(&[u8], &[u8])]) -> Option<Vec<u8>> {
        let mut text = text.to_vec();
        for p in parameters.iter().rev() {
            let length = text.len();
            let start = if p.start <= length as i32 { p.start } else { length as i32 };
            let before = length.min(start as u32 as usize);
            let end = p.end.max(0) as usize;
            if end > length {
                return None;
            }
            let mut made = to_nul(&text[..before]).to_vec();
            made.extend_from_slice(value(arguments, p.name));
            made.extend_from_slice(to_nul(&text[end..]));
            text = to_nul(&made).to_vec();
        }
        Some(text)
    }

    fn p(name: &str, start: i32, end: i32) -> Parameter<'_> {
        Parameter { name: name.as_bytes(), start, end }
    }

    /// The pass's text where it makes one, which must be the game's.
    fn pass(text: &str, parameters: &[Parameter], arguments: &[(&[u8], &[u8])]) -> Option<Vec<u8>> {
        let mut out = vec![b'x'; 3];
        let made = substitute(text.as_bytes(), parameters, arguments, &mut out).then_some(out);
        if let Some(made) = &made {
            assert_eq!(Some(made.clone()), game(text.as_bytes(), parameters, arguments), "{text:?} {parameters:?}");
        }
        made
    }

    /// The places of each `$name$` in `text`, as the finder gives them.
    fn found(text: &str) -> Vec<Parameter<'_>> {
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(first) = text[from..].find('$').map(|i| from + i) {
            let Some(second) = text[first + 1..].find('$').map(|i| first + 1 + i) else { break };
            found.push(Parameter { name: &text.as_bytes()[first + 1..second], start: first as i32, end: second as i32 + 1 });
            from = second + 1;
        }
        found
    }

    #[test]
    fn values_go_where_their_parameters_stand() {
        let text = "add_prestige = $amount$ if = { limit = { tag = $who$ } add_prestige = $amount$ }";
        let arguments: [(&[u8], &[u8]); 2] = [(b"who", b"FRA"), (b"amount", b"10")];
        assert_eq!(pass(text, &found(text), &arguments).unwrap(), b"add_prestige = 10 if = { limit = { tag = FRA } add_prestige = 10 }");
    }

    #[test]
    fn a_name_given_twice_takes_the_first_and_one_not_given_nothing() {
        let text = "a = $x$ b = $y$ c";
        let arguments: [(&[u8], &[u8]); 2] = [(b"x", b"1"), (b"x", b"2")];
        assert_eq!(pass(text, &found(text), &arguments).unwrap(), b"a = 1 b =  c");
        // A value is cut at its NUL, as the game's copy of it is.
        let cut: [(&[u8], &[u8]); 2] = [(b"x", b"1\x002"), (b"y", b"")];
        assert_eq!(pass(text, &found(text), &cut).unwrap(), b"a = 1 b =  c");
        // Names differing in length or bytes are other names.
        let others: [(&[u8], &[u8]); 2] = [(b"xx", b"1"), (b"X", b"2")];
        assert_eq!(pass(text, &found(text), &others).unwrap(), b"a =  b =  c");
    }

    #[test]
    fn edges_and_unpaired_signs_are_the_game_s() {
        for text in ["$x$", "$x$$y$", "a $x$", "$x$ b", "a $ b", "$x$ $", "", "$$", "no parameters"] {
            let arguments: [(&[u8], &[u8]); 2] = [(b"x", b"long value"), (b"", b"empty name")];
            assert!(pass(text, &found(text), &arguments).is_some(), "{text:?}");
        }
    }

    #[test]
    fn what_the_steps_would_not_come_to_is_left_to_the_game() {
        let arguments: [(&[u8], &[u8]); 1] = [(b"x", b"1")];
        // Out of order, overlapping, backwards, below 0, past the end.
        assert_eq!(pass("a $x$ b $x$", &[p("x", 6, 9), p("x", 2, 5)], &arguments), None);
        assert_eq!(pass("a $x$ b", &[p("x", 2, 5), p("x", 4, 6)], &arguments), None);
        assert_eq!(pass("a $x$ b", &[p("x", 5, 2)], &arguments), None);
        assert_eq!(pass("a $x$ b", &[p("x", -1, 2)], &arguments), None);
        assert_eq!(pass("a $x$ b", &[p("x", 2, 99)], &arguments), None);
        // A text with a NUL, which the game's copies cut.
        assert_eq!(pass("a $x$\0b", &[p("x", 2, 5)], &arguments), None);
        // Touching parameters, and one of nothing, are in order and apart.
        assert!(pass("$x$$x$", &[p("x", 0, 3), p("x", 3, 6)], &arguments).is_some());
        assert!(pass("ab", &[p("x", 1, 1)], &arguments).is_some());
    }

    /// Texts and places made at random, against the game's steps.
    #[test]
    fn the_pass_makes_the_game_s_text_wherever_it_makes_one() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let names: [&[u8]; 3] = [b"a", b"bb", b"c"];
        let mut made = 0;
        for _ in 0..20_000 {
            let length = next(40) as usize;
            let mut text: Vec<u8> = (0..length).map(|_| b"ab$ [x"[next(6) as usize]).collect();
            if length > 0 && next(10) == 0 {
                text[next(length as u64) as usize] = 0;
            }
            let count = next(5) as usize;
            let mut places: Vec<i32> = (0..count * 2).map(|_| next(length as u64 + 3) as i32 - 1).collect();
            if next(2) == 0 {
                places.sort_unstable();
            }
            let parameters: Vec<Parameter> =
                (0..count).map(|i| Parameter { name: names[next(3) as usize], start: places[2 * i], end: places[2 * i + 1] }).collect();
            let arguments: Vec<(&[u8], &[u8])> =
                (0..next(4)).map(|_| (names[next(3) as usize], [b"" as &[u8], b"v", b"long\0cut"][next(3) as usize])).collect();
            let mut out = Vec::new();
            if substitute(&text, &parameters, &arguments, &mut out) {
                assert_eq!(Some(out), game(&text, &parameters, &arguments), "{text:?} {parameters:?} {arguments:?}");
                made += 1;
            }
        }
        assert!(made > 2_000, "only {made} made");
    }
}
