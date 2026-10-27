//! Legacy escapes (GLOSSARY): how the CJK font plugin carried a character in
//! the game's single-byte text. Everything the framework knows about them is
//! here; the import points call [`decode`] and everything after them sees
//! only the module's game text.
//!
//! An escape is three bytes: a marker 0x10..=0x13, then the low and the high
//! byte of a UTF-16 code unit. Bytes the game treats specially are shifted
//! out of the way first: marker bit 0 says the low byte was raised by 0x0E,
//! bit 1 that the high byte was lowered by 9.

/// Decodes the legacy escapes in game text, turning each into the
/// character's UTF-8. None when the text has none. A marker without the two
/// bytes after it, or one that makes no character, is left as it is.
pub fn decode(text: &[u8]) -> Option<Vec<u8>> {
    if !text.iter().any(|b| (0x10..=0x13).contains(b)) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len());
    let mut found = false;
    let mut i = 0;
    while i < text.len() {
        let marker = text[i];
        if (0x10..=0x13).contains(&marker) && i + 2 < text.len() {
            let mut low = text[i + 1];
            let mut high = text[i + 2];
            if marker & 1 != 0 {
                low = low.wrapping_sub(0x0e);
            }
            if marker & 2 != 0 {
                high = high.wrapping_add(0x09);
            }
            if let Some(c) = char::from_u32(u32::from(high) << 8 | u32::from(low)) {
                // As the module keeps text: a character 1252 has as its byte.
                match super::encoding::single_byte(c as u32) {
                    Some(b) => out.push(b),
                    None => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
                }
                found = true;
                i += 3;
                continue;
            }
        }
        out.push(marker);
        i += 1;
    }
    found.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_escapes_with_and_without_shifted_bytes() {
        // 半 (U+534A): neither byte shifted.
        assert_eq!(decode(b"\x10\x4a\x53").unwrap(), "半".as_bytes());
        // A high byte 0x5B ('[') was lowered by 9, a low byte 0x20 raised by 0x0E.
        assert_eq!(decode(b"\x12\x00\x52").unwrap(), "\u{5b00}".as_bytes());
        assert_eq!(decode(b"\x11\x2e\x4e").unwrap(), "\u{4e20}".as_bytes());
        assert_eq!(decode(b"x\x10\x4a\x53y").unwrap(), "x半y".as_bytes());
    }

    #[test]
    fn text_without_escapes_or_with_a_cut_one_is_left() {
        assert_eq!(decode(b"plain"), None);
        assert_eq!(decode(b"cut\x10\x4a"), None);
    }
}
