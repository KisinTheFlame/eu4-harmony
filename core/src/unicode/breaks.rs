//! Where text may wrap. The game's layout loops wrap only after a glyph with
//! no height (a space), so Chinese, which has no spaces, would never wrap;
//! they ask here after each character instead, and a line may also break
//! wherever the Unicode rules (ICU) allow after a non-ASCII character.
//!
//! A loop asks once per character, so the breaks of the text it walks are
//! kept between questions. They are worked out anew whenever the text is not
//! the one they were worked out for: another address or length, or other
//! bytes where the loop now is, that is the character asked about and the
//! one after it, on which a break there depends (the game reuses buffers).
use super::{costs, game_text};
use crate::icu;
use std::cell::RefCell;

/// The breaks of the last text asked about.
#[derive(Default)]
struct Known {
    text: usize,
    bytes: Vec<u8>,
    /// Byte offsets a line may break before, ascending.
    breaks: Vec<usize>,
}

impl Known {
    /// The breaks of `text`, asked about at `text[start..end]`.
    fn breaks(&mut self, text: &[u8], start: usize, end: usize) -> &[usize] {
        costs::BREAK_QUESTIONS.add();
        let near = start..(end + 4).min(text.len());
        let same = self.text == text.as_ptr() as usize && self.bytes.len() == text.len() && self.bytes.get(near.clone()) == text.get(near);
        if !same {
            *self = Known { text: text.as_ptr() as usize, bytes: text.to_vec(), breaks: costs::BREAKS.time(|| opportunities(text)) };
        }
        &self.breaks
    }
}

thread_local! {
    static KNOWN: RefCell<Known> = RefCell::new(Known::default());
}

/// Where lines of `text` may break, by byte offset, ascending. The text is
/// handed to ICU as UTF-8 and each break maps back to the game's bytes.
fn opportunities(text: &[u8]) -> Vec<usize> {
    let mut utf8 = String::with_capacity(text.len());
    let mut starts = Vec::with_capacity(text.len() + 1);
    for (c, range) in game_text::chars(text) {
        starts.push((utf8.len(), range.start));
        utf8.push(c);
    }
    starts.push((utf8.len(), text.len()));
    let Some(breaks) = icu::load().ok().and_then(|icu| icu.line_breaks(&utf8).ok()) else { return Vec::new() };
    breaks.into_iter().filter_map(|b| starts.binary_search_by_key(&b.offset, |&(u, _)| u).ok().map(|i| starts[i].1)).collect()
}

/// Whether a line may break before `text[end]`, when the character before it
/// is non-ASCII; ASCII keeps the game's own rule.
pub fn after(text: &[u8], end: usize) -> bool {
    match game_text::char_before(text, end) {
        Some((c, start)) if !c.is_ascii() => KNOWN.with(|k| k.borrow_mut().breaks(text, start, end).binary_search(&end).is_ok()),
        _ => false,
    }
}

/// The last place a line of `text` may break at or before byte `end`, after
/// its start; None when there is none.
pub fn last_at_or_before(text: &[u8], end: usize) -> Option<usize> {
    let end = end.min(text.len());
    KNOWN.with(|k| k.borrow_mut().breaks(text, 0, end).iter().copied().take_while(|&b| b <= end).filter(|&b| b > 0).last())
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
        let text = game_text::from_str("匈雅提·亚诺什“谨慎者”Yuanzhang。");
        let end_of = |part: &str| {
            let part = game_text::from_str(part);
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
    fn a_line_cut_inside_a_latin_name_goes_back_to_its_start() {
        let text = game_text::from_str("由于缺乏显著事迹，Yuanzhang被历史学家称为");
        let name = text.windows(9).position(|w| w == b"Yuanzhang").unwrap();
        // Cut after "Yu": back to before "Y".
        assert_eq!(last_at_or_before(&text, name + 2), Some(name));
        // Cut between two Chinese characters: right there.
        assert_eq!(last_at_or_before(&text, 6), Some(6));
        assert_eq!(last_at_or_before(b"Yuanzhang", 4), None);
        assert_eq!(last_at_or_before(b"ab cd", 4), Some(3));
    }

    #[test]
    fn another_text_in_the_same_buffer_gets_its_own_breaks() {
        // Two texts of one length at one address, as when the game reuses a
        // buffer: 中文 may break after 中, but 中， may not.
        let mut buffer = "中文".as_bytes().to_vec();
        buffer.extend_from_slice("文".as_bytes());
        assert!(after(&buffer, 3));
        buffer[3..9].copy_from_slice("，文".as_bytes());
        assert!(!after(&buffer, 3));
    }
}
