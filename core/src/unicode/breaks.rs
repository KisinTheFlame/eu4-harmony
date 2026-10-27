//! Where text may wrap. The game wraps only after a glyph with no height (a
//! space), so Chinese, which has no spaces, would never wrap. While the game
//! lays out a text, its line break opportunities by the Unicode rules (ICU)
//! are known here, and the layout loop asks after each character.
//!
//! A layout is recognised from the loop itself: the character it is on
//! belongs to another text than the last, or lies before the last one.
use super::encoding;
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
    // The game keeps every character Windows-1252 has as its single byte
    // (`encoding`), so Chinese text holds lone bytes for `·`, `“` or `…`. ICU
    // reads the text as UTF-8; each character's offset there maps back.
    let mut utf8 = String::with_capacity(text.len());
    let mut starts = Vec::with_capacity(text.len() + 1);
    let mut at = 0;
    while at < text.len() {
        let (c, len) = encoding::char_at(&text[at..]);
        starts.push((utf8.len(), at));
        utf8.push(c);
        at += len;
    }
    starts.push((utf8.len(), text.len()));
    let Some(breaks) = icu::load().ok().and_then(|icu| icu.line_breaks(&utf8).ok()) else { return Vec::new() };
    breaks.into_iter().filter_map(|b| starts.binary_search_by_key(&b.offset, |&(u, _)| u).ok().map(|i| starts[i].1)).collect()
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

/// Whether a line may break before `text[end]`, when the character before it
/// is non-ASCII; ASCII keeps the game's own rule. For loops that know where a
/// character ends rather than where it starts.
pub fn after(text: &[u8], end: usize) -> bool {
    if end == 0 || end > text.len() {
        return false;
    }
    let mut start = end - 1;
    while start > 0 && end - start < 4 && text[start] & 0xc0 == 0x80 {
        start -= 1;
    }
    let (_, n) = super::scalar::decode(&text[start..]);
    if start + n != end || text[start] < 0x80 {
        return false;
    }
    at(text, start, end);
    wide_break_after()
}

/// [`after`] for a stub: the text and its length, and where the character ends.
pub extern "C" fn after_at(text: *const u8, len: u32, end: u32) -> bool {
    crate::guarded("text wrap", || !text.is_null() && after(unsafe { std::slice::from_raw_parts(text, len as usize) }, end as usize))
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
    fn breaks_are_found_from_where_a_character_ends() {
        let text = "中文，测试 ok".as_bytes();
        assert!(after(text, 3));
        assert!(!after(text, 6));
        assert!(after(text, 9));
        // Inside a character, or after ASCII, the game's rule stays.
        assert!(!after(text, 2));
        assert!(!after(text, text.len()));
    }

    #[test]
    fn breaks_go_on_after_a_windows_1252_byte() {
        // The game keeps ·, “ and ” as single bytes.
        let text = encoding::game_text("匈雅提·亚诺什“谨慎者”Yuanzhang。");
        let end_of = |part: &str| {
            let part = encoding::game_text(part);
            text.windows(part.len()).position(|w| w == part).unwrap() + part.len()
        };
        assert!(after(&text, end_of("亚")));
        assert!(after(&text, end_of("谨")));
        // Never before the closing quote or the full stop.
        assert!(!after(&text, end_of("慎者")));
        assert!(!after(&text, end_of("Yuanzhang") - 1));
        assert!(after(&text, text.len()));
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
