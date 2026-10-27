//! Where text may wrap. The game wraps only after a glyph with no height (a
//! space), so Chinese, which has no spaces, would never wrap. While the game
//! lays out a text, its line break opportunities by the Unicode rules (ICU)
//! are known here, and the layout loop asks after each character.
//!
//! A layout is recognised from the loop itself: the character it is on
//! belongs to another text than the last, or lies before the last one.
use crate::icu;
use std::cell::RefCell;

#[derive(Default)]
struct Layout {
    text: usize,
    len: usize,
    /// Byte offsets a line may break before, ascending.
    breaks: Vec<usize>,
    /// The character the loop is on, as a byte range.
    start: usize,
    end: usize,
    wide: bool,
}

thread_local! {
    static CURRENT: RefCell<Layout> = RefCell::new(Layout::default());
}

fn opportunities(text: &[u8]) -> Vec<usize> {
    // Text that is not UTF-8 (game text with 1252 bytes) is taken as far as it
    // is; a wide character never follows such a byte without UTF-8 around it.
    let valid = match std::str::from_utf8(text) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&text[..e.valid_up_to()]).unwrap_or_default(),
    };
    icu::load().ok().and_then(|icu| icu.line_breaks(valid).ok()).map(|b| b.into_iter().map(|b| b.offset).collect()).unwrap_or_default()
}

/// The layout loop is on the character at `text[start..end]`.
pub fn at(text: &[u8], start: usize, end: usize) {
    CURRENT.with(|c| {
        let mut c = c.borrow_mut();
        if c.text != text.as_ptr() as usize || c.len != text.len() || start < c.start {
            *c = Layout { text: text.as_ptr() as usize, len: text.len(), breaks: opportunities(text), ..Layout::default() };
        }
        c.start = start;
        c.end = end;
        c.wide = text.get(start).is_some_and(|&b| b >= 0x80);
    })
}

/// Whether the character the loop is on is a non-ASCII one after which a line
/// may break. ASCII keeps the game's own rule.
pub fn wide_break_after() -> bool {
    CURRENT.with(|c| {
        let c = c.borrow();
        c.wide && c.breaks.binary_search(&c.end).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_may_break_between_characters_but_not_before_a_comma() {
        let text = "中文，测试".as_bytes();
        at(text, 0, 3);
        assert!(wide_break_after());
        at(text, 3, 6);
        assert!(!wide_break_after());
        at(text, 6, 9);
        assert!(wide_break_after());
    }

    #[test]
    fn ascii_keeps_the_game_rule_and_a_new_text_starts_over() {
        let text = b"ab cd";
        at(text, 2, 3);
        assert!(!wide_break_after());
        let other = "中文".as_bytes();
        at(other, 0, 3);
        assert!(wide_break_after());
    }
}
