//! One character of the game's text: the code point at a byte offset and how
//! many bytes it takes. Text is UTF-8; a byte that does not start a valid
//! UTF-8 sequence stands for itself, as the Windows-1252 byte the game used
//! to keep, so text the game still produces in that encoding draws as before.

/// Decodes the character starting at `bytes[0]`. Returns (code point, length);
/// an empty slice gives (0, 0).
pub fn decode(bytes: &[u8]) -> (u32, usize) {
    let Some(&lead) = bytes.first() else { return (0, 0) };
    let (len, initial) = match lead {
        0x00..=0x7f => return (lead as u32, 1),
        0xc2..=0xdf => (2, lead as u32 & 0x1f),
        0xe0..=0xef => (3, lead as u32 & 0x0f),
        0xf0..=0xf4 => (4, lead as u32 & 0x07),
        _ => return (lead as u32, 1),
    };
    if bytes.len() < len {
        return (lead as u32, 1);
    }
    let mut scalar = initial;
    for &b in &bytes[1..len] {
        if b & 0xc0 != 0x80 {
            return (lead as u32, 1);
        }
        scalar = scalar << 6 | (b as u32 & 0x3f);
    }
    // Overlong forms, surrogates and values past U+10FFFF are not UTF-8.
    let shortest = match len {
        2 => 0x80,
        3 => 0x800,
        _ => 0x10000,
    };
    if scalar < shortest || (0xd800..0xe000).contains(&scalar) || scalar > 0x10ffff {
        return (lead as u32, 1);
    }
    (scalar, len)
}

/// The start of the character that byte `at` belongs to, reading `text` from
/// its start; `at` itself when it starts one or lies past the end.
pub fn floor_boundary(text: &[u8], at: usize) -> usize {
    let mut start = 0;
    while start < text.len() {
        let next = start + decode(&text[start..]).1;
        if next > at {
            return start;
        }
        start = next;
    }
    at
}

/// How many bytes the character ending at `at` takes; 0 at the start.
pub fn previous_len(text: &[u8], at: usize) -> usize {
    match at.min(text.len()) {
        0 => 0,
        end => end - floor_boundary(text, end - 1),
    }
}

/// [`decode`] at `text + offset`, reading at most `end - offset` bytes; for
/// the text loops' stubs. Returns the code point in the low 32 bits and the
/// length in the high 32.
pub extern "C" fn decode_at(text: *const u8, offset: usize, end: usize) -> u64 {
    if text.is_null() || offset >= end {
        return 0;
    }
    let available = (end - offset).min(4);
    let (scalar, len) = decode(unsafe { std::slice::from_raw_parts(text.add(offset), available) });
    scalar as u64 | (len as u64) << 32
}

#[cfg(test)]
mod tests {
    use super::{decode, floor_boundary, previous_len};

    #[test]
    fn boundaries_follow_whole_characters() {
        // "a", 中 (3 bytes), Windows-1252 é, then "b".
        let text = b"a\xe4\xb8\xad\xe9b";
        assert_eq!([0, 1, 2, 3, 4, 5, 6, 9].map(|at| floor_boundary(text, at)), [0, 1, 1, 1, 4, 5, 6, 9]);
        assert_eq!([0, 1, 4, 5, 6].map(|at| previous_len(text, at)), [0, 1, 3, 1, 1]);
    }

    #[test]
    fn utf8_sequences_decode_whole() {
        assert_eq!(decode(b"A"), (0x41, 1));
        assert_eq!(decode("é".as_bytes()), (0xe9, 2));
        assert_eq!(decode("中".as_bytes()), (0x4e2d, 3));
        assert_eq!(decode("\u{20bb7}".as_bytes()), (0x20bb7, 4));
        assert_eq!(decode("§Y".as_bytes()), (0xa7, 2));
    }

    #[test]
    fn other_bytes_stand_for_themselves() {
        // Windows-1252 é, § and a cut-off sequence.
        assert_eq!(decode(b"\xe9t\xe9"), (0xe9, 1));
        assert_eq!(decode(b"\xa7Y"), (0xa7, 1));
        assert_eq!(decode(b"\xe4\xb8"), (0xe4, 1));
        // Overlong and surrogate forms.
        assert_eq!(decode(b"\xc0\xaf"), (0xc0, 1));
        assert_eq!(decode(b"\xed\xa0\x80"), (0xed, 1));
        assert_eq!(decode(b""), (0, 0));
    }
}
