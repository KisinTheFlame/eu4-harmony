//! Localisation keeps every character. The game imports each key and value of
//! a localisation file through the function at 0x16fd650, which converts both
//! from UTF-8 to Windows-1252 into static buffers (a character 1252 lacks
//! becomes `?`) and passes them to the registration at 0x16fa8d0. The
//! replacement converts UTF-8 to the module's game text instead (`game_text`);
//! anything that is not UTF-8 still takes the game's path.
//!
//! Import: (key, value: `std::string*`, version: u32, unused, context*), the
//! context holding the line (`+0`, i32), whether the entry replaces an
//! existing one (`+4`, u8) and the collection (`+0x10`). Registration:
//! (collection, key, value: NUL-terminated, line, version, replace).
use super::game_text;
use crate::game_string;
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicU64, Ordering};

const IMPORT: usize = 0x16fd650;
const REGISTER: usize = 0x16fa8d0;

static ORIGINAL: Function<Import> = Function::new();
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

/// `string` as NUL-terminated game text, when it is UTF-8. A Chinese
/// localisation mod made for the CJK font plugin carries legacy escapes as
/// the 1252 characters of their bytes; they become plain characters here.
unsafe fn converted(string: *const u8) -> Option<Vec<u8>> {
    let mut text = game_text::from_str(std::str::from_utf8(game_string::text(string)).ok()?);
    if let Some(decoded) = super::legacy::decode(&text) {
        LEGACY.fetch_add(1, Ordering::Relaxed);
        text = decoded;
    }
    text.push(0);
    Some(text)
}

unsafe extern "C" fn import(key: *const u8, value: *const u8, version: u32, unused: usize, context: *const Context) {
    // The registration copies both strings.
    let texts = crate::guarded("localisation import", || converted(key).zip(converted(value)));
    if let Some((key, value)) = texts {
        KEPT.fetch_add(1, Ordering::Relaxed);
        let register = REGISTER_AT.get();
        let c = &*context;
        register(c.collection, key.as_ptr(), value.as_ptr(), c.line, version, c.replace);
    } else {
        LEFT.fetch_add(1, Ordering::Relaxed);
        let original = ORIGINAL.get();
        original(key, value, version, unused, context);
    }
}

/// Entries converted here, of them those with legacy escapes, and entries
/// left to the game's conversion.
pub fn counts() -> (u64, u64, u64) {
    (KEPT.load(Ordering::Relaxed), LEGACY.load(Ordering::Relaxed), LEFT.load(Ordering::Relaxed))
}

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(IMPORT, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "localisation entry import", import as Import, &ORIGINAL),
        Point::keep(REGISTER, "44 89 4c 24 20 4c 89 44 24 18 53 57 41 55 41 56", "localisation entry registration").recorded(REGISTER_AT.slot()),
        // How the import reads its context and calls the registration.
        Point::keep(0x16fd720usize, "48 8b 84 24 a0 00 00 00 48 8b 48 10 44 8b 08 0f b6 50 04 eb 2d", "localisation import context"),
    ]
}
