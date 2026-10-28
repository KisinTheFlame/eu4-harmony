//! Names on the map. A name's mesh is built by the font method at 0x159da60
//! (a layout pass and a drawing pass, both a byte at a time), measured by the
//! font method at 0x159e510 (font vtable +0x78), and placed by 0xfd7200
//! (straight names) or 0xfd5b70 (country names along a curve). Before building,
//! both count the visible glyphs to size the mesh; country names also get
//! spaces between letters until they fill the territory (0xfd6671), and each
//! glyph is then bent along the curve at a position given by its index over
//! the name's length (0xfd3850).
//!
//! The measuring method also wraps a name that is too wide for its space, at
//! spaces only; a Chinese character a line may break after ends a word too.
//!
//! Every one of those loops must take a Chinese character as one character,
//! or the counts disagree with the mesh and the bytes of one character are
//! spread apart: each lookup takes a whole character and moves its index over
//! it, the spacing loop appends whole characters, and the curve's length is
//! counted in characters.
//!
//! A name of one character stands upright. The curve turns each glyph to its
//! direction at the glyph's place along it (`t`, 0 to 1), and a single glyph
//! takes `t` = 1, the curve's end, where the curve may hook back: 明 (Ming)
//! lay upside down. The game's own
//! rule for one- and two-letter names (0xfd3ef5, `ebx` = 1000) blends in
//! another direction instead, but reads it from a vector of doubles as floats
//! (0xfd45d4, 0xfd464b), so whatever it gives is arbitrary.
use super::objects::font::OWN_GLYPHS;
use super::{breaks, game_text, text_loops};
use crate::points::{Point, Resume};
use std::sync::atomic::AtomicUsize;

macro_rules! resume {
    ($static:ident, $array:ident, $rva:expr) => {
        static $static: AtomicUsize = AtomicUsize::new(0);
        static $array: [Resume; 1] = [Resume { rva: $rva, address: &$static }];
    };
}

resume!(LAYOUT_DONE, LAYOUT_RESUME, 0x159dc13);
resume!(DRAW_DONE, DRAW_RESUME, 0x159df9a);
resume!(MEASURE_DONE, MEASURE_RESUME, 0x159e775);
resume!(STRAIGHT_COUNT_DONE, STRAIGHT_COUNT_RESUME, 0xfd734d);
resume!(CURVED_COUNT_DONE, CURVED_COUNT_RESUME, 0xfd6bda);
resume!(SPACED_DONE, SPACED_RESUME, 0xfd6727);
resume!(LENGTH_DONE, LENGTH_RESUME, 0xfd3ef5);
resume!(BEND_DONE, BEND_RESUME, 0xfd3f57);
resume!(UPRIGHT_DONE, UPRIGHT_RESUME, 0xfd468e);
static WORD_END: AtomicUsize = AtomicUsize::new(0);
static IN_WORD: AtomicUsize = AtomicUsize::new(0);
static WRAP_RESUME: [Resume; 2] = [Resume { rva: 0x159e7f0, address: &WORD_END }, Resume { rva: 0x159e841, address: &IN_WORD }];
static SHORT: AtomicUsize = AtomicUsize::new(0);
static LONG: AtomicUsize = AtomicUsize::new(0);
static GUARD_RESUME: [Resume; 2] = [Resume { rva: 0xfd415f, address: &SHORT }, Resume { rva: 0xfd4144, address: &LONG }];

fn characters(text: &[u8]) -> Vec<&[u8]> {
    game_text::chars(text).map(|(_, r)| &text[r]).collect()
}

/// The spacing loop: `dst` gets each character of `src` with `spacer` after
/// every one but the last.
extern "C" fn space_letters(dst: usize, src: *const u8, spacer: *const u8) {
    crate::guarded("map name spacing", || unsafe {
        let spacer = game_text::std_string(spacer);
        let chars = characters(game_text::std_string(src));
        for (i, c) in chars.iter().enumerate() {
            text_loops::append(dst, c);
            if i + 1 < chars.len() && !spacer.is_empty() {
                text_loops::append(dst, spacer);
            }
        }
    })
}

/// Writes how many characters the `std::string` at `string` holds to `count`.
extern "C" fn store_character_count(string: *const u8, count: *mut usize) {
    crate::guarded("map name length", || unsafe { *count = characters(game_text::std_string(string)).len() })
}

