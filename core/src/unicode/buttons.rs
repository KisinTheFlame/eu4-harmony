//! Button labels: a font's method at 0x1596660 (font vtable +0x40) renders a
//! button's text into a texture of its own, in two passes.
//!
//! The measuring pass walks the text (a local `std::string`: inline at
//! `rbp-0x38`, or `r12` when its capacity `r15` is 16 or more; index `r14d`,
//! length `[rbp-0x28]`), appends each byte to a word (`std::string` at
//! `rsp+0x78`, whose data pointer the loop keeps in `rbx`), looks the byte up
//! at 0x1597074 (index in `r9`, table in `rcx`, glyph to `r11`) and ends the
//! word at a glyph with no height (0x15970df). The drawing pass walks the laid
//! out text (inline at `rbp-0x60` or `r13`, capacity `r12`, index `r15d`) and
//! looks the byte up at 0x15983ad (table in `rdx`, glyph to `r8` and
//! `[rbp+0x58]`).
//!
//! Both lookups take a whole character and move the index over it; the
//! measuring pass also appends the rest of its bytes to the word, and the word
//! ends after a non-ASCII character a line may break after.
use super::objects::font::OWN_GLYPHS;
use super::{breaks, text_loops};
use crate::points::{Point, Resume};
use std::sync::atomic::AtomicUsize;

const MEASURE_GLYPH: usize = 0x1597074;
const MEASURE_FOUND: usize = 0x1597080;
const WORD_TEST: usize = 0x15970df;
const WORD_END: usize = 0x15970eb;
const NEXT_BYTE: usize = 0x15974cb;
const DRAW_GLYPH: usize = 0x15983ad;
const DRAW_FOUND: usize = 0x15983bc;

static MEASURE_DONE: AtomicUsize = AtomicUsize::new(0);
static END: AtomicUsize = AtomicUsize::new(0);
static NEXT: AtomicUsize = AtomicUsize::new(0);
static DRAW_DONE: AtomicUsize = AtomicUsize::new(0);
static MEASURE_RESUME: [Resume; 1] = [Resume { rva: MEASURE_FOUND, address: &MEASURE_DONE }];
static WORD_RESUME: [Resume; 2] = [Resume { rva: WORD_END, address: &END }, Resume { rva: NEXT_BYTE, address: &NEXT }];
static DRAW_RESUME: [Resume; 1] = [Resume { rva: DRAW_FOUND, address: &DRAW_DONE }];

/// Replaces `movzx eax, byte [r9+rax]; mov r11, [rcx+rax*8]; test r11, r11`,
/// keeping `rax` (text) and `rcx` (table): the glyph goes to `r11`, the index
/// `r14d` moves over the character's other bytes and `r9` gets it. Appending
/// to the word (at the game's `rsp+0x78`) may move its bytes, whose pointer
/// the loop keeps in `rbx`.
#[unsafe(naked)]
extern "C" fn measure_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rdx, [rax + r9]",
            "lea r8, [rbp + 8 + 0x78]",
            "lea r9, [rsp + 0x40]",
            "mov rcx, rdi",
            call glyph,
            "mov r11, [rsp + 0x48]",
            "add r14d, dword ptr [rsp + 0x40]",
            "dec r14d",
            "mov r9d, r14d",
        ),
        "mov rbx, qword ptr [rsp + 0x78]",
        "test r11, r11",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::layout_char,
        done = sym MEASURE_DONE,
    );
}

/// Replaces `cmp word [r11+6], 0; jne NEXT_BYTE`: the word also ends after a
/// character a line may break after. The text is inline at the game's
/// `rbp-0x38`, or at `r12` when its capacity `r15` is 16 or more; its length
/// is at the game's `rbp-0x28`; the character ends at `r14 + 1`.
#[unsafe(naked)]
extern "C" fn word_stub() {
    std::arch::naked_asm!(
        "cmp word ptr [r11 + 6], 0",
        "je 3f",
        call_rust!(
            "mov rax, [rbp]",
            "lea rcx, [rax - 0x38]",
            "cmp r15, 0x10",
            "cmovae rcx, r12",
            "mov edx, dword ptr [rax - 0x28]",
            "lea r8d, [r14 + 1]",
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

/// Replaces `movzx eax, byte [rcx+rax]; mov r8, [rdx+rax*8];
/// mov [rbp+0x58], r8; test r8, r8`, keeping the text in `rax` and the table
/// in `rdx`: the glyph goes to `r8`, the index `r15d` moves over the
/// character's other bytes and `rcx` gets it.
#[unsafe(naked)]
extern "C" fn draw_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea r8, [rsp + 0x40]",
            "add rax, rcx",
            "lea rcx, [rdx - {own}]",
            "mov rdx, rax",
            call glyph,
            "mov r8, [rsp + 0x48]",
            "add r15d, dword ptr [rsp + 0x40]",
            "dec r15d",
        ),
        "mov ecx, r15d",
        "mov [rbp + 0x58], r8",
        "test r8, r8",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym DRAW_DONE,
        own = const OWN_GLYPHS,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        // The measuring pass copies each byte into the word, then reloads the
        // word's data pointer into rbx before the lookup.
        Point::keep(0x1596831usize, "48 8d 45 c8 49 83 ff 10 49 0f 43 c4 41 8b de", "button text measuring loop"),
        Point::keep(0x1597059usize, "48 8b 5c 24 78 48 8d 8f 20 01 00 00 48 8d 45 c8 49 83 ff 10 49 0f 43 c4 45 8b ce", "button text measuring lookup"),
        Point::jump(MEASURE_GLYPH, "41 0f b6 04 01 4c 8b 1c c1 4d 85 db", "button text measuring glyph", measure_stub as *const () as usize, &MEASURE_RESUME),
        Point::keep(MEASURE_FOUND, "0f 84 e2 01 00 00", "button text null glyph"),
        Point::jump(WORD_TEST, "66 41 83 7b 06 00 0f 85 e0 03 00 00", "button text word end", word_stub as *const () as usize, &WORD_RESUME),
        Point::keep(NEXT_BYTE, "41 ff c6 44 3b 75 d8 0f 8c 59 f3 ff ff", "button text next byte"),
        Point::keep(0x159838ausize, "48 8d 97 20 01 00 00 48 89 55 60 48 8d 45 a0 49 83 fc 10 49 0f 43 c5 41 8b cf", "button text drawing lookup"),
        Point::jump(DRAW_GLYPH, "0f b6 04 01 4c 8b 04 c2 4c 89 45 58 4d 85 c0", "button text drawing glyph", draw_stub as *const () as usize, &DRAW_RESUME),
        Point::keep(DRAW_FOUND, "0f 85 96 00 00 00", "button text glyph found"),
        Point::keep(0x1597e45usize, "41 ff c7 45 3b fe 0f 8c 1f ff ff ff", "button text drawing next byte"),
    ]
}
