//! Text as the game keeps it (GLOSSARY: game text): a character Windows-1252
//! has is its single 1252 byte, as the game always kept it; any other
//! character is its UTF-8 sequence. Formatting codes (`§`, `£`, `¤`) and all
//! European text stay byte for byte what the game's own code expects, while
//! Chinese and every other script fit too.
//!
//! Every part of the module reads game text through here, so the one rule
//! that matters lives in one place: a byte that starts no valid UTF-8
//! sequence is the 1252 character it stands for. Only 1252 bytes that happen
//! to form a UTF-8 sequence (`Ã©` reads as `é`) are misread, which real text
//! hardly contains. Chinese text holds such lone bytes too (`·`, `“`, `…`), so
//! code that hands text to anything expecting UTF-8 converts it first.

/// The 1252 bytes 0x80..=0x9F that are not their own code point, by code point.
const SPECIALS: [(u32, u8); 27] = [
    (0x20ac, 0x80),
    (0x201a, 0x82),
    (0x0192, 0x83),
    (0x201e, 0x84),
    (0x2026, 0x85),
    (0x2020, 0x86),
    (0x2021, 0x87),
    (0x02c6, 0x88),
    (0x2030, 0x89),
    (0x0160, 0x8a),
    (0x2039, 0x8b),
    (0x0152, 0x8c),
    (0x017d, 0x8e),
    (0x2018, 0x91),
    (0x2019, 0x92),
    (0x201c, 0x93),
    (0x201d, 0x94),
    (0x2022, 0x95),
    (0x2013, 0x96),
    (0x2014, 0x97),
    (0x02dc, 0x98),
    (0x2122, 0x99),
    (0x0161, 0x9a),
    (0x203a, 0x9b),
    (0x0153, 0x9c),
    (0x017e, 0x9e),
    (0x0178, 0x9f),
];

/// The UTF-8 sequence starting at `bytes[0]`: (code point, length), or None
/// when the bytes there start no valid one.
fn utf8_at(bytes: &[u8]) -> Option<(u32, usize)> {
    let &lead = bytes.first()?;
    let (len, initial) = match lead {
        0x00..=0x7f => return Some((lead as u32, 1)),
        0xc2..=0xdf => (2, lead as u32 & 0x1f),
        0xe0..=0xef => (3, lead as u32 & 0x0f),
        0xf0..=0xf4 => (4, lead as u32 & 0x07),
        _ => return None,
    };
    let rest = bytes.get(1..len)?;
    let mut scalar = initial;
    for &b in rest {
        if b & 0xc0 != 0x80 {
            return None;
        }
        scalar = scalar << 6 | (b as u32 & 0x3f);
    }
    // Overlong forms, surrogates and values past U+10FFFF are not UTF-8.
    let shortest = match len {
        2 => 0x80,
        3 => 0x800,
        _ => 0x10000,
    };
    (scalar >= shortest && !(0xd800..0xe000).contains(&scalar) && scalar <= 0x10ffff).then_some((scalar, len))
}

/// The character starting at `text[0]` and how many bytes it takes;
/// ('\0', 0) for no text.
pub fn char_at(text: &[u8]) -> (char, usize) {
    match (utf8_at(text), text.first()) {
        (Some((scalar, len)), _) => (char::from_u32(scalar).unwrap_or('\u{fffd}'), len),
        (None, Some(&byte)) => {
            let scalar = SPECIALS.iter().find(|&&(_, b)| b == byte).map_or(byte as u32, |&(c, _)| c);
            (char::from_u32(scalar).unwrap_or('\u{fffd}'), 1)
        }
        (None, None) => ('\0', 0),
    }
}

/// How many bytes the character at `text[at]` takes; 0 at or past the end.
pub fn len_at(text: &[u8], at: usize) -> usize {
    text.get(at..).map_or(0, |rest| char_at(rest).1)
}

/// The characters of `text` and where each lies.
pub fn chars(text: &[u8]) -> impl Iterator<Item = (char, std::ops::Range<usize>)> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        let (c, len) = char_at(text.get(at..)?);
        (len > 0).then(|| {
            at += len;
            (c, at - len..at)
        })
    })
}

/// The character that ends at byte `end`, and where it starts, looking back
/// no more than a UTF-8 sequence's length; None at the start or past the end.
/// Unlike [`floor_boundary`] it does not read the text from its start, so a
/// loop can ask at every character.
pub fn char_before(text: &[u8], end: usize) -> Option<(char, usize)> {
    if end == 0 || end > text.len() {
        return None;
    }
    let mut start = end - 1;
    while start > 0 && end - start < 4 && text[start] & 0xc0 == 0x80 {
        start -= 1;
    }
    match char_at(&text[start..end]) {
        (c, len) if start + len == end => Some((c, start)),
        // Continuation bytes on their own: the last is a character itself.
        _ => Some((char_at(&text[end - 1..end]).0, end - 1)),
    }
}

/// The start of the character that byte `at` belongs to, reading `text` from
/// its start; `at` itself when it starts one or lies past the end.
pub fn floor_boundary(text: &[u8], at: usize) -> usize {
    chars(text).map(|(_, r)| r).find(|r| r.end > at).map_or(at, |r| r.start)
}

/// How many bytes the character ending at `at` takes; 0 at the start.
pub fn previous_len(text: &[u8], at: usize) -> usize {
    match at.min(text.len()) {
        0 => 0,
        end => end - floor_boundary(text, end - 1),
    }
}

