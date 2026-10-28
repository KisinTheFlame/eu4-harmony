//! Text fitted into a box: a font's methods at 0x159ec30 (font vtable +0x70)
//! and 0x159f510 (+0x80) measure text against a width and a height and cut
//! what does not fit, ending it with "..."; the second can also break it into
//! lines. Both walk the text (`std::string*`) a byte at a time, look the byte
//! up in the font's own table, and cut at the offset of a byte they visited.
//!
//! Here each lookup takes the whole character at the offset, so it measures
//! right, and the loop's step moves past the character's other bytes. The
//! offset stays on the character's first byte in between, so every cut falls
//! between characters.
//!
//! - 0x159ec30: index `edi` (and `rdx` at the lookup), text `rbx`, table
//!   `r14`; the glyph goes to `r15`; the step at 0x159f1db.
//! - 0x159f510: index `ebx` (`esi`, and `rdx` at the lookup), text `rdi`,
//!   table `r8`; the glyph goes to `r12`; the step at 0x159fde5.
//!
//! When 0x159f510 breaks text into lines (the country history window, for
//! one), it breaks at the last space, skipping it, or, with no space on the
//! line, at the character that overflowed, so Latin names in Chinese text
//! were split. It keeps the last space's offset in `r15d` and `r14d` and the
//! width after it in `xmm8` (0x159f85b). A place where a line may break after
//! a non-ASCII character (`breaks`) now counts as such a place too, and a line
//! broken there skips nothing (0x159fba1).
use super::glyphs::Glyph;
use crate::objects::text::font::OWN_GLYPHS;
use super::{breaks, game_text, text_loops};
use crate::points::{Point, Resume};
use std::cell::Cell;
use std::sync::atomic::AtomicUsize;

const FIT_GLYPH: usize = 0x159ef51;
const FIT_FOUND: usize = 0x159ef59;
const FIT_STEP: usize = 0x159f1db;
const FIT_NEXT: usize = 0x159f1e3;
const WRAP_GLYPH: usize = 0x159f88a;
const WRAP_FOUND: usize = 0x159f892;
const WRAP_STEP: usize = 0x159fde5;
const WRAP_NEXT: usize = 0x159fdec;
const WRAP_BREAK_TEST: usize = 0x159f85b;
const WRAP_BREAK_TESTED: usize = 0x159f871;
const WRAP_LINE_START: usize = 0x159fba1;
const WRAP_LINE_STARTED: usize = 0x159fbac;

static FIT_FOUND_AT: AtomicUsize = AtomicUsize::new(0);
static FIT_NEXT_AT: AtomicUsize = AtomicUsize::new(0);
static WRAP_FOUND_AT: AtomicUsize = AtomicUsize::new(0);
static WRAP_NEXT_AT: AtomicUsize = AtomicUsize::new(0);
static FIT_GLYPH_RESUME: [Resume; 1] = [Resume { rva: FIT_FOUND, address: &FIT_FOUND_AT }];
static FIT_STEP_RESUME: [Resume; 1] = [Resume { rva: FIT_NEXT, address: &FIT_NEXT_AT }];
static WRAP_GLYPH_RESUME: [Resume; 1] = [Resume { rva: WRAP_FOUND, address: &WRAP_FOUND_AT }];
static WRAP_STEP_RESUME: [Resume; 1] = [Resume { rva: WRAP_NEXT, address: &WRAP_NEXT_AT }];
static WRAP_BREAK_TESTED_AT: AtomicUsize = AtomicUsize::new(0);
static WRAP_LINE_STARTED_AT: AtomicUsize = AtomicUsize::new(0);
static WRAP_BREAK_RESUME: [Resume; 1] = [Resume { rva: WRAP_BREAK_TESTED, address: &WRAP_BREAK_TESTED_AT }];
static WRAP_LINE_RESUME: [Resume; 1] = [Resume { rva: WRAP_LINE_STARTED, address: &WRAP_LINE_STARTED_AT }];

/// What a loop's lookup saw, for its step: between the two the loop's index
/// stays on the character's first byte, where the game's own code cuts and
/// breaks lines, and the step then moves past the rest of it. The step only
/// skips what was looked up at that very place: the loops also step over
/// formatting codes they read themselves, a byte at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct LookedUp {
    text: usize,
    index: usize,
    rest: usize,
}

