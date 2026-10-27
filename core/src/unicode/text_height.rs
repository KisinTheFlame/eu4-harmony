//! The height of wrapped text, for scrollable text boxes: a font's method at
//! 0x159b470 (font vtable +0x58). It walks the text (`std::string*` in `rbx`,
//! index `edi`, length `r10d`, capacity `r11`), looks each byte up at
//! 0x159b687 (glyph to `rcx`), adds its advance to the line width (`xmm7`) and
//! the width since the last space (`xmm6`), and on overflow starts a new line
//! with what came after the last space. At 0x159b73d it notes a space.
//!
//! The lookup takes a whole character and moves the index over it; after a
//! non-ASCII character a line may break after, the width since the last
//! "space" starts over too, so Chinese wraps between characters.
use super::{breaks, glyphs};
use crate::points::{Point, Resume};
use std::sync::atomic::AtomicUsize;

const GLYPH: usize = 0x159b687;
const FOUND: usize = 0x159b696;
const SPACE_TEST: usize = 0x159b73d;
const NEXT_BYTE: usize = 0x159b752;

static GLYPH_DONE: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicUsize = AtomicUsize::new(0);
static GLYPH_RESUME: [Resume; 1] = [Resume { rva: FOUND, address: &GLYPH_DONE }];
static SPACE_RESUME: [Resume; 1] = [Resume { rva: NEXT_BYTE, address: &NEXT }];

/// Replaces `movzx eax, byte [rdi+rax]; mov rcx, [r14+rax*8+0x120];
/// test rcx, rcx`, keeping `r10` and `r11`.
#[unsafe(naked)]
extern "C" fn glyph_stub() {
    std::arch::naked_asm!(
        "push rbp",
        "mov rbp, rsp",
        "and rsp, -16",
        "sub rsp, 0x40",
        "mov [rsp + 0x30], r10",
        "mov [rsp + 0x38], r11",
        "mov rcx, r14",
        "lea rdx, [rax + rdi]",
        "lea r8, [rsp + 0x28]",
        "call {glyph}",
        "mov rcx, rax",
        "mov eax, dword ptr [rsp + 0x28]",
        "dec eax",
        "add edi, eax",
        "mov r10, [rsp + 0x30]",
        "mov r11, [rsp + 0x38]",
        "mov rsp, rbp",
        "pop rbp",
        "test rcx, rcx",
        "jmp qword ptr [rip + {done}]",
        glyph = sym glyphs::char_glyph,
        done = sym GLYPH_DONE,
    );
}

/// Replaces `mov rax, rbx; cmp r11, 0x10; jb +3; mov rax, [rbx];
/// cmp byte [rdi+rax], 0x20; jne +3; xorps xmm6, xmm6`, keeping `r11`.
#[unsafe(naked)]
extern "C" fn space_stub() {
    std::arch::naked_asm!(
        "mov rax, rbx",
        "cmp r11, 0x10",
        "jb 2f",
        "mov rax, qword ptr [rbx]",
        "2:",
        "cmp byte ptr [rdi + rax], 0x20",
        "je 3f",
        "push rbp",
        "mov rbp, rsp",
        "and rsp, -16",
        "sub rsp, 0x30",
        "mov [rsp + 0x20], r11",
        "mov rcx, rax",
        "mov edx, dword ptr [rbx + 0x10]",
        "lea r8d, [rdi + 1]",
        "call {after}",
        "mov r11, [rsp + 0x20]",
        "mov rsp, rbp",
        "pop rbp",
        "test al, al",
        "jz 4f",
        "3:",
        "xorps xmm6, xmm6",
        "4:",
        "jmp qword ptr [rip + {next}]",
        after = sym breaks::after_at,
        next = sym NEXT,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x159b677usize, "4c 8b 5b 18 48 8b c3 49 83 fb 10 72 03 48 8b 03", "text height lookup"),
        Point::jump(GLYPH, "0f b6 04 07 49 8b 8c c6 20 01 00 00 48 85 c9", "text height glyph", glyph_stub as *const () as usize, &GLYPH_RESUME),
        Point::keep(FOUND, "74 72", "text height null glyph"),
        Point::jump(SPACE_TEST, "48 8b c3 49 83 fb 10 72 03 48 8b 03 80 3c 07 20 75 03 0f 57 f6", "text height space", space_stub as *const () as usize, &SPACE_RESUME),
        Point::keep(NEXT_BYTE, "44 8b 53 10 ff c7 41 3b fa 0f 8c cf fd ff ff", "text height next byte"),
    ]
}
