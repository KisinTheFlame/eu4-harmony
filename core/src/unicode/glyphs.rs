//! The glyph table (GLOSSARY): glyphs for code points above 0xFF, which the
//! game's own per-font table cannot hold, and the temporary glyphs of one draw
//! call.
//!
//! A font's own table is 256 glyph pointers at font + 0x120; the game frees it
//! with the font (`font_load`). Glyphs for higher code points are kept here,
//! keyed by that table's address, in the same 16-byte layout, so text code can
//! use them wherever it uses the game's own.
//!
//! Temporary glyphs (ADR 0010) take code points from U+F0000 up for as long as
//! one draw call runs, so text shaped as a whole can travel through the game's
//! per-character code as ordinary characters.
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// A glyph as the game stores it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub x_offset: i16,
    pub y_offset: i16,
    pub advance: i16,
    /// Whether kerning pairs start with this glyph; the game reads it.
    pub kerned: u8,
    pub page: u8,
}
const _: () = assert!(std::mem::size_of::<Glyph>() == 16);

/// The first code point temporary glyphs use.
pub const TEMPORARY: u32 = 0xF0000;
/// One past the last code point temporary glyphs may use.
const TEMPORARY_END: u32 = 0xFFFFE;

static TABLE: LazyLock<RwLock<HashMap<(usize, u32), Box<Glyph>>>> = LazyLock::new(Default::default);

thread_local! {
    static TEMPORARIES: RefCell<Vec<Box<Glyph>>> = const { RefCell::new(Vec::new()) };
}

/// Adds the glyph for `scalar` to the font whose own table is at `table`.
/// Returns false, keeping the first, when the font already has one.
pub fn insert(table: usize, scalar: u32, glyph: Glyph) -> bool {
    let mut map = TABLE.write().unwrap_or_else(|e| e.into_inner());
    match map.entry((table, scalar)) {
        std::collections::hash_map::Entry::Occupied(_) => false,
        std::collections::hash_map::Entry::Vacant(v) => {
            v.insert(Box::new(glyph));
            true
        }
    }
}

/// The glyph for a code point above 0xFF, or null when the font has none. The
/// pointer stays valid until the font is forgotten, or, for a temporary glyph,
/// until its draw call ends.
pub fn get(table: usize, scalar: u32) -> *const Glyph {
    if (TEMPORARY..TEMPORARY_END).contains(&scalar) {
        let found = TEMPORARIES.with(|t| t.borrow().get((scalar - TEMPORARY) as usize).map(|g| &**g as *const Glyph));
        if let Some(found) = found {
            return found;
        }
    }
    TABLE.read().unwrap_or_else(|e| e.into_inner()).get(&(table, scalar)).map_or(std::ptr::null(), |g| &**g as *const Glyph)
}

/// The glyph `font` draws for `scalar`: from its own table for a character
/// Windows-1252 has, from the glyph table otherwise, then from a system font
/// (`fill`), and its `?` when none has it. Null only where the game's own
/// table is (control characters).
pub fn for_scalar(font: usize, scalar: u32) -> *const Glyph {
    let table = font + 0x120;
    let own = |byte: u8| unsafe { *(table as *const *const Glyph).add(byte as usize) };
    if let Some(byte) = super::encoding::single_byte(scalar) {
        return own(byte);
    }
    let found = get(table, scalar);
    if !found.is_null() {
        return found;
    }
    match super::fill::glyph(font, scalar) {
        filled if !filled.is_null() => filled,
        _ => own(b'?'),
    }
}

/// The character starting at `text` (read no further than its NUL) and the
/// glyph `font` draws for it.
pub unsafe fn char_at(font: usize, text: *const u8) -> (*const Glyph, usize) {
    let mut bytes = [0u8; 4];
    let mut n = 0;
    while n < 4 {
        bytes[n] = *text.add(n);
        n += 1;
        if bytes[n - 1] == 0 {
            break;
        }
    }
    let (scalar, len) = super::scalar::decode(&bytes[..n]);
    (for_scalar(font, scalar), len.max(1))
}

