//! Input: mouse and keyboard input at screen positions, as the player gives
//! it with a real mouse and keyboard (ADR 0037). Remote control only.
//!
//! A screen position is `x`, `y` in pixels of the frame the game draws, from
//! its top left corner, as `screenshot` takes it; with `scale`, a position
//! in a screenshot taken at that scale. A key is named by its place on the
//! keyboard, as a US layout names it ([`KEYS`]); a combination joins keys
//! with `+` (`ctrl+s`).
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

group! {
    /// Mouse and keyboard input at screen positions, as the player gives it.
    "input": InputRequest, InputAnswer;

    /// Clicks at a screen position: the cursor moved there, then the button
    /// pressed and let go, twice for a double click. For what has no
    /// capability of its own; where one exists, use it, since its answer
    /// says what the game did and a click's does not. Answered once the
    /// input is taken, before the game has it: inputs reach the game one
    /// after another over the next frames, and a request sent after this one
    /// runs once the frame that took it has run, so a screenshot then shows
    /// it. How long to wait for what it does is the client's.
    remote_anywhere Click "click" {
        /// Pixels from the frame's left edge.
        x: u32,
        /// Pixels from the frame's top edge.
        y: u32,
        /// left, right or middle; absent for left.
        button: Option<Button>,
        /// 1 to 3 clicks, 2 for a double click; absent for 1.
        count: Option<u32>,
        /// Keys held down during the click, such as shift or ctrl+shift.
        modifiers: Option<Keys>,
        /// The position is one in a screenshot taken at this scale, 1 to
        /// 100 percent of the frame; absent for 100.
        scale: Option<u32>,
    } -> (), "null once the input is taken";

    /// Moves the cursor to a screen position, for a tooltip or a hover. The
    /// cursor stays there until the next input or until the player's own
    /// mouse moves over the game; left at an edge of the frame, it scrolls
    /// the map as the player's would.
    remote_anywhere Move "move" {
        /// Pixels from the frame's left edge.
        x: u32,
        /// Pixels from the frame's top edge.
        y: u32,
        /// Keys held down during the move.
        modifiers: Option<Keys>,
        /// The scale of the screenshot the position is in; absent for 100.
        scale: Option<u32>,
    } -> (), "null once the input is taken";

    /// Drags from one screen position to another: the button pressed at the
    /// first, the cursor moved to the second over a few frames, the button
    /// let go there. For drawing a box round armies or moving a slider.
    remote_anywhere Drag "drag" {
        /// Where the drag starts: pixels from the frame's left edge.
        from_x: u32,
        /// Where the drag starts: pixels from the frame's top edge.
        from_y: u32,
        /// Where it ends: pixels from the frame's left edge.
        to_x: u32,
        /// Where it ends: pixels from the frame's top edge.
        to_y: u32,
        /// left, right or middle; absent for left.
        button: Option<Button>,
        /// Keys held down during the drag.
        modifiers: Option<Keys>,
        /// The scale of the screenshot the positions are in; absent for 100.
        scale: Option<u32>,
    } -> (), "null once the input is taken";

    /// Turns the mouse wheel at a screen position: the cursor moved there,
    /// then the wheel turned, a notch a frame. Up zooms the map in.
    remote_anywhere Scroll "scroll" {
        /// Pixels from the frame's left edge.
        x: u32,
        /// Pixels from the frame's top edge.
        y: u32,
        /// up, down, left or right.
        direction: Direction,
        /// Notches, 1 to 100; absent for 1.
        amount: Option<u32>,
        /// Keys held down while the wheel turns.
        modifiers: Option<Keys>,
        /// The scale of the screenshot the position is in; absent for 100.
        scale: Option<u32>,
    } -> (), "null once the input is taken";

    /// Presses a key or a combination, such as f1, escape, space or ctrl+s:
    /// the keys pressed in order and let go in reverse. A key that types a
    /// character types the US layout's, unless ctrl, alt or win is held; to
    /// type text, use input.type. Keys are named by their place on the
    /// keyboard as a US layout names them: letters, digits, escape, enter,
    /// space, tab, backspace, delete, insert, home, end, page_up, page_down,
    /// left, right, up, down, f1 to f12, minus, equals, grave (the key left
    /// of 1), comma, period, slash, semicolon, apostrophe, left_bracket,
    /// right_bracket, backslash, numpad_plus, numpad_minus, and ctrl, shift,
    /// alt and win; plus is shift and equals, as a keyboard types +, which
    /// speeds the game up.
    /// Other tools' names for them are taken too, such as control, super,
    /// return, esc, pageup and arrowleft, in any case.
    remote_anywhere Key "key" {
        /// The key, or keys joined with +.
        keys: Keys,
        /// How many times, 1 to 100; absent for 1.
        repeat: Option<u32>,
    } -> (), "null once the input is taken";

    /// Types text, any Unicode, as an input method gives the game text it
    /// composed: Chinese and every other script, without an input method.
    /// It goes where typed text goes, such as the edit box that has the
    /// keyboard; a character a frame.
    remote_anywhere Type "type" {
        /// The text.
        text: String,
    } -> (), "null once the input is taken";

    /// Presses a mouse button at a screen position and holds it: it stays
    /// down until input.mouse_up, or until a session begins.
    remote_anywhere MouseDown "mouse_down" {
        /// Pixels from the frame's left edge.
        x: u32,
        /// Pixels from the frame's top edge.
        y: u32,
        /// left, right or middle; absent for left.
        button: Option<Button>,
        /// The scale of the screenshot the position is in; absent for 100.
        scale: Option<u32>,
    } -> (), "null once the input is taken";

    /// Lets a mouse button go at a screen position.
    remote_anywhere MouseUp "mouse_up" {
        /// Pixels from the frame's left edge.
        x: u32,
        /// Pixels from the frame's top edge.
        y: u32,
        /// left, right or middle; absent for left.
        button: Option<Button>,
        /// The scale of the screenshot the position is in; absent for 100.
        scale: Option<u32>,
    } -> (), "null once the input is taken";

    /// Presses a key and holds it: it stays down until input.key_up, or
    /// until a session begins. It is pressed once; the game gets no repeats
    /// as from a real key held down.
    remote_anywhere KeyDown "key_down" {
        /// The key.
        key: Keys,
    } -> (), "null once the input is taken";

    /// Lets a key go.
    remote_anywhere KeyUp "key_up" {
        /// The key.
        key: Keys,
    } -> (), "null once the input is taken";

    /// Where the game takes the cursor to be, and the buttons and keys
    /// remote control holds down.
    remote_anywhere Get "get" {} -> Cursor, "x and y, the screen position of the cursor (null when it is off the frame); buttons and keys, those remote control holds down";
}

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Which way the wheel turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Where the cursor is, and what remote control holds down.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct Cursor {
    /// The cursor's screen position; null when it is off the frame.
    pub x: Option<u32>,
    pub y: Option<u32>,
    /// The buttons remote control holds down.
    pub buttons: Vec<Button>,
    /// The keys remote control holds down, by name.
    pub keys: Vec<String>,
}