impl LookedUp {
    /// The bytes a step at `index` of `text` skips besides its own.
    fn rest_at(self, text: usize, index: usize) -> usize {
        if self.text == text && self.index == index {
            self.rest
        } else {
            0
        }
    }
}

thread_local! {
    static LOOKED_UP: Cell<LookedUp> = const { Cell::new(LookedUp { text: 0, index: 0, rest: 0 }) };
}

/// The glyph for the character at `text[index]`, from the font whose own
/// table is at `table`.
extern "C" fn glyph(table: usize, text: *const u8, index: usize) -> *const Glyph {
    crate::guarded("fitted text glyph", || {
        let (glyph, len) = unsafe { text_loops::glyph_at(table - OWN_GLYPHS, text.add(index)) };
        LOOKED_UP.set(LookedUp { text: text as usize, index, rest: len - 1 });
        glyph
    })
}

/// How many bytes the step at `index` of `string` (a `std::string*`) skips
/// besides its own.
extern "C" fn rest(string: *const u8, index: u32) -> u32 {
    crate::guarded("fitted text step", || {
        let text = unsafe { game_text::std_string(string) }.as_ptr() as usize;
        LOOKED_UP.replace(LookedUp::default()).rest_at(text, index as usize) as u32
    })
}

/// Replaces `movzx eax, byte [rdx+rax]; mov r15, [r14+rax*8]`.
#[unsafe(naked)]
extern "C" fn fit_glyph_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov r8, rdx",
            "mov rdx, rax",
            "mov rcx, r14",
            call glyph,
            "mov r15, [rsp + 0x48]",
        ),
        "jmp qword ptr [rip + {found}]",
        glyph = sym glyph,
        found = sym FIT_FOUND_AT,
    );
}

/// Replaces `inc edi; mov edx, edi; mov r10d, [rbx+0x10]`.
#[unsafe(naked)]
extern "C" fn fit_step_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rbx",
            "mov edx, edi",
            call rest,
            "add edi, dword ptr [rsp + 0x48]",
        ),
        "inc edi",
        "mov edx, edi",
        "mov r10d, dword ptr [rbx + 0x10]",
        "jmp qword ptr [rip + {next}]",
        rest = sym rest,
        next = sym FIT_NEXT_AT,
    );
}

/// Replaces `movzx eax, byte [rax+rdx]; mov r12, [r8+rax*8]`.
#[unsafe(naked)]
extern "C" fn wrap_glyph_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, r8",
            "mov r8, rdx",
            "mov rdx, rax",
            call glyph,
            "mov r12, [rsp + 0x48]",
        ),
        "jmp qword ptr [rip + {found}]",
        glyph = sym glyph,
        found = sym WRAP_FOUND_AT,
    );
}

/// Replaces `inc ebx; mov esi, ebx; mov ecx, [rdi+0x10]`.
#[unsafe(naked)]
extern "C" fn wrap_step_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdi",
            "mov edx, ebx",
            call rest,
            "add ebx, dword ptr [rsp + 0x48]",
        ),
        "inc ebx",
        "mov esi, ebx",
        "mov ecx, dword ptr [rdi + 0x10]",
        "jmp qword ptr [rip + {next}]",
        rest = sym rest,
        next = sym WRAP_NEXT_AT,
    );
}

/// Replaces `cmp byte [rax+rdx], ' '; jne; xorps xmm8, xmm8; mov eax, esi;
/// cmovne eax, r14d; mov r14d, eax; mov r15d, eax`: the text is at `rax`
/// (`std::string*` in `rdi`), the character at `esi`. Keeps `rcx` and `rdx`.
#[unsafe(naked)]
extern "C" fn wrap_break_stub() {
    std::arch::naked_asm!(
        "cmp byte ptr [rax + rdx], 0x20",
        "je 3f",
        call_rust!(
            "mov rcx, rax",
            "mov edx, dword ptr [rdi + 0x10]",
            "mov r8d, esi",
            call after,
            "cmp byte ptr [rsp + 0x48], 0",
        ),
        "je 4f",
        "3:",
        "xorps xmm8, xmm8",
        "mov r14d, esi",
        "4:",
        "mov r15d, r14d",
        "jmp qword ptr [rip + {tested}]",
        after = sym breaks::after_at,
        tested = sym WRAP_BREAK_TESTED_AT,
    );
}