/// The game's single byte for a code point, if Windows-1252 has it. Code
/// points below 0x100 map to themselves, as the C1 controls come back from
/// bytes 0x81, 0x8D, 0x8F, 0x90 and 0x9D that 1252 leaves undefined.
pub fn single_byte(scalar: u32) -> Option<u8> {
    if scalar < 0x100 {
        return Some(scalar as u8);
    }
    SPECIALS.iter().find(|&&(c, _)| c == scalar).map(|&(_, b)| b)
}

/// `text` as the game keeps it.
pub fn from_str(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        push(&mut out, c);
    }
    out
}

/// Appends `c` to game text: its Windows-1252 byte when it has one, or its
/// UTF-8.
pub fn push(out: &mut Vec<u8>, c: char) {
    match single_byte(c as u32) {
        Some(b) => out.push(b),
        None => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
    }
}

/// Changes, in place, every byte of `text` that is a character of its own
/// (ASCII or Windows-1252) to what `change` makes of it.
pub fn map_single_bytes(text: &mut [u8], change: impl Fn(u8) -> u8) {
    let mut at = 0;
    while at < text.len() {
        let n = len_at(text, at);
        if n == 1 {
            text[at] = change(text[at]);
        }
        at += n;
    }
}

/// The bytes at `text` up to its NUL, or up to `most` of them.
///
/// # Safety
/// `text` is readable up to its NUL or for `most` bytes.
pub unsafe fn up_to_nul<'a>(text: *const u8, most: usize) -> &'a [u8] {
    let mut len = 0;
    while len < most && *text.add(len) != 0 {
        len += 1;
    }
    std::slice::from_raw_parts(text, len)
}

/// Text the game keeps, as UTF-8.
pub fn to_utf8(text: &[u8]) -> String {
    chars(text).map(|(c, _)| c).collect()
}

/// Text the game keeps, as UTF-8 without its formatting codes: a color
/// (`§` and one character) and an icon (`£`, its name of letters and digits,
/// and the character that ends it) are left out, as the game does not show
/// them as text (docs/research/unicode-text.md).
pub fn plain(text: &[u8]) -> String {
    let mut out = String::new();
    let mut chars = chars(text).map(|(c, _)| c).peekable();
    while let Some(c) = chars.next() {
        match c {
            '§' => {
                chars.next();
            }
            '£' => {
                while chars.next_if(|c| c.is_ascii_alphanumeric() || *c == '_').is_some() {}
                chars.next();
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_sequences_read_whole() {
        assert_eq!(char_at(b"A"), ('A', 1));
        assert_eq!(char_at("é".as_bytes()), ('é', 2));
        assert_eq!(char_at("中".as_bytes()), ('中', 3));
        assert_eq!(char_at("\u{20bb7}".as_bytes()), ('\u{20bb7}', 4));
        assert_eq!(char_at(b""), ('\0', 0));
    }

    #[test]
    fn other_bytes_are_their_1252_characters() {
        assert_eq!(char_at(b"\xe9t"), ('é', 1));
        assert_eq!(char_at(b"\xa7Y"), ('§', 1));
        assert_eq!(char_at(b"\x93x"), ('\u{201c}', 1));
        assert_eq!(char_at(b"\x85"), ('…', 1));
        // A cut-off sequence, an overlong form and a surrogate.
        assert_eq!(char_at(b"\xe4\xb8"), ('ä', 1));
        assert_eq!(char_at(b"\xc0\xaf"), ('À', 1));
        assert_eq!(char_at(b"\xed\xa0\x80"), ('í', 1));
    }

    #[test]
    fn boundaries_follow_whole_characters() {
        // "a", 中 (3 bytes), Windows-1252 é, then "b".
        let text = b"a\xe4\xb8\xad\xe9b";
        assert_eq!([0, 1, 2, 3, 4, 5, 6, 9].map(|at| floor_boundary(text, at)), [0, 1, 1, 1, 4, 5, 6, 9]);
        assert_eq!([0, 1, 4, 5, 6].map(|at| previous_len(text, at)), [0, 1, 3, 1, 1]);
        assert_eq!([0, 1, 4, 5, 6].map(|at| len_at(text, at)), [1, 3, 1, 1, 0]);
        let ranges: Vec<_> = chars(text).map(|(_, r)| r).collect();
        assert_eq!(ranges, [0..1, 1..4, 4..5, 5..6]);
        assert_eq!([1, 4, 5, 6].map(|end| char_before(text, end)), [Some(('a', 0)), Some(('中', 1)), Some(('é', 4)), Some(('b', 5))]);
        assert_eq!(char_before(text, 0), None);
        assert_eq!(char_before(text, 7), None);
    }

    #[test]
    fn game_text_reads_back_as_utf8() {
        for text in ["§YGröße€ …", "中文£gold£", "plain", "匈雅提·亚诺什“谨慎者”"] {
            assert_eq!(to_utf8(&from_str(text)), text);
        }
    }

    #[test]
    fn plain_text_leaves_formatting_codes_out() {
        assert_eq!(plain(&from_str("§R没有足够的§Y10§!£gold£金钱")), "没有足够的10金钱");
        assert_eq!(plain(&from_str("Costs £adm 50 points§")), "Costs 50 points");
        assert_eq!(plain(&from_str("Île-de-France · 巴黎")), "Île-de-France · 巴黎");
    }

    #[test]
    fn european_text_and_codes_are_single_bytes() {
        assert_eq!(from_str("§YGröße€ …"), b"\xa7YGr\xf6\xdfe\x80 \x85");
        assert_eq!(from_str("§Y中文£gold£"), b"\xa7Y\xe4\xb8\xad\xe6\x96\x87\xa3gold\xa3");
    }
}
