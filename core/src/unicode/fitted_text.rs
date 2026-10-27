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
use super::glyphs::{self, Glyph};
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

static FIT_FOUND_AT: AtomicUsize = AtomicUsize::new(0);
static FIT_NEXT_AT: AtomicUsize = AtomicUsize::new(0);
static WRAP_FOUND_AT: AtomicUsize = AtomicUsize::new(0);
static WRAP_NEXT_AT: AtomicUsize = AtomicUsize::new(0);
static FIT_GLYPH_RESUME: [Resume; 1] = [Resume { rva: FIT_FOUND, address: &FIT_FOUND_AT }];
static FIT_STEP_RESUME: [Resume; 1] = [Resume { rva: FIT_NEXT, address: &FIT_NEXT_AT }];
static WRAP_GLYPH_RESUME: [Resume; 1] = [Resume { rva: WRAP_FOUND, address: &WRAP_FOUND_AT }];
static WRAP_STEP_RESUME: [Resume; 1] = [Resume { rva: WRAP_NEXT, address: &WRAP_NEXT_AT }];

thread_local! {
    /// The character the loop looked up last: its text, its offset and how
    /// many bytes it has after the first.
    static LOOKED_UP: Cell<(usize, usize, usize)> = const { Cell::new((0, 0, 0)) };
}

/// The glyph for the character at `text[index]`, from the font whose own
/// table is at `table`.
extern "C" fn glyph(table: usize, text: *const u8, index: usize) -> *const Glyph {
    crate::guarded("fitted text glyph", || {
        let (glyph, len) = unsafe { glyphs::char_at(table - 0x120, text.add(index)) };
        LOOKED_UP.set((text as usize, index, len - 1));
        glyph
    })
}

/// How many bytes the step skips besides its own: the rest of the character
/// at `index` of `string` (a `std::string*`), if the loop looked it up.
extern "C" fn rest(string: *const usize, index: u32) -> u32 {
    crate::guarded("fitted text step", || {
        // MSVC keeps up to 15 bytes inline; the capacity is at + 0x18.
        let text = unsafe { if *string.add(3) >= 16 { *string } else { string as usize } };
        match LOOKED_UP.replace((0, 0, 0)) {
            (looked, at, rest) if looked == text && at == index as usize => rest as u32,
            _ => 0,
        }
    })
}

/// Saves the volatile registers but `rax` on a stack aligned for the call,
/// calls `$call` with the arguments the lines in `$args` set up, and restores
/// them; `rax` keeps the result.
macro_rules! around_call {
    ($($args:literal,)* call $call:ident) => {
        concat!(
            "push rbp\n",
            "mov rbp, rsp\n",
            "and rsp, -16\n",
            "sub rsp, 0xb0\n",
            "mov [rsp + 0x20], rcx\n",
            "mov [rsp + 0x28], rdx\n",
            "mov [rsp + 0x30], r8\n",
            "mov [rsp + 0x38], r9\n",
            "mov [rsp + 0x40], r10\n",
            "mov [rsp + 0x48], r11\n",
            "movdqa [rsp + 0x50], xmm0\n",
            "movdqa [rsp + 0x60], xmm1\n",
            "movdqa [rsp + 0x70], xmm2\n",
            "movdqa [rsp + 0x80], xmm3\n",
            "movdqa [rsp + 0x90], xmm4\n",
            "movdqa [rsp + 0xa0], xmm5\n",
            $($args, "\n",)*
            "call {", stringify!($call), "}\n",
            "mov rcx, [rsp + 0x20]\n",
            "mov rdx, [rsp + 0x28]\n",
            "mov r8, [rsp + 0x30]\n",
            "mov r9, [rsp + 0x38]\n",
            "mov r10, [rsp + 0x40]\n",
            "mov r11, [rsp + 0x48]\n",
            "movdqa xmm0, [rsp + 0x50]\n",
            "movdqa xmm1, [rsp + 0x60]\n",
            "movdqa xmm2, [rsp + 0x70]\n",
            "movdqa xmm3, [rsp + 0x80]\n",
            "movdqa xmm4, [rsp + 0x90]\n",
            "movdqa xmm5, [rsp + 0xa0]\n",
            "mov rsp, rbp\n",
            "pop rbp\n",
        )
    };
}

/// Replaces `movzx eax, byte [rdx+rax]; mov r15, [r14+rax*8]`.
#[unsafe(naked)]
extern "C" fn fit_glyph_stub() {
    std::arch::naked_asm!(
        around_call!("mov r8, rdx", "mov rdx, rax", "mov rcx, r14", call glyph),
        "mov r15, rax",
        "jmp qword ptr [rip + {found}]",
        glyph = sym glyph,
        found = sym FIT_FOUND_AT,
    );
}

/// Replaces `inc edi; mov edx, edi; mov r10d, [rbx+0x10]`.
#[unsafe(naked)]
extern "C" fn fit_step_stub() {
    std::arch::naked_asm!(
        "push rax",
        around_call!("mov rcx, rbx", "mov edx, edi", call rest),
        "add edi, eax",
        "pop rax",
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
        around_call!("mov rcx, r8", "mov r8, rdx", "mov rdx, rax", call glyph),
        "mov r12, rax",
        "jmp qword ptr [rip + {found}]",
        glyph = sym glyph,
        found = sym WRAP_FOUND_AT,
    );
}

/// Replaces `inc ebx; mov esi, ebx; mov ecx, [rdi+0x10]`.
#[unsafe(naked)]
extern "C" fn wrap_step_stub() {
    std::arch::naked_asm!(
        "push rax",
        around_call!("mov rcx, rdi", "mov edx, ebx", call rest),
        "add ebx, eax",
        "pop rax",
        "inc ebx",
        "mov esi, ebx",
        "mov ecx, dword ptr [rdi + 0x10]",
        "jmp qword ptr [rip + {next}]",
        rest = sym rest,
        next = sym WRAP_NEXT_AT,
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
    ]
}
