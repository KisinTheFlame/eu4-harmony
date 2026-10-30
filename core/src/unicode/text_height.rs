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
use super::{breaks, text_loops};
use crate::points::Point;
use crate::resume;

const GLYPH: usize = 0x159b687;
const FOUND: usize = 0x159b696;
const SPACE_TEST: usize = 0x159b73d;
const NEXT_BYTE: usize = 0x159b752;

resume!(GLYPH_RESUME: GLYPH_DONE = FOUND);
resume!(SPACE_RESUME: NEXT = NEXT_BYTE);

/// Replaces `movzx eax, byte [rdi+rax]; mov rcx, [r14+rax*8+0x120];
/// test rcx, rcx`: the text is at `rax`, the index in `edi`, which moves over
/// the character's other bytes; the glyph goes to `rcx`.
#[unsafe(naked)]
extern "C" fn glyph_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, r14",
            "lea rdx, [rax + rdi]",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov rcx, [rsp + 0x48]",
            "add edi, dword ptr [rsp + 0x40]",
            "dec edi",
        ),
        "test rcx, rcx",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym GLYPH_DONE,
    );
}

/// Replaces `mov rax, rbx; cmp r11, 0x10; jb +3; mov rax, [rbx];
/// cmp byte [rdi+rax], 0x20; jne +3; xorps xmm6, xmm6`: a line may also
/// break after the character ending at `rdi + 1`.
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
        call_rust!(
            "mov rcx, rax",
            "mov edx, dword ptr [rbx + 0x10]",
            "lea r8d, [rdi + 1]",
            call after,
            "cmp byte ptr [rsp + 0x48], 0",
        ),
        "je 4f",
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
        Point::jump(
            SPACE_TEST,
            "48 8b c3 49 83 fb 10 72 03 48 8b 03 80 3c 07 20 75 03 0f 57 f6",
            "text height space",
            space_stub as *const () as usize,
            &SPACE_RESUME,
        ),
        Point::keep(NEXT_BYTE, "44 8b 53 10 ff c7 41 3b fa 0f 8c cf fd ff ff", "text height next byte"),
    ]
}
