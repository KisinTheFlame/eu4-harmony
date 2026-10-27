//! The width of text, for edit boxes and text widgets: a font's method at
//! 0x159b7c0 (font vtable +0x60), (font, text, byte length, formatting),
//! returning the widest line. It walks the text (base `rbp`, index `rbx`, with
//! `edi` kept equal) and looks each byte up at 0x159b91a: the byte to `edx`,
//! the font's table to `rcx`, the glyph to `r11`.
//!
//! The lookup takes a whole character and moves both indexes over it, leaving
//! the lead byte in `edx` for the kerning call and the newline test after it.
use super::glyphs;
use crate::points::{Point, Resume};
use std::sync::atomic::AtomicUsize;

const GLYPH: usize = 0x159b91a;
const FOUND: usize = 0x159b92c;

static DONE: AtomicUsize = AtomicUsize::new(0);
static RESUME: [Resume; 1] = [Resume { rva: FOUND, address: &DONE }];

/// Replaces `movzx edx, byte [rbx+rbp]; lea rcx, [r15+0x120];
/// mov r11, [rcx+rdx*8]; test r11, r11`. No volatile register is live.
#[unsafe(naked)]
extern "C" fn glyph_stub() {
    std::arch::naked_asm!(
        "lea rax, [rbx + rbp]",
        "push rbp",
        "mov rbp, rsp",
        "and rsp, -16",
        "sub rsp, 0x30",
        "mov rcx, r15",
        "mov rdx, rax",
        "lea r8, [rsp + 0x28]",
        "call {glyph}",
        "mov r11, rax",
        "mov eax, dword ptr [rsp + 0x28]",
        "dec eax",
        "mov rsp, rbp",
        "pop rbp",
        "movzx edx, byte ptr [rbx + rbp]",
        "add rbx, rax",
        "add edi, eax",
        "lea rcx, [r15 + 0x120]",
        "test r11, r11",
        "jmp qword ptr [rip + {done}]",
        glyph = sym glyphs::char_glyph,
        done = sym DONE,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(GLYPH, "0f b6 14 2b 49 8d 8f 20 01 00 00 4c 8b 1c d1 4d 85 db", "text width glyph", glyph_stub as *const () as usize, &RESUME),
        Point::keep(FOUND, "74 51", "text width null glyph"),
        Point::keep(0x159b999usize, "ff c7 48 ff c3", "text width next byte"),
    ]
}
