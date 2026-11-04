//! Where text may wrap. The game's layout loops wrap only after a glyph with
//! no height (a space), so Chinese, which has no spaces, would never wrap;
//! they ask here after each character instead, and a line may also break
//! wherever the Unicode rules (ICU) allow after a non-ASCII character.
//!
//! A loop asks once per character, so the breaks of the text it walks are
//! kept between questions. They are looked up anew whenever the text is not
//! the one they were found for: another address or length, or other bytes
//! around where the loop now is (the game reuses buffers). Comparing the
//! whole text at every character would make a pass quadratic in its length;
//! a break depends on the characters next to it, and the rules that reach
//! further do so across spaces, so [`AROUND`] bytes on each side are
//! compared, and a loop meets any change on its way through the text.
//!
//! A frame lays out its texts one after another, and the next frame the same
//! texts again, so the breaks of every text met are kept too, by its whole
//! bytes ([`Kept`]), and ICU is asked only about a text not met before. A
//! text's breaks depend on its characters alone (ICU's rules are those of the
//! process's locale, which does not change), so a kept text answers what ICU
//! would.
use super::{costs, game_text};
use crate::icu::{self, LineBreak, LineBreaker};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Once;

/// How many bytes before and after the place asked about must be as they
/// were for the breaks found to stand.
const AROUND: usize = 16;

/// How much the texts kept may hold, their bytes and their breaks: many
/// frames' worth of the longest tooltips.
const KEPT_BYTES: usize = 4 << 20;

/// The breaks of the last text asked about.
#[derive(Default)]
struct Known {
    text: usize,
    bytes: Vec<u8>,
    /// Byte offsets a line may break before, ascending.
    breaks: Rc<[usize]>,
}

impl Known {
    /// The breaks of `text`, asked about at `text[start..end]`.
    fn breaks(&mut self, text: &[u8], start: usize, end: usize) -> &[usize] {
        costs::BREAK_QUESTIONS.add();
        let near = start.saturating_sub(AROUND)..(end + AROUND).min(text.len());
        let same = self.text == text.as_ptr() as usize && self.bytes.len() == text.len() && self.bytes.get(near.clone()) == text.get(near);
        if !same {
            let breaks = KEPT.with(|k| k.borrow_mut().breaks(text));
            *self = Known { text: text.as_ptr() as usize, bytes: text.to_vec(), breaks };
        }
        &self.breaks
    }
}

/// The breaks of the texts met, by their whole bytes, until they hold more
/// than their limit; then the least recently asked about go first.
struct Kept {
    texts: HashMap<Rc<[u8]>, Entry>,
    /// The texts by when they were last asked about, oldest first.
    order: BTreeMap<u64, Rc<[u8]>>,
    /// The number of the last question.
    asked: u64,
    bytes: usize,
    limit: usize,
}

/// A text kept: its breaks, and the number of the last question about it.
struct Entry {
    breaks: Rc<[usize]>,
    asked: u64,
}

impl Kept {
    fn new(limit: usize) -> Self {
        Self { texts: HashMap::new(), order: BTreeMap::new(), asked: 0, bytes: 0, limit }
    }

    /// What a text and its breaks count against the limit.
    fn size(text: &[u8], breaks: &[usize]) -> usize {
        text.len() + std::mem::size_of_val(breaks) + 64
    }

