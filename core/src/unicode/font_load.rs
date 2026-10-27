//! Bitmap fonts with code points above 0xFF. The game's `.fnt` loader parses
//! each `char` line and drops any id above 0xFF; a jump at that check stores
//! those glyphs in the glyph table instead, so a mod's font that carries
//! Chinese is drawn as it is (ADR 0010: bitmap fonts first).
//!
//! In the loader (function at 0x15953c0), a `char` line's fields are parsed
//! into: id `edi`, x `r14d`, y `r15d`, width `r12d`, height `esi`, x offset
//! `r13d`, y offset `[rbp+0x1148]`, advance `[rsp+0x40]`; the font is at
//! `[rbp+0x1130]`, its own table at font + 0x120. Then `cmp edi, 0xff; ja`
//! skips large ids. The game frees a font's table with the function at
//! 0x1594360, which takes the table.
use super::glyphs::{self, Glyph};
use crate::hooks::HookFrame;
use crate::points::{Point, Resume};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const ID_CHECK: usize = 0x1595c86;
/// Continues with an id of 0xFF or less, as the game does.
const SMALL_ID: usize = 0x1595c92;
/// Where the game goes after storing a glyph: back to the next line.
const STORED: usize = 0x1595cfa;
/// The game's path for an id it rejects.
const BAD_ID: usize = 0x1595f01;
const DESTROY_TABLE: usize = 0x1594360;

static SMALL: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
static BAD: AtomicUsize = AtomicUsize::new(0);
static RESUME: [Resume; 3] =
    [Resume { rva: SMALL_ID, address: &SMALL }, Resume { rva: STORED, address: &DONE }, Resume { rva: BAD_ID, address: &BAD }];

static LOADED: AtomicU64 = AtomicU64::new(0);
static DUPLICATES: AtomicU64 = AtomicU64::new(0);

/// Stores one parsed glyph with an id above 0xFF.
extern "C" fn store(font: usize, id: u32, x: i32, y: i32, width: i32, height: i32, x_offset: i32, y_offset: i32, advance: i32) {
    crate::guarded("font glyph", || {
        let glyph = Glyph {
            x: x as u16,
            y: y as u16,
            width: width as u16,
            height: height as u16,
            x_offset: x_offset as i16,
            y_offset: y_offset as i16,
            advance: advance as i16,
            kerned: 0,
            page: 0,
        };
        let counter = if glyphs::insert(font + 0x120, id, glyph) { &LOADED } else { &DUPLICATES };
        counter.fetch_add(1, Ordering::Relaxed);
    })
}

/// Replaces `cmp edi, 0xff; ja BAD_ID`. Volatile registers are dead here: the
/// game has just returned from parsing the last field.
#[unsafe(naked)]
extern "C" fn id_check() {
    std::arch::naked_asm!(
        "cmp edi, 0xff",
        "jbe 2f",
        "cmp edi, 0x10ffff",
        "ja 3f",
        // rsp is 16-byte aligned in the loader's body; 0x60 keeps it so.
        "sub rsp, 0x60",
        "mov rcx, qword ptr [rbp + 0x1130]",
        "mov edx, edi",
        "mov r8d, r14d",
        "mov r9d, r15d",
        "mov dword ptr [rsp + 0x20], r12d",
        "mov dword ptr [rsp + 0x28], esi",
        "mov dword ptr [rsp + 0x30], r13d",
        "mov eax, dword ptr [rbp + 0x1148]",
        "mov dword ptr [rsp + 0x38], eax",
        "mov eax, dword ptr [rsp + 0xa0]",
        "mov dword ptr [rsp + 0x40], eax",
        "call {store}",
        "add rsp, 0x60",
        "jmp qword ptr [rip + {done}]",
        "2:",
        "jmp qword ptr [rip + {small}]",
        "3:",
        "jmp qword ptr [rip + {bad}]",
        store = sym store,
        done = sym DONE,
        small = sym SMALL,
        bad = sym BAD,
    );
}

extern "C" fn destroy_before(frame: *mut HookFrame) {
    crate::guarded("font destroy", || glyphs::forget(unsafe { (*frame).args[0] }));
}

/// How many glyphs above 0xFF the fonts brought, for the report.
pub fn loaded() -> (u64, u64) {
    (LOADED.load(Ordering::Relaxed), DUPLICATES.load(Ordering::Relaxed))
}

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(ID_CHECK, "81 ff ff 00 00 00 0f 87 6f 02 00 00", "font char id check", id_check as *const () as usize, &RESUME),
        // The parsing the jump relies on: which register holds which field.
        Point::keep(
            0x1595bb0usize,
            "48 63 f8 48 8d 8d e0 01 00 00 e8 e1 51 17 00 48 8d 8d 24 06 00 00 e8 0d 34 4b 00 44 8b f0 48 8d 8d e0 01 00 00 e8 c6 51 17 00 \
             48 8d 8d 24 06 00 00 e8 f2 33 4b 00 44 8b f8 48 8d 8d e0 01 00 00 e8 ab 51 17 00 48 8d 8d 24 06 00 00 e8 d7 33 4b 00 44 8b e0 \
             48 8d 8d e0 01 00 00 e8 90 51 17 00 48 8d 8d 24 06 00 00 e8 bc 33 4b 00 8b f0 83 ff 20 41 0f 44 f5 48 8d 8d e0 01 00 00 e8 6f \
             51 17 00 48 8d 8d 24 06 00 00 e8 9b 33 4b 00 44 8b e8 48 8d 8d e0 01 00 00 e8 54 51 17 00 48 8d 8d 24 06 00 00 e8 80 33 4b 00 \
             89 85 48 11 00 00 48 8d 8d e0 01 00 00 e8 36 51 17 00 48 8d 8d 24 06 00 00 e8 62 33 4b 00 89 44 24 40 48 8d 8d e0 01 00 00 \
             e8 1a 51 17 00",
            "font char line parsing",
        ),
        Point::keep(SMALL_ID, "83 ff 0a 0f 84 66 02 00 00", "font char id newline check"),
        Point::keep(STORED, "4c 8b 74 24 38 45 33 ed e9 00 fb ff ff", "font char line done"),
        Point::keep(BAD_ID, "e8 3a be 17 00", "font char id rejected"),
        Point::observe(DESTROY_TABLE, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48", "font table destructor", 1, Some(destroy_before), None),
    ]
}
