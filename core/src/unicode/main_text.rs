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
//! code point and moves the index over the character's whole UTF-8 sequence,
//! and the layout pass ends a word after a non-ASCII character that a line may
//! break after (`breaks`), as it does after a space.
use super::{breaks, glyphs, scalar};
use crate::points::{Point, Resume};
use std::sync::atomic::AtomicUsize;

const LAYOUT_GLYPH: usize = 0x1599728;
const LAYOUT_GLYPH_DONE: usize = 0x1599734;
const LAYOUT_WORD_END: usize = 0x15997a9;
/// The path after a space: the word ends.
const LAYOUT_SPACE: usize = 0x15997b4;
/// The path after a visible glyph: the word goes on unless the line is full.
const LAYOUT_VISIBLE: usize = 0x15998c7;
const DRAW_GLYPH: usize = 0x159a796;
const DRAW_GLYPH_DONE: usize = 0x159a7ac;

static LAYOUT_DONE: AtomicUsize = AtomicUsize::new(0);
static SPACE: AtomicUsize = AtomicUsize::new(0);
static VISIBLE: AtomicUsize = AtomicUsize::new(0);
static DRAW_DONE: AtomicUsize = AtomicUsize::new(0);
static LAYOUT_RESUME: [Resume; 1] = [Resume { rva: LAYOUT_GLYPH_DONE, address: &LAYOUT_DONE }];
static WORD_END_RESUME: [Resume; 2] = [Resume { rva: LAYOUT_SPACE, address: &SPACE }, Resume { rva: LAYOUT_VISIBLE, address: &VISIBLE }];
static DRAW_RESUME: [Resume; 1] = [Resume { rva: DRAW_GLYPH_DONE, address: &DRAW_DONE }];

/// The word buffer holds 256 bytes.
const WORD: usize = 0x100;

/// Layout pass: the glyph for the character at `text[index]`. Copies the rest
/// of its bytes into the word buffer after `word_at` and stores its length.
extern "C" fn layout_glyph(font: usize, text: *const u8, index: u32, end: u32, word: *mut u8, word_at: u32, len: *mut u32) -> *const glyphs::Glyph {
    crate::guarded("text layout", || {
        let (index, end, word_at) = (index as usize, end as usize, word_at as usize);
        let all = unsafe { std::slice::from_raw_parts(text, end) };
        let (code, n) = scalar::decode(&all[index..]);
        let n = n.max(1);
        let extra = n - 1;
        if extra > 0 && word_at + extra <= WORD {
            unsafe { std::ptr::copy_nonoverlapping(text.add(index + 1), word.add(word_at), extra) };
        }
        breaks::at(all, index, index + n);
        unsafe { *len = n as u32 };
        glyphs::for_scalar(font, code)
    })
}

extern "C" fn wide_break() -> bool {
    crate::guarded("text wrap", breaks::wide_break_after)
}

/// Drawing pass: the glyph for the character at `text[index]` and its length.
extern "C" fn draw_glyph(font: usize, text: *const u8, index: i64, end: u32, len: *mut u32) -> *const glyphs::Glyph {
    crate::guarded("text draw", || {
        let (code, n) = scalar::decode(unsafe { std::slice::from_raw_parts(text.add(index as usize), (end as usize).saturating_sub(index as usize)) });
        unsafe { *len = n.max(1) as u32 };
        glyphs::for_scalar(font, code)
    })
}

/// Replaces `movzx eax, byte [r9]; mov rcx, [r14+rax*8+0x120]`. Everything
/// but rax and rcx stays as it was, apart from the two indexes.
#[unsafe(naked)]
extern "C" fn layout_stub() {
    std::arch::naked_asm!(
        "sub rsp, 0xd0",
        "movdqu [rsp + 0x40], xmm0",
        "movdqu [rsp + 0x50], xmm1",
        "movdqu [rsp + 0x60], xmm2",
        "movdqu [rsp + 0x70], xmm3",
        "movdqu [rsp + 0x80], xmm4",
        "movdqu [rsp + 0x90], xmm5",
        "mov [rsp + 0xa0], rdx",
        "mov [rsp + 0xa8], r8",
        "mov [rsp + 0xb0], r9",
        "mov [rsp + 0xb8], r10",
        "mov [rsp + 0xc0], r11",
        "mov [rsp + 0x20], r11",
        "mov dword ptr [rsp + 0x28], esi",
        "lea rax, [rsp + 0x38]",
        "mov [rsp + 0x30], rax",
        "mov rcx, r14",
        "mov rdx, qword ptr [rbp - 8]",
        "mov r8d, edi",
        "mov r9d, dword ptr [rsp + 0x148]",
        "call {glyph}",
        "mov rcx, rax",
        "mov eax, dword ptr [rsp + 0x38]",
        "dec eax",
        "add edi, eax",
        "add esi, eax",
        "movdqu xmm0, [rsp + 0x40]",
        "movdqu xmm1, [rsp + 0x50]",
        "movdqu xmm2, [rsp + 0x60]",
        "movdqu xmm3, [rsp + 0x70]",
        "movdqu xmm4, [rsp + 0x80]",
        "movdqu xmm5, [rsp + 0x90]",
        "mov rdx, [rsp + 0xa0]",
        "mov r8, [rsp + 0xa8]",
        "mov r9, [rsp + 0xb0]",
        "mov r10, [rsp + 0xb8]",
        "mov r11, [rsp + 0xc0]",
        "lea rsp, [rsp + 0xd0]",
        "jmp qword ptr [rip + {done}]",
        glyph = sym layout_glyph,
        done = sym LAYOUT_DONE,
    );
}

