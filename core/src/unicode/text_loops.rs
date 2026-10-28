//! The game's text loops and the glyphs they draw. The game walks text a
//! byte at a time in many places, each looking the byte up in the font's own
//! 256-entry table. Every such loop is patched (by the module named below) to
//! take a whole character instead: it looks the character up here and moves
//! its index over all of the character's bytes. A loop that is missed shows
//! as wrong glyphs, split characters or Chinese that never wraps, so a new
//! one goes into this list (docs/research/unicode-text.md has the details).
//!
//! | Loop | Function | Module |
//! |---|---|---|
//! | main text, layout and drawing | 0x15988e0 (font vtable +0x50) | `main_text` |
//! | buttons and instant text boxes | 0x1596660 (+0x40) | `buttons` |
//! | text floating up and fading | 0x159c600 (+0x48) | `floating_text` |
//! | text height, text width | 0x159b470 (+0x58), 0x159b7c0 (+0x60) | `text_height`, `text_width` |
//! | text fitted into a box, with "..." | 0x159ec30 (+0x70), 0x159f510 (+0x80) | `fitted_text` |
//! | map names: measure and wrap, mesh, glyph counts, letter spacing, curve | 0x159e510 (+0x78), 0x159da60, 0xfd7200, 0xfd5b70, 0xfd3850 | `map_names` |
//! | multi-line text widget: line fit and line breaks | 0x1537210, 0x1537480 | `edit_box` |
//! | edit boxes: caret, keys, cuts, forbidden characters | 0x15384d0 and others | `edit_box` |
//! | `.fnt` loading | 0x15953c0 | `font_load` |
//! | save names to ASCII, search folding | 0x1705180, 0x1704d70 | `save_names`, `search` |
//!
//! A glyph comes from, in order: the font's own table for a character
//! Windows-1252 has; the glyph table (`glyphs`) by code point, where fonts
//! made for the CJK plugin also keep `“ ” … —`; a system font (`fill`); the
//! font's own `?`. Null only where the font's own table is and nothing else
//! has the character (control characters), as the loops expect.
use super::glyphs::{self, Glyph};
use crate::objects::text::font::OWN_GLYPHS;
use super::{fill, game_text};

/// The glyph `font` draws for `c`.
pub fn glyph(font: usize, c: char) -> *const Glyph {
    let table = font + OWN_GLYPHS;
    let own = |byte: u8| unsafe { *(table as *const *const Glyph).add(byte as usize) };
    if let Some(byte) = game_text::single_byte(c as u32) {
        let glyph = own(byte);
        if !glyph.is_null() || byte < 0x80 {
            return glyph;
        }
    }
    let found = glyphs::get(table, c as u32);
    if !found.is_null() {
        return found;
    }
    match fill::glyph(font, c as u32) {
        filled if !filled.is_null() => filled,
        _ if game_text::single_byte(c as u32).is_some() => std::ptr::null(),
        _ => own(b'?'),
    }
}

/// The character starting at `text` (read no further than its NUL) and the
/// glyph `font` draws for it, with the character's length.
///
/// # Safety
/// `text` points at NUL-terminated bytes, or at least four readable ones.
pub unsafe fn glyph_at(font: usize, text: *const u8) -> (*const Glyph, usize) {
    let mut bytes = [0u8; 4];
    let mut n = 0;
    while n < 4 {
        bytes[n] = *text.add(n);
        n += 1;
        if bytes[n - 1] == 0 {
            break;
        }
    }
    let (c, len) = game_text::char_at(&bytes[..n]);
    (glyph(font, c), len.max(1))
}

/// [`glyph_at`] for the loops' stubs; the length goes to `*len`.
pub extern "C" fn char_glyph(font: usize, text: *const u8, len: *mut u32) -> *const Glyph {
    crate::guarded("text glyph", || {
        let (glyph, n) = unsafe { glyph_at(font, text) };
        unsafe { *len = n as u32 };
        glyph
    })
}

