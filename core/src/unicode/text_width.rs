//! The width of text, for edit boxes and text widgets: a font's method at
//! 0x159b7c0 (font vtable +0x60), (font, text, byte length, formatting),
//! returning the widest line. It walks the text (base `rbp`, index `rbx`, with
//! `edi` kept equal) and looks each byte up at 0x159b91a: the byte to `edx`,
//! the font's table to `rcx`, the glyph to `r11`.
//!
//! The lookup takes a whole character and moves both indexes over it, leaving
//! the lead byte in `edx` for the kerning call and the newline test after it.
use super::text_loops;
use crate::objects::text::font::OWN_GLYPHS;
use crate::points::Point;
use crate::resume;

const GLYPH: usize = 0x159b91a;
const FOUND: usize = 0x159b92c;

resume!(RESUME: DONE = FOUND);

/// Replaces `movzx edx, byte [rbx+rbp]; lea rcx, [r15+0x120];
/// mov r11, [rcx+rdx*8]; test r11, r11`: the text is at the game's `rbp`, the
/// index in `rbx` and `edi`, which move over the character's other bytes;
/// the glyph goes to `r11` and the character's first byte to `edx`.
#[unsafe(naked)]
extern "C" fn glyph_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rax, [rbp]",
            "lea rdx, [rax + rbx]",
            "mov rcx, r15",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov r11, [rsp + 0x48]",
            "mov rdx, [rbp]",
            "movzx edx, byte ptr [rbx + rdx]",
            "add rbx, qword ptr [rsp + 0x40]",
            "dec rbx",
            "add edi, dword ptr [rsp + 0x40]",
            "dec edi",
        ),
        "lea rcx, [r15 + {own}]",
        "test r11, r11",
        "jmp qword ptr [rip + {done}]",
        own = const OWN_GLYPHS,
        glyph = sym text_loops::char_glyph,
        done = sym DONE,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(GLYPH, "0f b6 14 2b 49 8d 8f 20 01 00 00 4c 8b 1c d1 4d 85 db", "text width glyph", glyph_stub as *const () as usize, &RESUME)
            .uses(&[OWN_GLYPHS]),
        Point::keep(FOUND, "74 51", "text width null glyph"),
        Point::keep(0x159b999usize, "ff c7 48 ff c3", "text width next byte"),
    ]
}