    /// The breaks of `text`: kept, or worked out by `find` and kept.
    fn get_or_find(&mut self, text: &[u8], find: impl FnOnce(&[u8]) -> Vec<usize>) -> Rc<[usize]> {
        self.asked += 1;
        if let Some(Entry { breaks, asked }) = self.texts.get_mut(text) {
            let key = self.order.remove(asked).expect("every text kept is in the order");
            *asked = self.asked;
            self.order.insert(self.asked, key);
            costs::BREAKS_KEPT.add();
            return breaks.clone();
        }
        let breaks: Rc<[usize]> = find(text).into();
        let size = Self::size(text, &breaks);
        if size <= self.limit {
            while self.bytes + size > self.limit {
                let (_, oldest) = self.order.pop_first().expect("texts are kept while bytes are counted");
                let gone = self.texts.remove(&oldest).expect("every text in the order is kept").breaks;
                self.bytes -= Self::size(&oldest, &gone);
            }
            let key: Rc<[u8]> = text.into();
            self.order.insert(self.asked, key.clone());
            self.texts.insert(key, Entry { breaks: breaks.clone(), asked: self.asked });
            self.bytes += size;
        }
        breaks
    }

    /// [`Self::get_or_find`] with ICU.
    fn breaks(&mut self, text: &[u8]) -> Rc<[usize]> {
        self.get_or_find(text, |text| costs::BREAKS.time(|| opportunities(text)))
    }
}

thread_local! {
    static KNOWN: RefCell<Known> = RefCell::new(Known::default());
    static KEPT: RefCell<Kept> = RefCell::new(Kept::new(KEPT_BYTES));
    /// ICU's line break iterator, opened at this thread's first text that is
    /// not kept; None when it cannot be.
    static BREAKER: RefCell<Option<LineBreaker>> = RefCell::new(icu::load().and_then(|icu| icu.line_breaker()).map_err(told).ok());
}

/// Logs, once, that ICU cannot find breaks: text then wraps only at spaces.
fn told(e: String) -> String {
    static TOLD: Once = Once::new();
    TOLD.call_once(|| log!("breaks: ICU cannot find line breaks, text wraps only at spaces: {e}"));
    e
}

/// The game's text as UTF-8, and where each character starts in both, the
/// ends of both last.
fn utf8_of(text: &[u8]) -> (String, Vec<(usize, usize)>) {
    let mut utf8 = String::with_capacity(text.len());
    let mut starts = Vec::with_capacity(text.len() + 1);
    for (c, range) in game_text::chars(text) {
        starts.push((utf8.len(), range.start));
        utf8.push(c);
    }
    starts.push((utf8.len(), text.len()));
    (utf8, starts)
}

/// ICU's breaks of the UTF-8, as byte offsets into the game's text.
fn in_game_text(breaks: Vec<LineBreak>, starts: &[(usize, usize)]) -> Vec<usize> {
    breaks.into_iter().filter_map(|b| starts.binary_search_by_key(&b.offset, |&(u, _)| u).ok().map(|i| starts[i].1)).collect()
}

