//! The game's color removal, used when naming a hired mercenary army and
//! elsewhere (docs/game/interface/text-formatting.md). A color marker must
//! be a character of its own: a UTF-8 continuation byte can also be 0xA7.
//! Other formatting, and every surviving byte, stay as they were.
use super::game_text;
use crate::base::called_by_game;
use crate::game_string;
use crate::points::{Function, Point};

const REMOVE: usize = 0x17046a0;
type Remove = unsafe extern "C" fn(*const u8, *mut u8) -> *mut u8;
static ORIGINAL: Function<Remove> = Function::new();

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
unsafe extern "C" fn remove(source: *const u8, out: *mut u8) -> *mut u8 {
    let text = without_colors(game_string::text(source));
    std::ptr::copy_nonoverlapping(game_string::empty().as_ptr().cast::<u8>(), out, game_string::OBJECT);
    game_string::append(out as usize, &text);
    out
}

pub fn points() -> Vec<Point> {
    vec![Point::replace(REMOVE, "48 89 5c 24 08 48 89 6c 24 18 48 89 54 24 10 56 57 41 56", "remove colors from game text", remove as Remove, &ORIGINAL)]
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
}
