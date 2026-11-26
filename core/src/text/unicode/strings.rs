//! UTF-8-aware replacements for the game's single-byte `std::string`
//! transformations:
//!
//! - **Color removal** (`0x17046a0`): skips `§` color markers as characters
//!   rather than raw `0xA7` bytes (which can be UTF-8 continuation bytes).
//! - **Search accent folding** (`0x1704d70`): folds Windows-1252 accented
//!   letters in place while keeping multi-byte UTF-8 sequences intact.
//! - **Save file name transliteration** (`0x1705180`): transliterates
//!   Windows-1252 bytes through the game's ASCII table (`0x1c37af0`) while
//!   leaving UTF-8 sequences intact for PhysFS.
use super::game_text;
use crate::base::called_by_game;
use crate::native::game_string;
use crate::native::points::{Function, Point};
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------- color removal ----------

const REMOVE_COLORS: usize = 0x17046a0;
type Remove = unsafe extern "C" fn(*const u8, *mut u8) -> *mut u8;
static REMOVE_ORIGINAL: Function<Remove> = Function::new();

/// Removes colors, leaving all other game text byte for byte intact.
fn without_colors(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut chars = game_text::chars(text);
    while let Some((c, range)) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.extend_from_slice(&text[range]);
        }
    }
    out
}

#[called_by_game]
unsafe extern "C" fn remove_colors(source: *const u8, out: *mut u8) -> *mut u8 {
    let text = without_colors(game_string::text(source));
    std::ptr::copy_nonoverlapping(game_string::empty().as_ptr().cast::<u8>(), out, game_string::OBJECT);
    game_string::append(out as usize, &text);
    out
}

// ---------- search accent folding ----------

const FOLD: usize = 0x1704d70;
type Fold = unsafe extern "C" fn(*mut u8);
static FOLD_ORIGINAL: Function<Fold> = Function::new();

/// The ASCII letter the game folds a Windows-1252 letter from 0xC0 up to.
fn folded(byte: u8) -> u8 {
    match byte {
        0xc0..=0xc6 => b'A',
        0xc7 => b'C',
        0xc8..=0xcb => b'E',
        0xcc..=0xcf => b'I',
        0xd0 => b'D',
        0xd1 => b'N',
        0xd2..=0xd6 | 0xd8 => b'O',
        0xd9..=0xdc => b'U',
        0xdd => b'Y',
        0xde => b't',
        0xdf => b's',
        0xe0..=0xe6 => b'a',
        0xe7 => b'c',
        0xe8..=0xeb => b'e',
        0xec..=0xef => b'i',
        0xf0 => b'd',
        0xf1 => b'n',
        0xf2..=0xf6 | 0xf8 => b'o',
        0xf9..=0xfc => b'u',
        0xfd | 0xff => b'y',
        0xfe => b't',
        other => other,
    }
}

/// Folds `text` in place, keeping its length.
fn fold(text: &mut [u8]) {
    game_text::map_single_bytes(text, folded);
}

#[called_by_game]
unsafe extern "C" fn fold_string(string: *mut u8) {
    fold(game_string::text_mut(string));
}

// ---------- save name transliteration ----------

const TRANSLITERATE: usize = 0x1705180;
const TABLE: usize = 0x1c37af0;

type Transliterate = unsafe extern "C" fn(*mut u8);

static TRANSLITERATE_ORIGINAL: Function<Transliterate> = Function::new();
static TABLE_AT: AtomicUsize = AtomicUsize::new(0);

/// The ASCII byte the game gives a Windows-1252 byte from 0x80 up.
fn ascii(byte: u8, table: &[u8; 128]) -> u8 {
    match byte {
        0xfe => b't',
        0xde => b'T',
        0xd0 => b'D',
        0xf0 => b'd',
        _ => table[(byte - 0x80) as usize],
    }
}

/// Transliterates `name` in place, keeping its length.
fn transliterate(name: &mut [u8], table: &[u8; 128]) {
    game_text::map_single_bytes(name, |byte| if byte >= 0x80 { ascii(byte, table) } else { byte });
}

#[called_by_game]
unsafe extern "C" fn transliterate_string(string: *mut u8) {
    let table = &*(TABLE_AT.load(Ordering::Acquire) as *const [u8; 128]);
    transliterate(game_string::text_mut(string), table);
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::replace(
            REMOVE_COLORS,
            "48 89 5c 24 08 48 89 6c 24 18 48 89 54 24 10 56 57 41 56",
            "remove colors from game text",
            remove_colors as Remove,
            &REMOVE_ORIGINAL,
        ),
        Point::replace(FOLD, "8b 41 10 85 c0 0f 84 7a 02 00 00 45 33 c0 44 8b", "search accent fold", fold_string as Fold, &FOLD_ORIGINAL),
        Point::replace(
            TRANSLITERATE,
            "8b 41 10 85 c0 0f 84 0c 01 00 00 45 33 c0 4c 8d",
            "save name to ASCII",
            transliterate_string as Transliterate,
            &TRANSLITERATE_ORIGINAL,
        ),
        Point::keep(TABLE, "3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f 3f", "ASCII table for bytes from 0x80").recorded(&TABLE_AT),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mercenary_name_keeps_its_color_marker_continuation_byte() {
        let name = game_text::from_str("大佣兵团");
        assert_eq!(without_colors(&name), name);
        assert_eq!(without_colors(&game_text::from_str("§Y大佣兵团§!")), name);
    }

    #[test]
    fn removes_only_colors_and_preserves_other_game_text_bytes() {
        for text in ["自由佣兵团", "§RGröße · £gold£ ¤ @FRA $X§!", "§Y日本語 한국어 \u{207c0}§!", "é …", ""] {
            let input = game_text::from_str(text);
            let expected = game_text::from_str(&text.replace("§R", "").replace("§Y", "").replace("§!", ""));
            assert_eq!(without_colors(&input), expected);
        }
        assert_eq!(without_colors(b"a\xa7Rb\xa7"), b"ab");
        assert_eq!(without_colors(b"\xa7R\xa7Y\xa7!"), b"");
    }

    #[test]
    fn folds_1252_letters_and_keeps_utf8_whole() {
        let mut text = b"\xc9cole \xe9t\xe9 ".to_vec();
        text.extend_from_slice("中席".as_bytes());
        fold(&mut text);
        let mut expected = b"Ecole ete ".to_vec();
        expected.extend_from_slice("中席".as_bytes());
        assert_eq!(text, expected);
    }

    #[test]
    fn keeps_utf8_and_turns_1252_to_ascii() {
        let mut table = [b'?'; 128];
        table[0xe9 - 0x80] = b'e';
        let mut name = b"Caf\xe9 \xfe".to_vec();
        name.extend_from_slice("中国".as_bytes());
        transliterate(&mut name, &table);
        let mut expected = b"Cafe t".to_vec();
        expected.extend_from_slice("中国".as_bytes());
        assert_eq!(name, expected);
    }
}
