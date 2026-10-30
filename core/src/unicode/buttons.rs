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
//!
//! Between the passes the texture is made, as many pixels across and down as
//! the text takes GUI units, and the window shows it stretched by the
//! interface scale. A font whose glyphs have more pixels than GUI units (the
//! system font look, `fill`) gets a texture with as many more, so that its
//! glyphs are drawn into it, and it onto the window, a texel to a pixel:
//! drawn into a smaller one, the game's point filter would drop rows and
//! columns of them. Everything else keeps the size in GUI units: the
//! projection, the layout and the size the game keeps for the texture.
use super::{breaks, text_loops};
use crate::objects::text::font::{OWN_GLYPHS, SCALE};
use crate::points::Point;
use crate::resume;

const MEASURE_GLYPH: usize = 0x1597074;
const MEASURE_FOUND: usize = 0x1597080;
const WORD_TEST: usize = 0x15970df;
const WORD_END: usize = 0x15970eb;
const NEXT_BYTE: usize = 0x15974cb;
const DRAW_GLYPH: usize = 0x15983ad;
const DRAW_FOUND: usize = 0x15983bc;
/// Whether the texture the button kept is the size wanted; making one.
const TEXTURE_KEPT: usize = 0x15978bf;
const TEXTURE_MAKE: usize = 0x15978fd;
/// u8: the game makes textures a power of two across and down.
const POWERS_OF_TWO: usize = 0x2453ec9;

resume!(MEASURE_RESUME: MEASURE_DONE = MEASURE_FOUND);
resume!(WORD_RESUME: END = WORD_END, NEXT = NEXT_BYTE);
resume!(DRAW_RESUME: DRAW_DONE = DRAW_FOUND);
resume!(KEPT_RESUME: REUSE = 0x159791b, REMAKE = 0x15978d1);
resume!(MAKE_RESUME: MAKE = 0x1597909);

/// The pixels a texture of `size` GUI units takes for text drawn from glyphs
/// of `scale` GUI units a pixel: as many pixels a GUI unit as the glyphs
/// have, and powers of two when the game makes them so.
fn texture_pixels(scale: f32, size: [i32; 2], powers_of_two: bool) -> [i32; 2] {
    if !(scale > 0.0 && scale < 1.0) {
        return size;
    }
    size.map(|v| {
        // The scale is 1 / density as an f32: the slack keeps 150.00001
        // pixels at 150.
        let pixels = (v as f64 / scale as f64 - 1e-3).ceil().max(1.0) as i32;
        if powers_of_two {
            (pixels as u32).next_power_of_two() as i32
        } else {
            pixels
        }
    })
}

/// The pixels the texture of `font`'s text of `width` × `height` GUI units
/// takes, as width | height << 32.
extern "C" fn texture_for(font: usize, width: i32, height: i32) -> u64 {
    crate::guarded("button text texture", || {
        let (scale, powers_of_two) = unsafe { (*((font + SCALE) as *const f32), *(crate::hooked_memory().at(POWERS_OF_TWO) as *const u8) != 0) };
        let [w, h] = texture_pixels(scale, [width, height], powers_of_two);
        w as u32 as u64 | (h as u32 as u64) << 32
    })
}

/// Whether the texture a button kept, `kept` pixels, is the one `font`'s
/// text of `wanted` GUI units takes.
unsafe extern "C" fn texture_fits(font: usize, wanted: *const [i32; 2], kept: *const [i32; 2]) -> bool {
    let ([width, height], kept) = unsafe { (*wanted, *kept) };
    let pixels = texture_for(font, width, height);
    kept == [pixels as u32 as i32, (pixels >> 32) as u32 as i32]
}

/// Replaces `mov eax, [rbp-0x68]; cmp [rsp+0x48], eax; jne REMAKE;
/// mov eax, [rbp-0x64]; cmp [rsp+0x4c], eax; je REUSE`: the size the texture
/// kept has (the game's `rsp+0x48`) against the pixels the wanted size
/// (`rbp-0x68`) takes. The font is in `rdi`.
#[unsafe(naked)]
extern "C" fn kept_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rax, [rbp]",
            "lea rdx, [rax - 0x68]",
            "lea r8, [rbp + 8 + 0x48]",
            "mov rcx, rdi",
            call fits,
            "cmp byte ptr [rsp + 0x48], 0",
        ),
        "jne 3f",
        "jmp qword ptr [rip + {remake}]",
        "3:",
        "jmp qword ptr [rip + {reuse}]",
        fits = sym texture_fits,
        remake = sym REMAKE,
        reuse = sym REUSE,
    );
}

/// Replaces `mov r9d, 0xb; mov edx, r10d; mov rcx, r12` before the renderer
/// makes the texture: its width `r10d` and height `r8d` become the pixels
/// they take. The font is in `rdi`.
#[unsafe(naked)]
extern "C" fn make_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdi",
            "mov edx, r10d",
            call pixels,
            "mov r10d, dword ptr [rsp + 0x48]",
            "mov r8d, dword ptr [rsp + 0x4c]",
        ),
        "mov r9d, 0xb",
        "mov edx, r10d",
        "mov rcx, r12",
        "jmp qword ptr [rip + {make}]",
        pixels = sym texture_for,
        make = sym MAKE,
    );
}

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
        // The texture: the font in rdi, as its context is read from it.
        Point::keep(0x1597872usize, "48 8b 47 48", "button text font"),
        Point::keep(0x1597890usize, "80 3d 32 c6 eb 00 00", "button text texture powers of two").uses(&[POWERS_OF_TWO]),
        Point::jump(
            TEXTURE_KEPT,
            "8b 45 98 39 44 24 48 75 09 8b 45 9c 39 44 24 4c 74 4a",
            "button text texture kept",
            kept_stub as *const () as usize,
            &KEPT_RESUME,
        ),
        Point::keep(0x15978d1usize, "49 8b ce ff 15 56 9a a3 00", "button text texture released"),
        Point::keep(0x159791busize, "48 8b 57 48", "button text texture reused"),
        Point::jump(TEXTURE_MAKE, "41 b9 0b 00 00 00 41 8b d2 49 8b cc", "button text texture made", make_stub as *const () as usize, &MAKE_RESUME),
        Point::keep(0x1597909usize, "ff d0 48 89 85 b0 21 00 00", "button text texture making"),
    ]
}

#[cfg(test)]
mod tests {
    use super::texture_pixels;

    #[test]
    fn denser_text_gets_a_texture_as_dense() {
        // Glyphs of 1.5 pixels a GUI unit.
        assert_eq!(texture_pixels(1.0 / 1.5, [100, 21], false), [150, 32]);
        assert_eq!(texture_pixels(1.0 / 1.5, [100, 21], true), [256, 32]);
        assert_eq!(texture_pixels(1.0 / 1.25, [64, 18], false), [80, 23]);
        // A font as it was made, or bigger than its pixels: as it was.
        assert_eq!(texture_pixels(1.0, [100, 21], false), [100, 21]);
        assert_eq!(texture_pixels(2.0, [100, 21], false), [100, 21]);
    }
}
