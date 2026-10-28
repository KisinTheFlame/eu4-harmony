//! The glyph table (GLOSSARY): glyphs for code points above 0xFF, which the
//! game's own per-font table cannot hold, and the temporary glyphs of one draw
//! call. It only keeps them; which glyph a character gets is `text_loops`'s.
//!
//! A font's own table is 256 glyph pointers at font + 0x120; the game frees it
//! with the font (`fonts`, which also seals and clears a font's glyphs here). Glyphs for higher code points are kept here,
//! by that table's address, in the same 16-byte layout, so text code can use
//! them wherever it uses the game's own.
//!
//! Many fonts load the same `.fnt` file: the Chinese language mod's 46 fonts
//! use a handful, each with some 21,000 glyphs above 0xFF. So the glyphs a
//! font brings from its file are collected while it loads, and once it has
//! loaded ([`seal`]) they become a set shared with every font that brought the
//! same glyphs; kept per font, they took 66 MB. A set never changes once
//! made, so a glyph in it stays where it is as long as a font holds the set.
//! Glyphs added to a font after that (drawn from a system font) are its own.
//!
//! Temporary glyphs (ADR 0010) take code points from U+F0000 up for as long as
//! one draw call runs, so text shaped as a whole can travel through the game's
//! per-character code as ordinary characters.
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex, RwLock, Weak};

/// A glyph as the game stores it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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

/// Glyphs sorted by code point, shared by the fonts that brought them.
#[derive(PartialEq, Eq, Hash)]
struct Set {
    scalars: Box<[u32]>,
    glyphs: Box<[Glyph]>,
}

impl Set {
    fn get(&self, scalar: u32) -> Option<&Glyph> {
        self.scalars.binary_search(&scalar).ok().map(|i| &self.glyphs[i])
    }
}

#[derive(Default)]
struct Font {
    /// While the font loads: the glyphs its file has brought so far, and
    /// their code points, to keep the first of each.
    loading: Vec<(u32, Glyph)>,
    seen: HashSet<u32>,
    /// Once it has loaded: the glyphs its file brought.
    set: Option<Arc<Set>>,
    /// Glyphs added since, each kept where it is.
    added: HashMap<u32, Box<Glyph>>,
}

static FONTS: LazyLock<RwLock<HashMap<usize, Font>>> = LazyLock::new(Default::default);
/// Every set some font holds, by a hash of its glyphs.
static SETS: LazyLock<Mutex<HashMap<u64, Vec<Weak<Set>>>>> = LazyLock::new(Default::default);

thread_local! {
    static TEMPORARIES: RefCell<Vec<Box<Glyph>>> = const { RefCell::new(Vec::new()) };
}

/// The set with these glyphs: one a font holds already, or a new one.
fn shared(mut glyphs: Vec<(u32, Glyph)>) -> Arc<Set> {
    glyphs.sort_unstable_by_key(|&(scalar, _)| scalar);
    let set = Set { scalars: glyphs.iter().map(|&(s, _)| s).collect(), glyphs: glyphs.iter().map(|&(_, g)| g).collect() };
    let mut hasher = DefaultHasher::new();
    set.hash(&mut hasher);
    let mut sets = SETS.lock().unwrap_or_else(|e| e.into_inner());
    let same = sets.entry(hasher.finish()).or_default();
    same.retain(|w| w.strong_count() > 0);
    if let Some(kept) = same.iter().filter_map(Weak::upgrade).find(|kept| **kept == set) {
        return kept;
    }
    let set = Arc::new(set);
    same.push(Arc::downgrade(&set));
    set
}

/// The font whose own table is at `table` has loaded: the glyphs its file
/// brought become its set. Glyphs added after this are its own.
pub fn seal(table: usize) {
    let mut fonts = FONTS.write().unwrap_or_else(|e| e.into_inner());
    let font = fonts.entry(table).or_default();
    if font.set.is_none() {
        font.seen = HashSet::new();
        font.set = Some(shared(std::mem::take(&mut font.loading)));
    }
}

