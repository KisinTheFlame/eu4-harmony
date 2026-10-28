//! The game's main text drawing, function 0x15988e0 (font, `std::string*`,
//! …): most text boxes go through it. It runs twice over the text.
//!
//! The layout pass walks the text byte by byte (index `edi`, text at
//! `[rbp-8]`, length `[rsp+0x78]`), copying each byte into a 256-byte word
//! buffer (index `esi`, base `r11`), and looks the byte up in the font's own
//! table. A glyph with no height ends the word; a word that does not fit
//! moves to a new line. The result, with the line breaks it chose, goes to a
//! static buffer at 0x242bf90.
//!
//! The drawing pass walks that buffer (index `r15d`, base `r9`, length
//! `[rsp+0x40]`) and emits six vertices per glyph.
//!
//! Three jumps make both passes read characters: each looks the glyph up by
//! character and moves the index over the character's whole sequence, and
//! the layout pass ends a word after a non-ASCII character that a line may
//! break after (`breaks`), as it does after a space.
use crate::resume;
use super::{breaks, text_loops};
use crate::points::Point;

const LAYOUT_GLYPH: usize = 0x1599728;
const LAYOUT_GLYPH_DONE: usize = 0x1599734;
const LAYOUT_WORD_END: usize = 0x15997a9;
/// The path after a space: the word ends.
const LAYOUT_SPACE: usize = 0x15997b4;
/// The path after a visible glyph: the word goes on unless the line is full.
const LAYOUT_VISIBLE: usize = 0x15998c7;
const DRAW_GLYPH: usize = 0x159a796;
const DRAW_GLYPH_DONE: usize = 0x159a7ac;

resume!(LAYOUT_RESUME: LAYOUT_DONE = LAYOUT_GLYPH_DONE);
resume!(WORD_END_RESUME: SPACE = LAYOUT_SPACE, VISIBLE = LAYOUT_VISIBLE);
resume!(DRAW_RESUME: DRAW_DONE = DRAW_GLYPH_DONE);

/// Replaces `movzx eax, byte [r9]; mov rcx, [r14+rax*8+0x120]`: the text is
/// at the game's `[rbp-8]`, its length at the game's `[rsp+0x78]`, the index
/// in `edi`, the 256-byte word buffer at `r11` with its index in `esi`. Both
/// indexes move over the character's other bytes; the glyph goes to `rcx`.
#[unsafe(naked)]
extern "C" fn layout_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rax, [rbp]",
            "mov rdx, qword ptr [rax - 8]",
            "mov r9d, dword ptr [rbp + 8 + 0x78]",
            "mov rcx, r14",
            "mov r8d, edi",
            "mov [rsp + 0x20], r11",
            "mov dword ptr [rsp + 0x28], esi",
            "mov dword ptr [rsp + 0x30], 0x100",
            "lea rax, [rsp + 0x40]",
            "mov [rsp + 0x38], rax",
            call glyph,
            "mov rcx, [rsp + 0x48]",
            "add edi, dword ptr [rsp + 0x40]",
            "dec edi",
            "add esi, dword ptr [rsp + 0x40]",
            "dec esi",
        ),
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::layout_char_in,
        done = sym LAYOUT_DONE,
    );
}

/// Replaces `cmp word [rcx+6], 0; jne LAYOUT_VISIBLE`: the word also ends
/// after a character a line may break after, which ends at `edi + 1`.
#[unsafe(naked)]
extern "C" fn word_end_stub() {
    std::arch::naked_asm!(
        "cmp word ptr [rcx + 6], 0",
        "je 2f",
        call_rust!(
            "mov rax, [rbp]",
            "mov rcx, qword ptr [rax - 8]",
            "mov edx, dword ptr [rbp + 8 + 0x78]",
            "lea r8d, [rdi + 1]",
            call after,
            "cmp byte ptr [rsp + 0x48], 0",
        ),
        "jne 2f",
        "jmp qword ptr [rip + {visible}]",
        "2:",
        "jmp qword ptr [rip + {space}]",
        after = sym breaks::after_at,
        visible = sym VISIBLE,
        space = sym SPACE,
    );
}