/// Where lines of `text` may break, by byte offset, ascending. The text is
/// handed to ICU as UTF-8 and each break maps back to the game's bytes.
fn opportunities(text: &[u8]) -> Vec<usize> {
    let (utf8, starts) = utf8_of(text);
    let breaks = BREAKER.with(|b| match b.borrow_mut().as_mut() {
        Some(breaker) => breaker.line_breaks(&utf8).map_err(told).unwrap_or_default(),
        None => Vec::new(),
    });
    in_game_text(breaks, &starts)
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
///
/// # Safety
/// `text` is null or has `len` readable bytes.
pub unsafe extern "C" fn after_at(text: *const u8, len: u32, end: u32) -> bool {
    crate::guarded("text wrap", || !text.is_null() && after(unsafe { std::slice::from_raw_parts(text, len as usize) }, end as usize))
}

/// [`after`] for a stub that has the game's `std::string`.
///
/// # Safety
/// `string` is the game's `std::string`.
pub unsafe extern "C" fn after_in(string: *const u8, end: u32) -> bool {
    crate::guarded("text wrap", || after(unsafe { crate::game_string::text(string) }, end as usize))
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

    #[test]
    fn a_text_met_before_is_answered_from_those_kept() {
        let mut kept = Kept::new(1 << 20);
        let mut asked = 0;
        let mut find = |text: &[u8]| {
            asked += 1;
            vec![text.len()]
        };
        assert_eq!(&*kept.get_or_find(b"abc", &mut find), [3]);
        assert_eq!(&*kept.get_or_find(b"de", &mut find), [2]);
        assert_eq!(&*kept.get_or_find(b"abc", &mut find), [3]);
        assert_eq!(asked, 2);
    }

    #[test]
    fn past_the_limit_the_least_recently_asked_about_go_first() {
        let one = Kept::size(b"aaaa", &[4]);
        let mut kept = Kept::new(2 * one);
        let mut asked = Vec::new();
        let mut find = |text: &[u8]| {
            asked.push(text.to_vec());
            vec![text.len()]
        };
        kept.get_or_find(b"aaaa", &mut find);
        kept.get_or_find(b"bbbb", &mut find);
        // aaaa is asked about again, so bbbb is the oldest when cccc comes.
        kept.get_or_find(b"aaaa", &mut find);
        kept.get_or_find(b"cccc", &mut find);
        assert_eq!(kept.bytes, 2 * one);
        kept.get_or_find(b"aaaa", &mut find);
        kept.get_or_find(b"bbbb", &mut find);
        assert_eq!(asked, [b"aaaa".to_vec(), b"bbbb".to_vec(), b"cccc".to_vec(), b"bbbb".to_vec()]);
    }

    #[test]
    fn a_text_larger_than_the_limit_is_not_kept() {
        let mut kept = Kept::new(100);
        kept.get_or_find(&[b'x'; 200], |t| vec![t.len()]);
        assert_eq!((kept.bytes, kept.texts.len(), kept.order.len()), (0, 0, 0));
    }

    /// Checks every line of `files` twice, the second time answered from
    /// those kept, against ICU asked anew.
    fn kept_breaks_are_icus(files: &[String]) {
        let icu = icu::load().unwrap();
        let texts: Vec<Vec<u8>> = files.iter().flat_map(|file| file.lines()).map(game_text::from_str).collect();
        let mut kept = Kept::new(KEPT_BYTES);
        for _ in 0..2 {
            for text in &texts {
                let (utf8, starts) = utf8_of(text);
                let fresh = in_game_text(icu.line_breaks(&utf8).unwrap(), &starts);
                assert_eq!(&*kept.breaks(text), fresh, "{utf8}");
            }
        }
    }

    #[test]
    fn breaks_kept_are_icus_for_the_frameworks_texts() {
        let files = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/localisation")).unwrap();
        let texts: Vec<String> = files.filter_map(|f| std::fs::read_to_string(f.unwrap().path()).ok()).collect();
        assert!(texts.len() >= 5);
        kept_breaks_are_icus(&texts);
    }

    #[test]
    #[ignore = "needs the game, EU4_GAME_DIR"]
    fn breaks_kept_are_icus_for_the_games_texts() {
        let dir = std::path::Path::new(&std::env::var("EU4_GAME_DIR").expect("EU4_GAME_DIR")).join("localisation");
        let texts: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|f| f.ok().map(|f| f.path()))
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_l_english.yml")))
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .collect();
        assert!(!texts.is_empty());
        kept_breaks_are_icus(&texts);
    }

    #[test]
    fn breaks_are_worked_out_again_when_the_bytes_around_change() {
        let mut known = Known::default();
        let mut text = "中文中文中文中文中文中文".as_bytes().to_vec();
        known.breaks(&text, 3, 6);
        // Within AROUND of the place asked about: noticed.
        text[6 + AROUND - 1] = b'x';
        known.breaks(&text, 3, 6);
        assert_eq!(known.bytes, text);
        // Further on: left for the loop to meet when it gets there.
        let last = text.len() - 1;
        text[last] = b'y';
        known.breaks(&text, 3, 6);
        assert_ne!(known.bytes, text);
        known.breaks(&text, last - 3, last);
        assert_eq!(known.bytes, text);
    }
}
