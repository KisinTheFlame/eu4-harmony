//! Text that floats up and fades, drawn by a font's method at 0x159c600 (font
//! vtable +0x48). A trace never saw single player call it (docs/unicode-module.md).
//!
//! The layout pass walks the text (`std::string*` in `rsi`, index `edi`),
//! appends each byte to a word (`std::string` at `rbp+0x128`), looks the byte
//! up at 0x159c833 (glyph to `r12`), and ends the word at a glyph with no
//! height (0x159c8a6), where a word that would overflow the width moves to a
//! new line. The drawing pass walks the result (base `rax` rebuilt from `r8`
//! and `r9`, index `esi`) and looks the byte up at 0x159cec5 (glyph to `r13`).
//!
//! Both lookups take a whole character and move the index over it (the layout
//! pass also appends the rest of its bytes to the word); the word also ends
//! after a non-ASCII character a line may break after.
use super::{breaks, text_loops};
use crate::points::{Point, Resume};
use std::sync::atomic::AtomicUsize;

const LAYOUT_GLYPH: usize = 0x159c833;
const LAYOUT_FOUND: usize = 0x159c842;
const WORD_TEST: usize = 0x159c8a6;
const WORD_END: usize = 0x159c8b3;
const NEXT_BYTE: usize = 0x159c9a1;
const DRAW_GLYPH: usize = 0x159cec5;
const DRAW_FOUND: usize = 0x159ced4;

static LAYOUT_DONE: AtomicUsize = AtomicUsize::new(0);
static END: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicUsize = AtomicUsize::new(0);
static DRAW_DONE: AtomicUsize = AtomicUsize::new(0);
static LAYOUT_RESUME: [Resume; 1] = [Resume { rva: LAYOUT_FOUND, address: &LAYOUT_DONE }];
static WORD_RESUME: [Resume; 2] = [Resume { rva: WORD_END, address: &END }, Resume { rva: NEXT_BYTE, address: &NEXT }];
static DRAW_RESUME: [Resume; 1] = [Resume { rva: DRAW_FOUND, address: &DRAW_DONE }];

/// Replaces `movzx eax, byte [rdi+rax]; mov r12, [r15+rax*8+0x120];
/// test r12, r12`: the text is at `rax`, the index in `edi`, which moves over
/// the character's other bytes; the word is at the game's `rbp+0x128`; the
/// glyph goes to `r12`.
#[unsafe(naked)]
extern "C" fn layout_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rdx, [rax + rdi]",
            "mov rax, [rbp]",
            "lea r8, [rax + 0x128]",
            "lea r9, [rsp + 0x40]",
            "mov rcx, r15",
            call glyph,
            "mov r12, [rsp + 0x48]",
            "add edi, dword ptr [rsp + 0x40]",
            "dec edi",
        ),
        "test r12, r12",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::layout_char,
        done = sym LAYOUT_DONE,
    );
}

/// Replaces `cmp word [r12+6], 0; jne NEXT_BYTE`: the word also ends after a
/// character a line may break after. The text is the `std::string` at `rsi`;
/// the character ends at `rdi + 1`.
#[unsafe(naked)]
extern "C" fn word_stub() {
    std::arch::naked_asm!(
        "cmp word ptr [r12 + 6], 0",
        "je 3f",
        call_rust!(
            "mov rcx, rsi",
            "cmp qword ptr [rsi + 0x18], 0x10",
            "jb 2f",
            "mov rcx, qword ptr [rsi]",
            "2:",
            "mov edx, dword ptr [rsi + 0x10]",
            "lea r8d, [rdi + 1]",
            call after,
            "cmp byte ptr [rsp + 0x48], 0",
        ),
        "jne 3f",
        "jmp qword ptr [rip + {next}]",
        "3:",
        "jmp qword ptr [rip + {end}]",
        after = sym breaks::after_at,
        next = sym NEXT,
        end = sym END,
    );
}

/// Replaces `movzx eax, byte [rsi+rax]; mov r13, [r15+rax*8+0x120];
/// test r13, r13`: the text is at `rax`, the index in `esi`, which moves over
/// the character's other bytes; the glyph goes to `r13`.
#[unsafe(naked)]
extern "C" fn draw_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rdx, [rax + rsi]",
            "mov rcx, r15",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov r13, [rsp + 0x48]",
            "add esi, dword ptr [rsp + 0x40]",
            "dec esi",
        ),
        "test r13, r13",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym DRAW_DONE,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        // The layout pass appends each byte to its word, then tests the byte.
        Point::keep(0x159c6d8usize, "0f b6 1c 07", "floating text reads a byte"),
        Point::jump(LAYOUT_GLYPH, "0f b6 04 07 4d 8b a4 c7 20 01 00 00 4d 85 e4", "floating text layout glyph", layout_stub as *const () as usize, &LAYOUT_RESUME),
        Point::keep(LAYOUT_FOUND, "0f 84 f0 00 00 00", "floating text null glyph"),
        Point::jump(WORD_TEST, "66 41 83 7c 24 06 00 0f 85 ee 00 00 00", "floating text word end", word_stub as *const () as usize, &WORD_RESUME),
        Point::keep(WORD_END, "44 8b a5 98 03 00 00", "floating text word ends"),
        Point::keep(NEXT_BYTE, "ff c7 3b 7e 10 0f 8c 24 fd ff ff", "floating text next byte"),
        Point::jump(DRAW_GLYPH, "0f b6 04 06 4d 8b ac c7 20 01 00 00 4d 85 ed", "floating text draw glyph", draw_stub as *const () as usize, &DRAW_RESUME),
        Point::keep(DRAW_FOUND, "75 54", "floating text glyph found"),
        Point::keep(0x159cc76usize, "ff c6 3b 74 24 70 0f 8c 31 ff ff ff", "floating text draw next byte"),
    ]
}