/// Layout pass (0x159da60): `movzx eax, byte [r15+rax]; mov r11,
/// [r9+rax*8+0x120]; test r11, r11`, index `r14d` (copy in `r15`), the word
/// at the pass's rsp + 0x28.
#[unsafe(naked)]
extern "C" fn layout_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rdx, [rax + r15]",
            "lea r8, [rbp + 8 + 0x28]",
            "mov rcx, r9",
            "lea r9, [rsp + 0x40]",
            call glyph,
            "mov r11, [rsp + 0x48]",
            "add r14d, dword ptr [rsp + 0x40]",
            "dec r14d",
            "mov r15d, r14d",
        ),
        "test r11, r11",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::layout_char,
        done = sym LAYOUT_DONE,
    );
}

/// Drawing pass: `movzx eax, byte [r9+rax]; mov rdx, [r15+rax*8];
/// test rdx, rdx`, index `r11d` (copy in `r9`), font in `r8`.
#[unsafe(naked)]
extern "C" fn draw_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rdx, [rax + r9]",
            "mov rcx, r8",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov rdx, [rsp + 0x48]",
            "add r11d, dword ptr [rsp + 0x40]",
            "dec r11d",
            "mov r9d, r11d",
        ),
        "test rdx, rdx",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym DRAW_DONE,
    );
}

/// Measuring (0x159e510): `movzx eax, byte [rax+rdx]; mov r11,
/// [r13+rax*8+0x120]; movss xmm1, [r13+0x968]; test r11, r11`, index `edi`
/// (copy in `edx`), font in `r13`.
#[unsafe(naked)]
extern "C" fn measure_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "add rdx, rax",
            "mov rcx, r13",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov r11, [rsp + 0x48]",
            "add edi, dword ptr [rsp + 0x40]",
            "dec edi",
            "mov edx, edi",
        ),
        "movss xmm1, dword ptr [r13 + 0x968]",
        "test r11, r11",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym MEASURE_DONE,
    );
}

/// Measuring's wrap test (0x159e7d8): `movsx eax, word [r11+6]; ...;
/// ucomiss xmm0, xmm8; jp/jne`, where a glyph with no height (a space) ends a
/// word, after which the line may wrap. A non-ASCII character a line may
/// break after ends one too. The text is the `std::string` at `rbx`; the
/// character ends at `edi + 1`.
#[unsafe(naked)]
extern "C" fn wrap_stub() {
    std::arch::naked_asm!(
        "movsx eax, word ptr [r11 + 6]",
        "test eax, eax",
        "jz 2f",
        call_rust!(
            "mov rcx, rbx",
            "cmp qword ptr [rbx + 0x18], 16",
            "jb 3f",
            "mov rcx, [rbx]",
            "3:",
            "mov edx, dword ptr [rbx + 0x10]",
            "lea r8d, [rdi + 1]",
            call after,
            "cmp byte ptr [rsp + 0x48], 0",
        ),
        "jnz 2f",
        "jmp qword ptr [rip + {in_word}]",
        "2:",
        "jmp qword ptr [rip + {word_end}]",
        after = sym breaks::after_at,
        in_word = sym IN_WORD,
        word_end = sym WORD_END,
    );
}

/// Straight names' glyph count: `movzx eax, byte [rcx+rax]; mov rdx,
/// [r12+rax*8+0x120]; test rdx, rdx`, index `rcx`, font in `r12`.
#[unsafe(naked)]
extern "C" fn straight_count_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rdx, [rax + rcx]",
            "mov rcx, r12",
            "mov qword ptr [rsp + 0x40], 0",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov rdx, [rsp + 0x48]",
            "add rcx, qword ptr [rsp + 0x40]",
            "dec rcx",
        ),
        "test rdx, rdx",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym STRAIGHT_COUNT_DONE,
    );
}

/// Country names' glyph count: `movzx eax, byte [rax+rcx]; mov rdx,
/// [r14+rax*8]; test rdx, rdx`, index `rcx`, the font's table in `r14`.
#[unsafe(naked)]
extern "C" fn curved_count_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "lea rdx, [rax + rcx]",
            "lea rcx, [r14 - {own}]",
            "mov qword ptr [rsp + 0x40], 0",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov rdx, [rsp + 0x48]",
            "add rcx, qword ptr [rsp + 0x40]",
            "dec rcx",
        ),
        "test rdx, rdx",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym CURVED_COUNT_DONE,
        own = const OWN_GLYPHS,
    );
}

/// Replaces the spacing loop: the spaced text goes to the game frame's
/// `[rbp+0x30]`, from the unspaced `[rbp+0x90]` and the spacer `[rbp+0x70]`;
/// `rbx` ends as the loop left it (the last index, `r12`) and `rdi` holds the
/// spacer's capacity.
#[unsafe(naked)]
extern "C" fn spacing_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rax, [rbp]",
            "lea rcx, [rax + 0x30]",
            "lea rdx, [rax + 0x90]",
            "lea r8, [rax + 0x70]",
            call space,
        ),
        "xor ebx, ebx",
        "test r12, r12",
        "cmovg rbx, r12",
        "mov rdi, qword ptr [rbp + 0x88]",
        "jmp qword ptr [rip + {done}]",
        space = sym space_letters,
        done = sym SPACED_DONE,
    );
}

