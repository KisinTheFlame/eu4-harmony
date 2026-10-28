//! A font's life as the Unicode module follows it, in one place: while the
//! font loads, its file's glyphs above 0xFF go to the glyph table
//! (`font_load`) and its page is caught as the game decodes it (`fill`); once
//! it has loaded, those glyphs become a shared set (`glyphs::seal`) and its
//! page is filled, after which a font drawn wholly from the system font keeps
//! none of them; when the game destroys the font, both forget it.
use super::{fill, glyphs};
use crate::{hooks::HookFrame, objects::text::font::OWN_GLYPHS, points::Point};

/// The font method that loads its page, then parses its `.fnt` (font).
const LOAD: usize = 0x15953c0;
/// Frees a font's own glyph table as the font goes (the table).
const DESTROY_TABLE: usize = 0x1594360;

extern "C" fn load_before(frame: *mut HookFrame) {
    crate::guarded("font load", || fill::loading(unsafe { (*frame).args[0] }))
}

extern "C" fn load_after(frame: *mut HookFrame) {
    crate::guarded("font loaded", || {
        let font = unsafe { (*frame).args[0] };
        // The font's file has been parsed: its glyphs above 0xFF are all in.
        glyphs::seal(font + OWN_GLYPHS);
        if fill::loaded(font) {
            glyphs::clear(font + OWN_GLYPHS);
        }
    })
}

extern "C" fn destroy_before(frame: *mut HookFrame) {
    crate::guarded("font destroy", || {
        let table = unsafe { (*frame).args[0] };
        glyphs::forget(table);
        fill::forget(table - OWN_GLYPHS);
    })
}

pub fn points() -> Vec<Point> {
    vec![
        Point::observe(LOAD, "48 89 5c 24 08 57 48 81 ec 80 00 00 00 48 8b f9", "font load", 1, Some(load_before), Some(load_after)),
        Point::observe(DESTROY_TABLE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48", "font table destructor", 1, Some(destroy_before), None),
    ]
}
