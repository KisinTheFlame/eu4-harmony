//! How the Unicode module keeps text in the game: a character Windows-1252
//! has is its single 1252 byte, as the game always kept it; any other
//! character is its UTF-8 sequence. Formatting codes (`§`, `£`, `¤`) and all
//! European text stay byte for byte what the game's own code expects, while
//! Chinese and every other script fit too. `scalar::decode` reads a byte that
//! starts no valid UTF-8 sequence as itself; only 1252 bytes that happen to
//! form one (`Ã©` reads as `é`) are misread, which real text hardly contains.

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

/// The game's single byte for a code point, if Windows-1252 has it. Code
/// points below 0x100 map to themselves, which is how the game's own lone
/// bytes 0x80..=0x9F come back from decoding.
pub fn single_byte(scalar: u32) -> Option<u8> {
    if scalar < 0x100 {
        return Some(scalar as u8);
    }
    SPECIALS.iter().find(|&&(c, _)| c == scalar).map(|&(_, b)| b)
}

/// `text` as the game keeps it.
pub fn game_text(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        match single_byte(c as u32) {
            Some(b) => out.push(b),
            None => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    out
}

/// The character at the start of text the game keeps, and how many bytes it
/// takes; ('\0', 0) for no text.
pub fn char_at(text: &[u8]) -> (char, usize) {
    let (scalar, len) = super::scalar::decode(text);
    // A lone byte is its Windows-1252 character.
    let scalar = match len {
        1 => SPECIALS.iter().find(|&&(_, b)| b as u32 == scalar).map_or(scalar, |&(c, _)| c),
        _ => scalar,
    };
    (char::from_u32(scalar).unwrap_or('\u{fffd}'), len)
}

/// Text the game keeps, as UTF-8.
pub fn to_utf8(text: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let (c, len) = char_at(&text[at..]);
        out.push(c);
        at += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unicode::scalar::decode;

    #[test]
    fn game_text_reads_back_as_utf8() {
        for text in ["§YGröße€ …", "中文£gold£", "plain"] {
            assert_eq!(to_utf8(&game_text(text)), text);
        }
    }

    #[test]
    fn european_text_and_codes_are_single_bytes() {
        assert_eq!(game_text("§YGröße€ …"), b"\xa7YGr\xf6\xdfe\x80 \x85");
    }

    #[test]
    fn other_characters_stay_utf8_and_decode_back() {
        let text = game_text("§Y中文£gold£");
        assert_eq!(text, b"\xa7Y\xe4\xb8\xad\xe6\x96\x87\xa3gold\xa3");
        let mut at = 0;
        let mut scalars = Vec::new();
        while at < text.len() {
            let (c, len) = decode(&text[at..]);
            scalars.push(c);
            at += len;
        }
        assert_eq!(scalars, [0xa7, 'Y' as u32, 0x4e2d, 0x6587, 0xa3, 'g' as u32, 'o' as u32, 'l' as u32, 'd' as u32, 0xa3]);
    }
}