/// Replaces `mov rcx, [rdi+0x10]; mov [rbp+0x168], rcx`: the curve's length
/// in characters goes to the game frame's `[rbp+0x168]`; `rcx` keeps the byte
/// length, which still ends the loop.
#[unsafe(naked)]
extern "C" fn length_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdi",
            "mov rdx, [rbp]",
            "add rdx, 0x168",
            call count,
        ),
        "mov rcx, qword ptr [rdi + 0x10]",
        "jmp qword ptr [rip + {done}]",
        count = sym store_character_count,
        done = sym LENGTH_DONE,
    );
}

/// Replaces `cmp dword [rdi+0x10], 1; jle`: positions along the curve need
/// two characters or more.
#[unsafe(naked)]
extern "C" fn guard_stub() {
    std::arch::naked_asm!(
        "cmp qword ptr [rbp + 0x168], 1",
        "jle 2f",
        "jmp qword ptr [rip + {long}]",
        "2:",
        "jmp qword ptr [rip + {short}]",
        long = sym LONG,
        short = sym SHORT,
    );
}

/// Bending: `movzx eax, byte [rcx+rax]; mov [rbp+0x808], al;
/// movss xmm12, [rdx+0x848]; mov r14, [rdx+rax*8]`, index `rcx` (kept at
/// `[rbp+0x118]`), the font's table in `rdx`.
#[unsafe(naked)]
extern "C" fn bend_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov r8, rdx",
            "lea rdx, [rax + rcx]",
            "lea rcx, [r8 - {own}]",
            "mov qword ptr [rsp + 0x40], 0",
            "lea r8, [rsp + 0x40]",
            call glyph,
            "mov r14, [rsp + 0x48]",
            "movzx eax, byte ptr [rax + rcx]",
            "add rcx, qword ptr [rsp + 0x40]",
            "dec rcx",
        ),
        "mov [rbp + 0x118], rcx",
        "mov byte ptr [rbp + 0x808], al",
        "movss xmm12, dword ptr [rdx + 0x848]",
        "jmp qword ptr [rip + {done}]",
        glyph = sym text_loops::char_glyph,
        done = sym BEND_DONE,
        own = const OWN_GLYPHS,
    );
}

/// Replaces `lea rdx, [rbp]; lea rcx, [rbp+0x320]` after the glyph's
/// direction, two floats at the game frame's `[rbp]`, is normalized (by
/// 0x8b3080): a name of one character (`[rbp+0x168]`, see [`length_stub`])
/// gets (1, 0), so its glyph stands upright.
#[unsafe(naked)]
extern "C" fn upright_stub() {
    std::arch::naked_asm!(
        "cmp qword ptr [rbp + 0x168], 1",
        "jne 2f",
        "mov dword ptr [rbp], 0x3f800000",
        "mov dword ptr [rbp + 4], 0",
        "2:",
        "lea rdx, [rbp]",
        "lea rcx, [rbp + 0x320]",
        "jmp qword ptr [rip + {done}]",
        done = sym UPRIGHT_DONE,
    );
}

