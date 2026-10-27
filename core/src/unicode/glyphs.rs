//! The glyph table (GLOSSARY): glyphs for code points above 0xFF, which the
//! game's own per-font table cannot hold, and the temporary glyphs of one draw
//! call. It only keeps them; which glyph a character gets is `text_loops`'s.
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