/// A key: its place on the keyboard, and what a US layout makes of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    /// Its name, as answers write it.
    pub name: &'static str,
    /// Other names a client may write it by, in lower case without `_`.
    aliases: &'static [&'static str],
    /// The scan code (set 1) the keyboard sends for it, and whether it is
    /// one of the extended keys (sent after `E0`).
    pub scan: u8,
    pub extended: bool,
    /// The Windows virtual key a US layout gives it.
    pub virtual_key: u8,
    /// The characters it types on a US layout, without and with shift.
    pub text: Option<(char, char)>,
}

impl Key {
    /// Whether it is one of the modifiers: ctrl, shift, alt or win.
    pub fn modifier(&self) -> bool {
        matches!(self.name, "ctrl" | "shift" | "alt" | "win")
    }

    /// The key named `name`, by its name or another tool's, in any case.
    pub fn named(name: &str) -> Option<Key> {
        let lower = name.to_lowercase();
        let squeezed: String = lower.chars().filter(|c| !matches!(c, '_' | ' ')).collect();
        let found = |n: &str| KEYS.iter().find(|k| k.name == n || k.name.replace('_', "") == n || k.aliases.contains(&n)).copied();
        found(&lower).or_else(|| found(&squeezed))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

const fn key(name: &'static str, aliases: &'static [&'static str], scan: u8, virtual_key: u8, text: Option<(char, char)>) -> Key {
    Key { name, aliases, scan, extended: false, virtual_key, text }
}

const fn extended(name: &'static str, aliases: &'static [&'static str], scan: u8, virtual_key: u8) -> Key {
    Key { name, aliases, scan, extended: true, virtual_key, text: None }
}

const fn letter(c: char, scan: u8) -> Key {
    // The names are the letters themselves, kept in a table of their own.
    Key { name: LETTER_NAMES[c as usize - 'a' as usize], aliases: &[], scan, extended: false, virtual_key: c.to_ascii_uppercase() as u8, text: Some((c, c.to_ascii_uppercase())) }
}

const LETTER_NAMES: [&str; 26] = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z"];

/// Every key input names, by its place on a US keyboard.
pub const KEYS: &[Key] = &[
    letter('a', 0x1e),
    letter('b', 0x30),
    letter('c', 0x2e),
    letter('d', 0x20),
    letter('e', 0x12),
    letter('f', 0x21),
    letter('g', 0x22),
    letter('h', 0x23),
    letter('i', 0x17),
    letter('j', 0x24),
    letter('k', 0x25),
    letter('l', 0x26),
    letter('m', 0x32),
    letter('n', 0x31),
    letter('o', 0x18),
    letter('p', 0x19),
    letter('q', 0x10),
    letter('r', 0x13),
    letter('s', 0x1f),
    letter('t', 0x14),
    letter('u', 0x16),
    letter('v', 0x2f),
    letter('w', 0x11),
    letter('x', 0x2d),
    letter('y', 0x15),
    letter('z', 0x2c),
    key("1", &[], 0x02, b'1', Some(('1', '!'))),
    key("2", &[], 0x03, b'2', Some(('2', '@'))),
    key("3", &[], 0x04, b'3', Some(('3', '#'))),
    key("4", &[], 0x05, b'4', Some(('4', '$'))),
    key("5", &[], 0x06, b'5', Some(('5', '%'))),
    key("6", &[], 0x07, b'6', Some(('6', '^'))),
    key("7", &[], 0x08, b'7', Some(('7', '&'))),
    key("8", &[], 0x09, b'8', Some(('8', '*'))),
    key("9", &[], 0x0a, b'9', Some(('9', '('))),
    key("0", &[], 0x0b, b'0', Some(('0', ')'))),
    key("escape", &["esc"], 0x01, 0x1b, None),
    key("enter", &["return"], 0x1c, 0x0d, None),
    key("space", &["spacebar", " "], 0x39, 0x20, Some((' ', ' '))),
    key("tab", &[], 0x0f, 0x09, None),
    key("backspace", &["back"], 0x0e, 0x08, None),
    key("minus", &["-", "hyphen"], 0x0c, 0xbd, Some(('-', '_'))),
    key("equals", &["=", "equal"], 0x0d, 0xbb, Some(('=', '+'))),
    key("grave", &["`", "backquote", "backtick", "tilde", "~"], 0x29, 0xc0, Some(('`', '~'))),
    key("left_bracket", &["[", "bracketleft"], 0x1a, 0xdb, Some(('[', '{'))),
    key("right_bracket", &["]", "bracketright"], 0x1b, 0xdd, Some((']', '}'))),
    key("backslash", &["\\"], 0x2b, 0xdc, Some(('\\', '|'))),
    key("semicolon", &[";"], 0x27, 0xba, Some((';', ':'))),
    key("apostrophe", &["'", "quote"], 0x28, 0xde, Some(('\'', '"'))),
    key("comma", &[","], 0x33, 0xbc, Some((',', '<'))),
    key("period", &[".", "dot"], 0x34, 0xbe, Some(('.', '>'))),
    key("slash", &["/"], 0x35, 0xbf, Some(('/', '?'))),
    extended("insert", &["ins"], 0x52, 0x2d),
    extended("delete", &["del"], 0x53, 0x2e),
    extended("home", &[], 0x47, 0x24),
    extended("end", &[], 0x4f, 0x23),
    extended("page_up", &["pgup", "prior"], 0x49, 0x21),
    extended("page_down", &["pgdn", "next"], 0x51, 0x22),
    extended("left", &["arrowleft"], 0x4b, 0x25),
    extended("right", &["arrowright"], 0x4d, 0x27),
    extended("up", &["arrowup"], 0x48, 0x26),
    extended("down", &["arrowdown"], 0x50, 0x28),
    key("numpad_plus", &["kpplus", "kpadd", "add"], 0x4e, 0x6b, Some(('+', '+'))),
    key("numpad_minus", &["kpminus", "kpsubtract", "subtract"], 0x4a, 0x6d, Some(('-', '-'))),
    key("f1", &[], 0x3b, 0x70, None),
    key("f2", &[], 0x3c, 0x71, None),
    key("f3", &[], 0x3d, 0x72, None),
    key("f4", &[], 0x3e, 0x73, None),
    key("f5", &[], 0x3f, 0x74, None),
    key("f6", &[], 0x40, 0x75, None),
    key("f7", &[], 0x41, 0x76, None),
    key("f8", &[], 0x42, 0x77, None),
    key("f9", &[], 0x43, 0x78, None),
    key("f10", &[], 0x44, 0x79, None),
    key("f11", &[], 0x57, 0x7a, None),
    key("f12", &[], 0x58, 0x7b, None),
    key("ctrl", &["control", "controll", "ctrll", "lctrl"], 0x1d, 0x11, None),
    key("shift", &["shiftl", "lshift"], 0x2a, 0x10, None),
    key("alt", &["altl", "lalt", "option"], 0x38, 0x12, None),
    extended("win", &["super", "superl", "meta", "metal", "cmd", "command", "windows", "os"], 0x5b, 0x5b),
];

/// A key, or keys pressed together, written as their names joined with `+`
/// (`ctrl+s`). `plus`, or `+` (`ctrl++`), is shift and equals, as a keyboard
/// types +.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys(pub Vec<Key>);

impl Keys {
    /// The keys `text` names; why not, when a name is no key's.
    pub fn parse(text: &str) -> Result<Keys, String> {
        let text = text.trim();
        // A + after a + (or alone) is the plus key itself.
        let (rest, plus) = match text {
            "+" => ("", true),
            _ if text.ends_with("++") => (&text[..text.len() - 2], true),
            _ => (text, false),
        };
        let mut names: Vec<&str> = if rest.is_empty() { Vec::new() } else { rest.split('+').collect() };
        if plus {
            names.push("plus");
        }
        let mut keys = Vec::new();
        for name in names {
            let found = match name.trim().to_lowercase().as_str() {
                "plus" => vec![Key::named("shift"), Key::named("equals")].into_iter().collect::<Option<Vec<_>>>(),
                _ => Key::named(name).map(|k| vec![k]),
            };
            for key in found.ok_or_else(|| format!("{name:?} is no key; eu4-harmony wiki input key lists them"))? {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        if keys.is_empty() {
            return Err("no key named".into());
        }
        Ok(Keys(keys))
    }
}

impl fmt::Display for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.0.iter().map(|k| k.name).collect();
        f.write_str(&names.join("+"))
    }
}

impl Serialize for Keys {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Keys::parse(&text).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<&'static str> {
        Keys::parse(text).unwrap().0.iter().map(|k| k.name).collect()
    }

    #[test]
    fn keys_are_named_as_any_tool_names_them() {
        assert_eq!(names("ctrl+s"), ["ctrl", "s"]);
        assert_eq!(names("CTRL+S"), ["ctrl", "s"]);
        assert_eq!(names("Control+Shift+Page_Down"), ["ctrl", "shift", "page_down"]);
        assert_eq!(names("Return"), ["enter"]);
        assert_eq!(names("ESC"), ["escape"]);
        assert_eq!(names("ARROWLEFT"), ["left"]);
        assert_eq!(names("super+e"), ["win", "e"]);
        assert_eq!(names("pageup"), ["page_up"]);
        assert_eq!(names("`"), ["grave"]);
        assert_eq!(names("ctrl++"), ["ctrl", "shift", "equals"]);
        assert_eq!(names("+"), ["shift", "equals"]);
        assert_eq!(names("plus"), ["shift", "equals"]);
        assert_eq!(names("shift+plus"), ["shift", "equals"]);
        assert_eq!(names("="), ["equals"]);
        assert_eq!(names("kp_add"), ["numpad_plus"]);
        assert_eq!(names("-"), ["minus"]);
        assert_eq!(names("F12"), ["f12"]);
        assert_eq!(names("7"), ["7"]);
    }

    #[test]
    fn a_name_that_is_no_key_is_refused() {
        assert_eq!(Keys::parse("ctrl+banana").unwrap_err(), "\"banana\" is no key; eu4-harmony wiki input key lists them");
        assert!(Keys::parse("").is_err());
        assert!(Keys::parse("ctrl+").is_err());
    }

    #[test]
    fn keys_write_their_own_names() {
        assert_eq!(serde_json::to_value(Keys::parse("Control+Return").unwrap()).unwrap(), "ctrl+enter");
        let keys: Keys = serde_json::from_value(serde_json::json!("shift+a")).unwrap();
        assert_eq!(keys.0[1].text, Some(('a', 'A')));
    }

    #[test]
    fn every_key_has_its_own_name_and_place() {
        for (i, a) in KEYS.iter().enumerate() {
            for b in &KEYS[i + 1..] {
                assert_ne!(a.name, b.name);
                assert!((a.scan, a.extended) != (b.scan, b.extended), "{} and {} share a place", a.name, b.name);
                assert!(!b.aliases.contains(&a.name) && !a.aliases.contains(&b.name));
            }
            assert_eq!(Key::named(a.name), Some(*a));
        }
    }
}