/// Replaces `cmp word [rcx+6], 0; jne LAYOUT_VISIBLE`.
#[unsafe(naked)]
extern "C" fn word_end_stub() {
    std::arch::naked_asm!(
        "cmp word ptr [rcx + 6], 0",
        "je 2f",
        "sub rsp, 0xd0",
        "movdqu [rsp + 0x40], xmm0",
        "movdqu [rsp + 0x50], xmm1",
        "movdqu [rsp + 0x60], xmm2",
        "movdqu [rsp + 0x70], xmm3",
        "movdqu [rsp + 0x80], xmm4",
        "movdqu [rsp + 0x90], xmm5",
        "mov [rsp + 0x20], rax",
        "mov [rsp + 0x28], rcx",
        "mov [rsp + 0x30], rdx",
        "mov [rsp + 0xa0], r8",
        "mov [rsp + 0xa8], r9",
        "mov [rsp + 0xb0], r10",
        "mov [rsp + 0xb8], r11",
        "call {wide}",
        "test al, al",
        "movdqu xmm0, [rsp + 0x40]",
        "movdqu xmm1, [rsp + 0x50]",
        "movdqu xmm2, [rsp + 0x60]",
        "movdqu xmm3, [rsp + 0x70]",
        "movdqu xmm4, [rsp + 0x80]",
        "movdqu xmm5, [rsp + 0x90]",
        "mov rax, [rsp + 0x20]",
        "mov rcx, [rsp + 0x28]",
        "mov rdx, [rsp + 0x30]",
        "mov r8, [rsp + 0xa0]",
        "mov r9, [rsp + 0xa8]",
        "mov r10, [rsp + 0xb0]",
        "mov r11, [rsp + 0xb8]",
        "lea rsp, [rsp + 0xd0]",
        "jnz 2f",
        "jmp qword ptr [rip + {visible}]",
        "2:",
        "jmp qword ptr [rip + {space}]",
        wide = sym wide_break,
        visible = sym VISIBLE,
        space = sym SPACE,
    );
}

/// Replaces `movzx r8d, byte [rcx+r9]; movss xmm3, [r14+0x968];
/// mov rdx, [r14+r8*8+0x120]`, and moves `r15d` over the character.
#[unsafe(naked)]
extern "C" fn draw_stub() {
    std::arch::naked_asm!(
        "sub rsp, 0xd0",
        "movdqu [rsp + 0x40], xmm0",
        "movdqu [rsp + 0x50], xmm1",
        "movdqu [rsp + 0x60], xmm2",
        "movdqu [rsp + 0x80], xmm4",
        "movdqu [rsp + 0x90], xmm5",
        "mov [rsp + 0xa0], rax",
        "mov [rsp + 0xa8], rcx",
        "mov [rsp + 0xb0], r9",
        "mov [rsp + 0xb8], r10",
        "mov [rsp + 0xc0], r11",
        "lea rdx, [rsp + 0x38]",
        "mov [rsp + 0x20], rdx",
        "mov r8, rcx",
        "mov rdx, r9",
        "mov r9d, dword ptr [rsp + 0x110]",
        "mov rcx, r14",
        "call {glyph}",
        "mov rdx, rax",
        "mov eax, dword ptr [rsp + 0x38]",
        "dec eax",
        "add r15d, eax",
        "movdqu xmm0, [rsp + 0x40]",
        "movdqu xmm1, [rsp + 0x50]",
        "movdqu xmm2, [rsp + 0x60]",
        "movdqu xmm4, [rsp + 0x80]",
        "movdqu xmm5, [rsp + 0x90]",
        "mov rax, [rsp + 0xa0]",
        "mov rcx, [rsp + 0xa8]",
        "mov r9, [rsp + 0xb0]",
        "mov r10, [rsp + 0xb8]",
        "mov r11, [rsp + 0xc0]",
        "lea rsp, [rsp + 0xd0]",
        "movzx r8d, byte ptr [rcx + r9]",
        "movss xmm3, dword ptr [r14 + 0x968]",
        "jmp qword ptr [rip + {done}]",
        glyph = sym draw_glyph,
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
        ),
        Point::keep(DRAW_GLYPH_DONE, "48 89 55 38", "text drawing glyph found"),
    ]
}