/// [`char_at`] for the text loops' stubs; the length goes to `*len`.
pub extern "C" fn char_glyph(font: usize, text: *const u8, len: *mut u32) -> *const Glyph {
    crate::guarded("text glyph", || {
        let (glyph, n) = unsafe { char_at(font, text) };
        unsafe { *len = n as u32 };
        glyph
    })
}

/// The game's `std::string::append(this, bytes, count)`, recorded at startup.
pub static APPEND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub const STRING_APPEND: usize = 0x932f0;

type Append = unsafe extern "C" fn(usize, *const u8, usize) -> usize;

/// For layout passes that copy each byte into a word (`std::string*`) before
/// looking it up: the glyph for the character at `text`, with the rest of its
/// bytes appended to `word`; the length goes to `*len`.
pub extern "C" fn layout_char(font: usize, text: *const u8, word: usize, len: *mut u32) -> *const Glyph {
    crate::guarded("text layout glyph", || unsafe {
        let (glyph, n) = char_at(font, text);
        if n > 1 {
            let append: Append = std::mem::transmute(APPEND.load(std::sync::atomic::Ordering::Acquire));
            append(word, text.add(1), n - 1);
        }
        *len = n as u32;
        glyph
    })
}

/// The font whose own table is at `table` is being destroyed.
pub fn forget(table: usize) {
    TABLE.write().unwrap_or_else(|e| e.into_inner()).retain(|&(t, _), _| t != table);
}

/// How many glyphs the table holds, for the report.
pub fn count() -> usize {
    TABLE.read().unwrap_or_else(|e| e.into_inner()).len()
}

/// The temporary glyphs of one draw call on this thread; they end with it.
pub struct DrawCall(());

impl DrawCall {
    pub fn begin() -> Self {
        TEMPORARIES.with(|t| t.borrow_mut().clear());
        Self(())
    }

    /// Gives `glyph` a code point that means it until the call ends, or None
    /// when the call has used them all.
    pub fn temporary(&self, glyph: Glyph) -> Option<u32> {
        TEMPORARIES.with(|t| {
            let mut t = t.borrow_mut();
            let scalar = TEMPORARY + t.len() as u32;
            (scalar < TEMPORARY_END).then(|| {
                t.push(Box::new(glyph));
                scalar
            })
        })
    }
}

impl Drop for DrawCall {
    fn drop(&mut self) {
        TEMPORARIES.with(|t| t.borrow_mut().clear());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(x: u16) -> Glyph {
        Glyph { x, width: 12, height: 14, advance: 12, ..Default::default() }
    }

    #[test]
    fn keeps_the_first_glyph_per_font_and_forgets_a_font() {
        let (a, b) = (0x1000_0120, 0x2000_0120);
        assert!(insert(a, 0x4e2d, glyph(1)));
        assert!(!insert(a, 0x4e2d, glyph(2)));
        assert!(insert(b, 0x4e2d, glyph(3)));
        assert_eq!(unsafe { *get(a, 0x4e2d) }.x, 1);
        assert_eq!(unsafe { *get(b, 0x4e2d) }.x, 3);
        assert!(get(a, 0x4e2e).is_null());
        forget(a);
        assert!(get(a, 0x4e2d).is_null());
        assert_eq!(unsafe { *get(b, 0x4e2d) }.x, 3);
    }

    #[test]
    fn temporary_glyphs_last_for_their_draw_call() {
        let table = 0x3000_0120;
        let first;
        {
            let call = DrawCall::begin();
            first = call.temporary(glyph(7)).unwrap();
            let second = call.temporary(glyph(8)).unwrap();
            assert_eq!((first, second), (TEMPORARY, TEMPORARY + 1));
            assert_eq!(unsafe { *get(table, second) }.x, 8);
        }
        assert!(get(table, first).is_null());
    }
}