/// Replaces `movzx r8d, byte [rcx+r9]; movss xmm3, [r14+0x968];
/// mov rdx, [r14+r8*8+0x120]`: the laid-out text is at `r9`, its length at
/// the game's `[rsp+0x40]`, the index in `rcx` and `r15d`, which moves over
/// the character's other bytes.
#[unsafe(naked)]
extern "C" fn draw_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov r8, rcx",
            "mov rdx, r9",
            "mov r9d, dword ptr [rbp + 8 + 0x40]",
            "mov rcx, r14",
            "lea rax, [rsp + 0x40]",
            "mov [rsp + 0x20], rax",
            call glyph,
            "mov rdx, [rsp + 0x48]",
            "add r15d, dword ptr [rsp + 0x40]",
            "dec r15d",
        ),
        "movzx r8d, byte ptr [rcx + r9]",
        "movss xmm3, dword ptr [r14 + {scale}]",
        "jmp qword ptr [rip + {done}]",
        scale = const crate::objects::text::font::SCALE,
        glyph = sym text_loops::glyph_in,
        done = sym DRAW_DONE,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        // Where the layout pass keeps its text, length and indexes.
        Point::keep(
            0x15989c4usize,
            "4c 89 65 f8 49 83 7c 24 18 10 72 08 49 8b 3c 24 48 89 7d f8 b8 00 7d 00 00 44 3b f8 44 0f 4d f8 4c 89 7c 24",
            "text layout source and length",
        ),
        Point::keep(0x159959cusize, "4c 8b 44 24 78 45 85 c0", "text layout length"),
        Point::keep(
            0x15995b0usize,
            "4c 63 cf 48 8b 55 f8 4c 03 ca 48 63 ce 41 0f b6 01 4c 8d 1d 08 a7 e9 00 42 88 04 19 ff c6",
            "text layout copies a byte to the word",
        ),
        Point::keep(0x1599a3dusize, "ff c7 3b 7c 24 78 4c 8b 44 24 78 0f 8c 62 fb ff ff", "text layout next byte"),
        Point::jump(LAYOUT_GLYPH, "41 0f b6 01 49 8b 8c c6 20 01 00 00", "text layout glyph", layout_stub as *const () as usize, &LAYOUT_RESUME),
        Point::keep(LAYOUT_GLYPH_DONE, "48 89 4d 00 48 85 c9 0f 84 3b 02 00 00", "text layout glyph found"),
        Point::jump(LAYOUT_WORD_END, "66 83 79 06 00 0f 85 13 01 00 00", "text layout word end", word_end_stub as *const () as usize, &WORD_END_RESUME),
        Point::keep(LAYOUT_SPACE, "8d 04 1b", "text layout after a space"),
        Point::keep(LAYOUT_VISIBLE, "80 3d c2 26 e9 00 00", "text layout after a glyph"),
        // Where the drawing pass keeps its length and index.
        Point::keep(0x159a200usize, "48 ff c0 41 80 3c 01 00 75 f6 48 89 44 24 40 44 8b fb", "text drawing length"),
        Point::keep(0x159a230usize, "49 63 cf 80 bd a0 23 00 00 00 0f 84 56 05 00 00", "text drawing index"),
        Point::keep(0x159b085usize, "4c 8d 0d 04 0f e9 00 48 8b 44 24 40 41 ff c7 44 3b f8 0f 8c 93 f1 ff ff", "text drawing next byte"),
        Point::jump(
            DRAW_GLYPH,
            "46 0f b6 04 09 f3 41 0f 10 9e 68 09 00 00 4b 8b 94 c6 20 01 00 00",
            "text drawing glyph",
            draw_stub as *const () as usize,
            &DRAW_RESUME,
        )
        .uses(&[crate::objects::text::font::SCALE, crate::objects::text::font::OWN_GLYPHS]),
        Point::keep(DRAW_GLYPH_DONE, "48 89 55 38", "text drawing glyph found"),
    ]
}