pub fn points() -> Vec<Point> {
    let stub = |f: extern "C" fn()| f as *const () as usize;
    vec![
        Point::keep(0x159db79usize, "45 0f b6 04 07", "map text copies a byte"),
        Point::jump(0x159dc03usize, "41 0f b6 04 07 4d 8b 9c c1 20 01 00 00 4d 85 db", "map text layout glyph", stub(layout_stub), &LAYOUT_RESUME),
        Point::keep(0x159dc13usize, "0f 84 c9 01 00 00", "map text null glyph"),
        Point::keep(0x159deadusize, "41 ff c6 44 3b 73 10 0f 8c 9b fc ff ff", "map text layout next byte"),
        Point::jump(0x159df8eusize, "41 0f b6 04 01 49 8b 14 c7 48 85 d2", "map text draw glyph", stub(draw_stub), &DRAW_RESUME),
        Point::keep(0x159df9ausize, "75 73", "map text glyph found"),
        Point::keep(0x159e489usize, "41 ff c3 45 3b dc 0f 8c db fa ff ff", "map text draw next byte"),
        Point::keep(0x159e61dusize, "8b d7", "wrapped text measuring copies the index"),
        Point::jump(0x159e75dusize, "0f b6 04 10 4d 8b 9c c5 20 01 00 00 f3 41 0f 10 8d 68 09 00 00 4d 85 db", "wrapped text measuring glyph", stub(measure_stub), &MEASURE_RESUME),
        Point::keep(0x159e775usize, "0f 84 19 01 00 00", "wrapped text measuring null glyph"),
        Point::jump(
            0x159e7d8usize,
            "41 0f bf 43 06 66 0f 6e c0 0f 5b c0 f3 0f 59 c1 41 0f 2e c0 7a 53 75 51",
            "wrapped text measuring word end",
            stub(wrap_stub),
            &WRAP_RESUME,
        ),
        Point::keep(0x159e7f0usize, "66 0f 6e 85 78 10 00 00", "wrapped text measuring wraps at a word end"),
        Point::keep(0x159e841usize, "48 8b 85 90 10 00 00", "wrapped text measuring inside a word"),
        // The index ends the loop as the stubs left it: the next character.
        Point::keep(0x159e8f1usize, "ff c7 44 8b 4b 10 41 3b f9", "wrapped text measuring next byte"),
        Point::jump(0xfd733eusize, "0f b6 04 01 49 8b 94 c4 20 01 00 00 48 85 d2", "straight map name glyph count", stub(straight_count_stub), &STRAIGHT_COUNT_RESUME),
        Point::keep(0xfd734dusize, "74 09", "straight map name count skip"),
        Point::keep(0xfd7358usize, "48 ff c1 49 3b c8 7c d0", "straight map name count next byte"),
        Point::jump(0xfd6bcfusize, "0f b6 04 08 49 8b 14 c6 48 85 d2", "country name glyph count", stub(curved_count_stub), &CURVED_COUNT_RESUME),
        Point::keep(0xfd6bdausize, "74 0a", "country name count skip"),
        Point::keep(0xfd6be6usize, "48 ff c1 49 3b c8 7c d2", "country name count next byte"),
        Point::keep(0xfd664fusize, "48 8b bd 88 00 00 00 c6 45 01 00 4c 89 7d 30 4c 89 7d 40 48 c7 45 48 0f 00 00 00 c6 45 30 00 49 8b df", "country name spacing setup"),
        Point::jump(0xfd6671usize, "4d 85 e4 7e 6e", "country name letter spacing", stub(spacing_stub), &SPACED_RESUME),
        Point::keep(0xfd6727usize, "48 8b 85 40 02 00 00", "country name spacing done"),
        Point::jump(0xfd3eeausize, "48 8b 4f 10 48 89 8d 68 01 00 00", "map name curve length", stub(length_stub), &LENGTH_RESUME),
        Point::keep(0xfd3ef5usize, "8d 41 fe", "map name curve short names"),
        Point::jump(0xfd3f40usize, "0f b6 04 01 88 85 08 08 00 00 f3 44 0f 10 a2 48 08 00 00 4c 8b 34 c2", "map name curve glyph", stub(bend_stub), &BEND_RESUME),
        Point::keep(0xfd3f57usize, "4d 85 f6 0f 84 64 14 00 00", "map name curve glyph found"),
        Point::jump(0xfd413eusize, "83 7f 10 01 7e 1b", "map name curve position guard", stub(guard_stub), &GUARD_RESUME),
        Point::keep(0xfd4144usize, "66 0f 6e f6 0f 5b f6 48 8b 85 68 01 00 00", "map name curve position"),
        Point::keep(0xfd415fusize, "f2 0f 10 85 30 01 00 00", "map name curve after position"),
        Point::keep(0xfd467eusize, "e8 fd e9 8d ff", "map name curve normalizes the glyph direction"),
        Point::jump(0xfd4683usize, "48 8d 55 00 48 8d 8d 20 03 00 00", "map name curve upright single character", stub(upright_stub), &UPRIGHT_RESUME),
        Point::keep(0xfd468eusize, "e8 bd 5b 0f ff", "map name curve glyph normal"),
        Point::keep(0xfd53a9usize, "48 8b 8d 18 01 00 00", "map name curve reloads its index"),
        Point::keep(0xfd53c4usize, "ff c6 89 b5 e8 07 00 00 48 ff c1 48 89 8d 18 01 00 00", "map name curve next glyph"),
    ]
}

#[cfg(test)]
mod tests {
    use super::characters;

    #[test]
    fn characters_keep_utf8_sequences_and_single_bytes_whole() {
        let text = "明 \u{20bb7}".as_bytes().iter().copied().chain([0xe9]).collect::<Vec<u8>>();
        let chars: Vec<&[u8]> = characters(&text);
        assert_eq!(chars.len(), 4);
        assert_eq!(chars[0], "明".as_bytes());
        assert_eq!(chars[2].len(), 4);
        assert_eq!(chars[3], [0xe9]);
    }
}