/// For loops that know where the text ends: the glyph for the character at
/// `text[index]`, `end` bytes long in all; its length goes to `*len`.
pub extern "C" fn glyph_in(font: usize, text: *const u8, index: u32, end: u32, len: *mut u32) -> *const Glyph {
    crate::guarded("text glyph", || {
        let all = unsafe { std::slice::from_raw_parts(text, end as usize) };
        let (c, n) = game_text::char_at(all.get(index as usize..).unwrap_or_default());
        unsafe { *len = n.max(1) as u32 };
        glyph(font, c)
    })
}

/// For layout passes that copy each byte into a word (`std::string*`) before
/// looking it up: the glyph for the character at `text`, with the rest of its
/// bytes appended to `word`; the length goes to `*len`.
pub extern "C" fn layout_char(font: usize, text: *const u8, word: usize, len: *mut u32) -> *const Glyph {
    crate::guarded("text layout glyph", || unsafe {
        let (glyph, n) = glyph_at(font, text);
        if n > 1 {
            crate::game_string::append(word, std::slice::from_raw_parts(text.add(1), n - 1));
        }
        *len = n as u32;
        glyph
    })
}

/// For layout passes that copy each byte into a fixed word buffer (`word`,
/// `capacity` bytes, the character's first byte already at `word_at - 1`):
/// [`glyph_in`], with the rest of the character's bytes copied in after it
/// while they fit.
pub extern "C" fn layout_char_in(font: usize, text: *const u8, index: u32, end: u32, word: *mut u8, word_at: u32, capacity: u32, len: *mut u32) -> *const Glyph {
    crate::guarded("text layout glyph", || {
        let glyph = glyph_in(font, text, index, end, len);
        let extra = unsafe { *len } as usize - 1;
        if extra > 0 && word_at as usize + extra <= capacity as usize {
            unsafe { std::ptr::copy_nonoverlapping(text.add(index as usize + 1), word.add(word_at as usize), extra) };
        }
        glyph
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(x: u16) -> Glyph {
        Glyph { x, width: 12, height: 14, advance: 12, ..Default::default() }
    }

    #[test]
    fn a_1252_character_the_own_table_lacks_comes_from_its_code_point() {
        // A font object: 256 own glyph pointers; only `a` and `?`.
        let (a, question) = (record(1), record(9));
        let mut font = vec![0usize; OWN_GLYPHS / 8 + 256];
        font[OWN_GLYPHS / 8 + b'a' as usize] = &a as *const Glyph as usize;
        font[OWN_GLYPHS / 8 + b'?' as usize] = &question as *const Glyph as usize;
        let font = font.as_ptr() as usize;
        // The font keeps … (1252 byte 0x85) at U+2026.
        assert!(glyphs::insert(font + OWN_GLYPHS, 0x2026, record(2)));
        assert_eq!(unsafe { *glyph(font, 'a') }.x, 1);
        assert_eq!(unsafe { *glyph(font, '…') }.x, 2);
        let text = b"\x85";
        assert_eq!(unsafe { *glyph_at(font, text.as_ptr()).0 }.x, 2);
        // A character no font has is `?`; control characters stay null, as
        // the game expects.
        assert_eq!(unsafe { *glyph(font, '中') }.x, 9);
        assert!(glyph(font, '\n').is_null());
        assert!(glyph(font, '\u{201c}').is_null());
        glyphs::forget(font + OWN_GLYPHS);
    }

    #[test]
    fn a_layout_word_gets_the_whole_character() {
        let a = record(1);
        let mut font = vec![0usize; OWN_GLYPHS / 8 + 256];
        font[OWN_GLYPHS / 8 + b'?' as usize] = &a as *const Glyph as usize;
        let font = font.as_ptr() as usize;
        let text = "x中".as_bytes();
        let mut word = [0u8; 4];
        word[0] = text[1];
        let mut len = 0;
        layout_char_in(font, text.as_ptr(), 1, text.len() as u32, word.as_mut_ptr(), 1, 4, &mut len);
        assert_eq!((len, &word[..3]), (3, "中".as_bytes()));
        // A word buffer that is full keeps what it has.
        let mut full = [0u8; 2];
        layout_char_in(font, text.as_ptr(), 1, text.len() as u32, full.as_mut_ptr(), 1, 2, &mut len);
        assert_eq!((len, full), (3, [0, 0]));
    }
}
