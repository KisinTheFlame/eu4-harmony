//! Old saves and plugin-era mods' scripts keep characters as legacy escapes
//! (GLOSSARY). Everything the game reads from script files (`.txt`, `.gui`,
//! `.gfx`) and saves (plain text, compressed and ironman, and the save list's
//! titles) comes out of one of two lexers, a token at a time, into the lexer's
//! own buffer; the escapes are decoded there as each token is finished, so the
//! rest of the game only ever sees the module's game text.
//!
//! Both lexers share a layout: `+0x14` token type, `+0x18` the token's text
//! (512 bytes, NUL-terminated), `+0x21c` a flag set when the next call only
//! returns a token peeked before. Their "next token" methods are 0x170e700
//! (text) and 0x170ebf0 (binary); each returns 1 for a token, 0 at the end.
use super::legacy;
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicU64, Ordering};

const TEXT_TOKEN: usize = 0x170e700;
const BINARY_TOKEN: usize = 0x170ebf0;
const TEXT: usize = 0x18;
const TEXT_SIZE: usize = 0x200;
const PEEKED: usize = 0x21c;

static TEXT_ORIGINAL: Function<NextToken> = Function::new();
static BINARY_ORIGINAL: Function<NextToken> = Function::new();
static DECODED: AtomicU64 = AtomicU64::new(0);

type NextToken = unsafe extern "C" fn(usize) -> i32;

/// Decodes the escapes in a finished token's text, in place; the decoded
/// text is never longer.
unsafe fn decode_token(lexer: usize) {
    let text = (lexer + TEXT) as *mut u8;
    let len = (0..TEXT_SIZE).position(|i| *text.add(i) == 0).unwrap_or(TEXT_SIZE - 1);
    let bytes = std::slice::from_raw_parts(text, len);
    if let Some(decoded) = legacy::decode(bytes) {
        if decoded.len() <= len {
            std::ptr::copy_nonoverlapping(decoded.as_ptr(), text, decoded.len());
            *text.add(decoded.len()) = 0;
            DECODED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

unsafe fn next_token(lexer: usize, original: &Function<NextToken>) -> i32 {
    // A peeked token was decoded when it was read.
    let peeked = *((lexer + PEEKED) as *const u8) != 0;
    let next = original.get();
    let result = next(lexer);
    if result != 0 && !peeked {
        crate::guarded("legacy escapes in a token", || decode_token(lexer));
    }
    result
}

unsafe extern "C" fn text_token(lexer: usize) -> i32 {
    next_token(lexer, &TEXT_ORIGINAL)
}

unsafe extern "C" fn binary_token(lexer: usize) -> i32 {
    next_token(lexer, &BINARY_ORIGINAL)
}

/// How many tokens had legacy escapes, for the log.
pub fn decoded() -> u64 {
    DECODED.load(Ordering::Relaxed)
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(TEXT_TOKEN, "40 57 48 83 ec 40 80 b9 1c 02 00 00 01 48 8b f9", "script lexer next token", text_token as NextToken, &TEXT_ORIGINAL),
        Point::replace(
            BINARY_TOKEN,
            "48 89 5c 24 20 55 56 57 48 8b ec 48 83 ec 40 48",
            "binary save lexer next token",
            binary_token as NextToken,
            &BINARY_ORIGINAL,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_decoded_in_place() {
        let mut lexer = vec![0u8; 0x220];
        let escaped = b"\x10\x4a\x53\x10\x4a\x53 x";
        lexer[TEXT..TEXT + escaped.len()].copy_from_slice(escaped);
        unsafe { decode_token(lexer.as_ptr() as usize) };
        let end = lexer[TEXT..].iter().position(|&b| b == 0).unwrap();
        assert_eq!(&lexer[TEXT..TEXT + end], "半半 x".as_bytes());
    }
}