/// Replaces `lea r12d, [r15+1]; mov [rbp+0x128], r12d`, where a line broken
/// at offset `r15d` starts the next line past the space there; a line broken
/// before a character keeps it.
#[unsafe(naked)]
extern "C" fn wrap_line_stub() {
    std::arch::naked_asm!(
        "mov rax, rdi",
        "cmp qword ptr [rdi + 0x18], 0x10",
        "jb 2f",
        "mov rax, qword ptr [rdi]",
        "2:",
        "mov r12d, r15d",
        "mov ecx, r15d",
        "cmp byte ptr [rax + rcx], 0x20",
        "jne 3f",
        "inc r12d",
        "3:",
        "mov dword ptr [rbp + 0x128], r12d",
        "jmp qword ptr [rip + {started}]",
        started = sym WRAP_LINE_STARTED_AT,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        // 0x159ec30: the text's data pointer is in rax and the offset in rdx
        // at the lookup; the font's table in r14.
        Point::keep(0x159ef48usize, "f3 41 0f 10 b6 48 08 00 00", "fitted text glyph scale"),
        Point::jump(FIT_GLYPH, "0f b6 04 02 4d 8b 3c c6", "fitted text glyph", fit_glyph_stub as *const () as usize, &FIT_GLYPH_RESUME),
        Point::keep(FIT_FOUND, "4d 85 ff", "fitted text glyph found"),
        Point::jump(FIT_STEP, "ff c7 8b d7 44 8b 53 10", "fitted text next byte", fit_step_stub as *const () as usize, &FIT_STEP_RESUME),
        Point::keep(FIT_NEXT, "41 3b fa", "fitted text end test"),
        // 0x159f510: the same with the table in r8.
        Point::keep(0x159f87dusize, "4c 8b 45 b8 f3 41 0f 10 b0 48 08 00 00", "wrapped text glyph scale"),
        Point::jump(WRAP_GLYPH, "0f b6 04 10 4d 8b 24 c0", "wrapped text glyph", wrap_glyph_stub as *const () as usize, &WRAP_GLYPH_RESUME),
        Point::keep(WRAP_FOUND, "4d 85 e4", "wrapped text glyph found"),
        Point::jump(WRAP_STEP, "ff c3 8b f3 8b 4f 10", "wrapped text next byte", wrap_step_stub as *const () as usize, &WRAP_STEP_RESUME),
        Point::keep(WRAP_NEXT, "44 0f b6 8d 48 01 00 00", "wrapped text flags"),
        // Where it remembers the last place a line may break, and where a
        // line broken there starts the next.
        Point::keep(0x159f849usize, "48 8b c7 48 8b 4f 18 48 83 f9 10 72 03 48 8b 07 8b d6", "wrapped text break test source"),
        Point::jump(
            WRAP_BREAK_TEST,
            "80 3c 10 20 75 04 45 0f 57 c0 8b c6 41 0f 45 c6 44 8b f0 44 8b f8",
            "wrapped text break test",
            wrap_break_stub as *const () as usize,
            &WRAP_BREAK_RESUME,
        ),
        Point::keep(WRAP_BREAK_TESTED, "48 8b c7", "wrapped text break tested"),
        Point::keep(0x159fae4usize, "45 8b cf e8 04 50 16 00", "wrapped text line up to the break"),
        Point::jump(WRAP_LINE_START, "45 8d 67 01 44 89 a5 28 01 00 00", "wrapped text next line start", wrap_line_stub as *const () as usize, &WRAP_LINE_RESUME),
        Point::keep(WRAP_LINE_STARTED, "45 8b f7", "wrapped text line started"),
    ]
}

#[cfg(test)]
mod tests {
    use super::LookedUp;

    #[test]
    fn a_step_skips_only_what_was_looked_up_there() {
        let seen = LookedUp { text: 0x1000, index: 4, rest: 2 };
        assert_eq!(seen.rest_at(0x1000, 4), 2);
        // A formatting code the loop read itself, or another text.
        assert_eq!(seen.rest_at(0x1000, 5), 0);
        assert_eq!(seen.rest_at(0x2000, 4), 0);
        assert_eq!(LookedUp::default().rest_at(0, 0), 0);
    }
}