/// Adds the glyph for `scalar` to the font whose own table is at `table`.
/// Returns false, keeping the first, when the font already has one.
pub fn insert(table: usize, scalar: u32, glyph: Glyph) -> bool {
    let mut fonts = FONTS.write().unwrap_or_else(|e| e.into_inner());
    let font = fonts.entry(table).or_default();
    match &font.set {
        None => {
            let first = font.seen.insert(scalar);
            if first {
                font.loading.push((scalar, glyph));
            }
            first
        }
        Some(set) if set.get(scalar).is_some() || font.added.contains_key(&scalar) => false,
        Some(_) => {
            font.added.insert(scalar, Box::new(glyph));
            true
        }
    }
}

/// The glyph for a code point above 0xFF, or null when the font has none. The
/// pointer stays valid until the font is cleared or forgotten, or, for a
/// temporary glyph, until its draw call ends.
pub fn get(table: usize, scalar: u32) -> *const Glyph {
    if (TEMPORARY..TEMPORARY_END).contains(&scalar) {
        let found = TEMPORARIES.with(|t| t.borrow().get((scalar - TEMPORARY) as usize).map(|g| &**g as *const Glyph));
        if let Some(found) = found {
            return found;
        }
    }
    loop {
        let fonts = FONTS.read().unwrap_or_else(|e| e.into_inner());
        let Some(font) = fonts.get(&table) else { return std::ptr::null() };
        let Some(set) = &font.set else {
            // A font whose load `fonts` did not see the end of: seal it now.
            drop(fonts);
            seal(table);
            continue;
        };
        return match set.get(scalar).or_else(|| font.added.get(&scalar).map(|g| &**g)) {
            Some(g) => g as *const Glyph,
            None => std::ptr::null(),
        };
    }
}

/// The font at `table` keeps none of the glyphs its file brought (the system
/// font look draws them all); glyphs added after this are its own.
pub fn clear(table: usize) {
    let mut fonts = FONTS.write().unwrap_or_else(|e| e.into_inner());
    fonts.insert(table, Font { set: Some(shared(Vec::new())), ..Default::default() });
}

/// The font whose own table is at `table` is being destroyed.
pub fn forget(table: usize) {
    FONTS.write().unwrap_or_else(|e| e.into_inner()).remove(&table);
}

/// For the log: how many sets the fonts hold, and how many glyphs are in them.
pub fn shared_sets() -> (usize, usize) {
    let fonts = FONTS.read().unwrap_or_else(|e| e.into_inner());
    let mut seen = HashSet::new();
    let mut glyphs = 0;
    for set in fonts.values().filter_map(|f| f.set.as_ref()) {
        if !set.scalars.is_empty() && seen.insert(Arc::as_ptr(set)) {
            glyphs += set.scalars.len();
        }
    }
    (seen.len(), glyphs)
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
    fn fonts_that_bring_the_same_glyphs_share_them() {
        let (a, b, c) = (0x4000_0120, 0x5000_0120, 0x6000_0120);
        // The same glyphs in a different order, and a font with one more.
        for (font, order) in [(a, [0x4e2d, 0x6587]), (b, [0x6587, 0x4e2d]), (c, [0x4e2d, 0x6587])] {
            for scalar in order {
                insert(font, scalar, glyph(scalar as u16));
            }
        }
        insert(c, 0x8bd5, glyph(9));
        for font in [a, b, c] {
            seal(font);
        }
        assert_eq!(get(a, 0x4e2d), get(b, 0x4e2d));
        assert_ne!(get(a, 0x4e2d), get(c, 0x4e2d));
        assert_eq!(unsafe { *get(c, 0x8bd5) }.x, 9);
        // A glyph added after sealing is the font's own, and the set stays.
        assert!(insert(a, 0x5b57, glyph(5)));
        assert!(!insert(a, 0x4e2d, glyph(6)));
        assert!(get(b, 0x5b57).is_null());
        assert_eq!(unsafe { *get(a, 0x5b57) }.x, 5);
        // Cleared, a font keeps only what it adds.
        clear(c);
        assert!(get(c, 0x8bd5).is_null());
        assert!(insert(c, 0x8bd5, glyph(7)));
        assert_eq!(unsafe { *get(c, 0x8bd5) }.x, 7);
        for font in [a, b, c] {
            forget(font);
        }
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
