//! Text ingestion into the Unicode module's UTF-8 game text:
//!
//! 1. **Legacy escapes**: three-byte escapes (`0x10..=0x13` + low/high UTF-16
//!    bytes) used by older CJK language mods and saves are decoded to UTF-8.
//! 2. **Localisation import** (`0x16fd650` -> `0x16fa8d0`): intercepts UTF-8
//!    entries before the game narrows them to Windows-1252, decoding legacy
//!    escapes when present.
//! 3. **Script & binary lexers** (`0x170e700`, `0x170ebf0`): decodes legacy
//!    escapes in place on each newly produced token buffer so the rest of the
//!    game sees only game text.
use super::game_text;
use crate::base::called_by_game;
use crate::native::game_string;
use crate::native::points::{Function, Point};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------- legacy escapes ----------

/// Decodes the legacy escapes in game text, turning each into the
/// character's UTF-8. None when the text has none. A marker without the two
/// bytes after it, or one that makes no character, is left as it is.
pub(super) fn decode_legacy(text: &[u8]) -> Option<Vec<u8>> {
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
                game_text::push(&mut out, c);
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

// ---------- localisation import ----------

const IMPORT: usize = 0x16fd650;
const REGISTER: usize = 0x16fa8d0;

static IMPORT_ORIGINAL: Function<Import> = Function::new();
static REGISTER_AT: Function<Register> = Function::new();
static KEPT: AtomicU64 = AtomicU64::new(0);
/// Entries that are not UTF-8, left to the game's own conversion.
static LEFT: AtomicU64 = AtomicU64::new(0);
static LEGACY: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
struct Context {
    line: i32,
    replace: u8,
    _pad: [u8; 11],
    collection: usize,
}

type Import = unsafe extern "C" fn(*const u8, *const u8, u32, usize, *const Context);
type Register = unsafe extern "C" fn(usize, *const u8, *const u8, i32, u32, u8);

/// `string` as NUL-terminated game text, when it is UTF-8. An older Chinese
/// language mod carries legacy escapes as the 1252 characters of their
/// bytes; they become plain characters here.
unsafe fn converted(string: *const u8) -> Option<Vec<u8>> {
    let mut text = game_text::from_str(std::str::from_utf8(game_string::text(string)).ok()?);
    if let Some(decoded) = decode_legacy(&text) {
        LEGACY.fetch_add(1, Ordering::Relaxed);
        text = decoded;
    }
    text.push(0);
    Some(text)
}

#[called_by_game]
unsafe extern "C" fn import(key: *const u8, value: *const u8, version: u32, unused: usize, context: *const Context) {
    // The registration copies both strings.
    let texts = crate::base::guarded("localisation import", || converted(key).zip(converted(value)));
    if let Some((key, value)) = texts {
        KEPT.fetch_add(1, Ordering::Relaxed);
        let register = REGISTER_AT.get();
        let c = &*context;
        register(c.collection, key.as_ptr(), value.as_ptr(), c.line, version, c.replace);
    } else {
        LEFT.fetch_add(1, Ordering::Relaxed);
        let original = IMPORT_ORIGINAL.get();
        original(key, value, version, unused, context);
    }
}

/// Entries converted here, of them those with legacy escapes, and entries
/// left to the game's conversion.
pub(super) fn localisation_counts() -> (u64, u64, u64) {
    (KEPT.load(Ordering::Relaxed), LEGACY.load(Ordering::Relaxed), LEFT.load(Ordering::Relaxed))
}

// ---------- script & binary lexers ----------

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
    if let Some(decoded) = decode_legacy(bytes) {
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
        crate::base::guarded("legacy escapes in a token", || decode_token(lexer));
    }
    result
}

#[called_by_game]
unsafe extern "C" fn text_token(lexer: usize) -> i32 {
    next_token(lexer, &TEXT_ORIGINAL)
}

#[called_by_game]
unsafe extern "C" fn binary_token(lexer: usize) -> i32 {
    next_token(lexer, &BINARY_ORIGINAL)
}

/// How many tokens had legacy escapes, for the log.
pub(super) fn decoded_tokens() -> u64 {
    DECODED.load(Ordering::Relaxed)
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::replace(IMPORT, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "localisation entry import", import as Import, &IMPORT_ORIGINAL),
        Point::keep(REGISTER, "44 89 4c 24 20 4c 89 44 24 18 53 57 41 55 41 56", "localisation entry registration").recorded(REGISTER_AT.slot()),
        // How the import reads its context and calls the registration.
        Point::keep(0x16fd720usize, "48 8b 84 24 a0 00 00 00 48 8b 48 10 44 8b 08 0f b6 50 04 eb 2d", "localisation import context"),
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
    fn decodes_escapes_with_and_without_shifted_bytes() {
        // 半 (U+534A): neither byte shifted.
        assert_eq!(decode_legacy(b"\x10\x4a\x53").unwrap(), "半".as_bytes());
        // A high byte 0x5B ('[') was lowered by 9, a low byte 0x20 raised by 0x0E.
        assert_eq!(decode_legacy(b"\x12\x00\x52").unwrap(), "\u{5b00}".as_bytes());
        assert_eq!(decode_legacy(b"\x11\x2e\x4e").unwrap(), "\u{4e20}".as_bytes());
        assert_eq!(decode_legacy(b"x\x10\x4a\x53y").unwrap(), "x半y".as_bytes());
    }

    #[test]
    fn text_without_escapes_or_with_a_cut_one_is_left() {
        assert_eq!(decode_legacy(b"plain"), None);
        assert_eq!(decode_legacy(b"cut\x10\x4a"), None);
    }

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
