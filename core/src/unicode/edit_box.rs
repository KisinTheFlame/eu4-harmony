//! Edit boxes, a character at a time. The game's text editor (box + 0xc8,
//! vtable 0x1d915c0) keeps its text as a `std::string` at +0x30 and its caret
//! as a line (+0x56) and a byte column within it (+0x54); every operation
//! moved, cut and deleted single bytes, which splits a UTF-8 sequence. Here
//! each one moves over, cuts before or deletes a whole character
//! (docs/research/text-input.md §2–3):
//!
//! - Left (0x15384d0) and Right (0x15385a0) step over a whole character;
//!   Backspace (0x1538db0) and Delete (0x1538f30) erase all of its bytes
//!   before the game erases the last one;
//! - a click (0x15361f0), Up (0x15387d0) and Down (0x1538870) keep their
//!   byte column, which can fall inside a character, so they end on the
//!   start of that character;
//! - the length cuts in InsertAtCaret, Append and Prepend, and the line fit
//!   of multi-line boxes (0x1537210), end before a character they would split;
//! - the forbidden-character filter (0xb19590) compares characters, not
//!   bytes, so it no longer strips continuation bytes that happen to equal
//!   `£`, `¤` or `§`;
//! - paste and copy go through SDL's clipboard, which is UTF-8, so text is
//!   converted to and from the game's encoding on the way;
//! - keys (0x15366c0): plain Home and End move instead of selecting, and
//!   Shift with Left or Right selects a character.
use super::{breaks, game_text};
use crate::hooks::HookFrame;
use crate::points::{Point, Resume};
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

const LINE_COL_TO_INDEX: usize = 0x153aa90;
const SET_CARET: usize = 0x1536f50;
const ERASE_AT: usize = 0x153ab90;
const LEFT: usize = 0x15384d6;
const LEFT_LINE_START: usize = 0x15384eb;
const LEFT_MOVED: usize = 0x1538513;
const RIGHT: usize = 0x1538616;
const RIGHT_SET: usize = 0x153861c;
const BACKSPACE: usize = 0x1538e5f;
const BACKSPACE_ERASE: usize = 0x1538e65;
const DELETE: usize = 0x1538fbe;
const DELETE_ERASE: usize = 0x1538fc3;
const CLICK: usize = 0x153646f;
const CLICK_DONE: usize = 0x1536479;
const UP: usize = 0x15387d0;
const DOWN: usize = 0x1538870;
const INSERT_CUT: usize = 0x153a979;
const INSERT_CUT_DONE: usize = 0x153a981;
const APPEND_CUT: usize = 0x153a2e4;
const APPEND_CUT_DONE: usize = 0x153a2ec;
const PREPEND_CUT: usize = 0x153a6f5;
const PREPEND_CUT_DONE: usize = 0x153a6fd;
const FIT_PREFIX: usize = 0x1537210;
const REMOVE_CHARS: usize = 0xb19590;
const PASTE_GET: usize = 0x1539250;
const PASTE_GOT: usize = 0x1539255;
const COPY_SET: usize = 0x1539604;
const COPY_DONE: usize = 0x1539609;
const KEY_UNMATCHED: usize = 0x1536a31;
const KEY_NEXT: usize = 0x1536a38;
const KEY_HANDLED: usize = 0x1536b54;

static GET_CLIPBOARD: AtomicUsize = AtomicUsize::new(0);
static SET_CLIPBOARD: AtomicUsize = AtomicUsize::new(0);
static SDL_FREE: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_FIT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_REMOVE: AtomicUsize = AtomicUsize::new(0);

static LEFT_RESUME: [Resume; 2] =
    [Resume { rva: LEFT_LINE_START, address: &LEFT_AT_LINE_START }, Resume { rva: LEFT_MOVED, address: &LEFT_DONE }];
static LEFT_AT_LINE_START: AtomicUsize = AtomicUsize::new(0);
static LEFT_DONE: AtomicUsize = AtomicUsize::new(0);
static RIGHT_DONE: AtomicUsize = AtomicUsize::new(0);
static BACKSPACE_DONE: AtomicUsize = AtomicUsize::new(0);
static DELETE_DONE: AtomicUsize = AtomicUsize::new(0);
static CLICKED: AtomicUsize = AtomicUsize::new(0);
static INSERT_DONE: AtomicUsize = AtomicUsize::new(0);
static APPEND_DONE: AtomicUsize = AtomicUsize::new(0);
static PREPEND_DONE: AtomicUsize = AtomicUsize::new(0);
static PASTED: AtomicUsize = AtomicUsize::new(0);
static COPIED: AtomicUsize = AtomicUsize::new(0);
static RIGHT_RESUME: [Resume; 1] = [Resume { rva: RIGHT_SET, address: &RIGHT_DONE }];
static BACKSPACE_RESUME: [Resume; 1] = [Resume { rva: BACKSPACE_ERASE, address: &BACKSPACE_DONE }];
static DELETE_RESUME: [Resume; 1] = [Resume { rva: DELETE_ERASE, address: &DELETE_DONE }];
static CLICK_RESUME: [Resume; 1] = [Resume { rva: CLICK_DONE, address: &CLICKED }];
static INSERT_RESUME: [Resume; 1] = [Resume { rva: INSERT_CUT_DONE, address: &INSERT_DONE }];
static APPEND_RESUME: [Resume; 1] = [Resume { rva: APPEND_CUT_DONE, address: &APPEND_DONE }];
static PREPEND_RESUME: [Resume; 1] = [Resume { rva: PREPEND_CUT_DONE, address: &PREPEND_DONE }];
static PASTE_RESUME: [Resume; 1] = [Resume { rva: PASTE_GOT, address: &PASTED }];
static COPY_RESUME: [Resume; 1] = [Resume { rva: COPY_DONE, address: &COPIED }];
static KEY_NEXT_AT: AtomicUsize = AtomicUsize::new(0);
static KEY_HANDLED_AT: AtomicUsize = AtomicUsize::new(0);
static KEY_RESUME: [Resume; 2] = [Resume { rva: KEY_NEXT, address: &KEY_NEXT_AT }, Resume { rva: KEY_HANDLED, address: &KEY_HANDLED_AT }];

// What the edit box operations do to the text, worked out without the game.

/// The column Left moves to, from `column` on the line starting at byte
/// `line_start`: the start of the character before the caret, or of the one
/// the caret lies inside, or the end of the text when the caret lies past it.
/// Backspace leaves the caret one byte into the character after the one it
/// erased (or past the end of the text), so the Left it calls lands on the
/// erased character's place.
pub fn left_column_in(text: &[u8], line_start: usize, column: usize) -> usize {
    let at = line_start + column;
    if at > text.len() {
        return text.len().saturating_sub(line_start);
    }
    column - game_text::previous_len(text, at).clamp(1, column.max(1))
}

/// The index Right moves to from `index`: past the character there.
pub fn right_index_in(text: &[u8], index: usize) -> usize {
    index + game_text::len_at(text, index).max(1)
}

/// Backspace at `index`: where the character before it starts and how many
/// bytes it has.
pub fn backspace_in(text: &[u8], index: usize) -> (usize, usize) {
    let len = game_text::previous_len(text, index).max(1);
    (index.saturating_sub(len), len)
}

/// Delete at `index`: how many bytes the character there has.
pub fn delete_in(text: &[u8], index: usize) -> usize {
    game_text::len_at(text, index).max(1)
}

/// How many bytes of `line` a multi-line box shows on one line, from the
/// game's `count` of bytes that fit: never splitting a character, at least
/// the first character, and, when the text goes on, ending where a line may
/// break if there is such a place.
pub fn fit_in(line: &[u8], count: i32) -> i32 {
    match game_text::floor_boundary(line, count.max(0) as usize) {
        0 if count > 0 && !line.is_empty() => game_text::len_at(line, 0) as i32,
        end if end < line.len() => breaks::last_at_or_before(line, end).unwrap_or(end) as i32,
        end => end as i32,
    }
}

/// `text` without the characters `forbidden` has (read up to its NUL, as the
/// game does), shifted down in place; returns the new length.
pub fn remove_in(text: &mut [u8], forbidden: &[u8]) -> usize {
    let forbidden = &forbidden[..forbidden.iter().position(|&b| b == 0).unwrap_or(forbidden.len())];
    let forbidden: Vec<char> = game_text::chars(forbidden).map(|(c, _)| c).collect();
    let mut write = 0;
    for (c, range) in game_text::chars(&text.to_vec()) {
        if !forbidden.contains(&c) {
            let len = range.len();
            text.copy_within(range, write);
            write += len;
        }
    }
    write
}

// The hooks, on the game's editor.

fn memory() -> crate::memory::Memory {
    *crate::memory().expect("hooks run after startup")
}

/// The editor's text.
unsafe fn text<'a>(inner: usize) -> &'a [u8] {
    game_text::std_string((inner + 0x30) as *const u8)
}

/// The caret's line and byte column.
unsafe fn caret(inner: usize) -> (usize, usize) {
    (*((inner + 0x56) as *const u16) as usize, *((inner + 0x54) as *const u16) as usize)
}

/// Where a line and column are in the text, as the editor counts (it never
/// goes past the end of the text).
fn index_of(inner: usize, line: usize, column: usize) -> crate::Result<usize> {
    let m = memory();
    m.call(m.at(LINE_COL_TO_INDEX), &[inner, line, column]).map(|i| i as u32 as usize)
}

fn erase(inner: usize, at: usize, count: usize) -> crate::Result<()> {
    let m = memory();
    for _ in 0..count {
        m.call(m.at(ERASE_AT), &[inner, at])?;
    }
    Ok(())
}

/// The column Left moves to from a column that is not 0.
extern "C" fn left_column(inner: usize) -> u32 {
    crate::guarded("edit box left", || {
        let (line, column) = unsafe { caret(inner) };
        let line_start = index_of(inner, line, 0).unwrap_or_else(|e| crate::fatal(&format!("edit box left: {e}")));
        left_column_in(unsafe { text(inner) }, line_start, column) as u32
    })
}

/// The index Right moves to from `index`.
extern "C" fn right_index(inner: usize, index: u32) -> u32 {
    crate::guarded("edit box right", || right_index_in(unsafe { text(inner) }, index as usize) as u32)
}

/// Before Backspace erases the byte before `index` and calls Left: erases
/// the rest of the character that byte ends and moves the caret back by as
/// many bytes. Returns where the game's own erase then goes.
extern "C" fn backspace_prepare(inner: usize, index: u32) -> u32 {
    crate::guarded("edit box backspace", || {
        let (start, len) = backspace_in(unsafe { text(inner) }, index as usize);
        if len > 1 {
            if let Err(e) = erase(inner, start, len - 1) {
                crate::fatal(&format!("edit box backspace: {e}"));
            }
            let column = unsafe { &mut *((inner + 0x54) as *mut u16) };
            *column = column.saturating_sub(len as u16 - 1);
        }
        start as u32
    })
}

/// Before Delete erases the byte at `index`: erases the rest of the
/// character it starts. Returns `index`.
extern "C" fn delete_prepare(inner: usize, index: u32) -> u32 {
    crate::guarded("edit box delete", || {
        let len = delete_in(unsafe { text(inner) }, index as usize);
        if len > 1 {
            if let Err(e) = erase(inner, index as usize + 1, len - 1) {
                crate::fatal(&format!("edit box delete: {e}"));
            }
        }
        index
    })
}

/// A click's byte column in a line (`std::string*`), moved back to the start
/// of the character it falls in.
extern "C" fn line_column(line: *const u8, column: u32) -> u32 {
    crate::guarded("edit box click", || game_text::floor_boundary(unsafe { game_text::std_string(line) }, column as usize) as u32)
}

/// Up and Down kept the byte column; the caret goes back to the start of the
/// character it fell in.
extern "C" fn snap_caret(frame: *mut HookFrame) {
    crate::guarded("edit box caret", || {
        let inner = unsafe { (*frame).args[0] };
        let result = (|| -> crate::Result<()> {
            let (line, column) = unsafe { caret(inner) };
            let at = index_of(inner, line, column)?;
            let start = game_text::floor_boundary(unsafe { text(inner) }, at);
            if start != at {
                let m = memory();
                m.call(m.at(SET_CARET), &[inner, start])?;
                // The editor's own "caret moved" method, as Up and Down end.
                m.call(m.vslot(inner, 0x208)?, &[inner])?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            crate::fatal(&format!("edit box caret: {e}"));
        }
    })
}

/// How many bytes of a `std::string` to keep when it is cut to `count`.
extern "C" fn cut(string: *const u8, count: u32) -> u32 {
    crate::guarded("edit box cut", || game_text::floor_boundary(unsafe { game_text::std_string(string) }, count as usize) as u32)
}

type Fit = unsafe extern "C" fn(usize, usize) -> i32;

/// The bytes of a line (`std::string*`) that fit a multi-line box's width
/// ([`fit_in`]). The caller (0x1537480) breaks a line at the last space before
/// them, and with no space, as in Chinese, right at them, which cut Latin
/// names in two (the country history window lays its text out in such a box).
unsafe extern "C" fn fit_prefix(inner: usize, line: usize) -> i32 {
    let original: Fit = std::mem::transmute(ORIGINAL_FIT.load(Ordering::Acquire));
    let count = original(inner, line);
    crate::guarded("edit box line fit", || fit_in(game_text::std_string(line as *const u8), count))
}

/// Removes from `text` (`std::string*`) every character found in `chars`
/// (`std::string*`). Returns whether the text got shorter.
unsafe extern "C" fn remove_chars(text: *mut u8, chars: *const u8) -> u8 {
    crate::guarded("edit box filter", || {
        let bytes = game_text::std_string_mut(text);
        let len = remove_in(bytes, game_text::std_string(chars));
        let shorter = len < bytes.len();
        if shorter {
            // The terminator lives one past the size, inside the capacity.
            *bytes.as_mut_ptr().add(len) = 0;
            *(text.add(0x10) as *mut usize) = len;
        }
        shorter as u8
    })
}

/// A NUL-terminated string, read no further than `max` bytes.
unsafe fn c_bytes<'a>(text: *const u8, max: usize) -> &'a [u8] {
    let mut len = 0;
    while len < max && *text.add(len) != 0 {
        len += 1;
    }
    std::slice::from_raw_parts(text, len)
}

thread_local! {
    /// The pasted text, until the next paste: [`paste_text`] returns a
    /// pointer into it, and the game copies the text at once.
    static PASTE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

type GetText = unsafe extern "C" fn() -> *mut u8;
type SetText = unsafe extern "C" fn(*const u8) -> i32;
type Free = unsafe extern "C" fn(*mut u8);

/// SDL's clipboard text in the game's encoding, NUL-terminated.
extern "C" fn paste_text() -> *const u8 {
    crate::guarded("edit box paste", || unsafe {
        let get: GetText = std::mem::transmute(GET_CLIPBOARD.load(Ordering::Acquire));
        let free: Free = std::mem::transmute(SDL_FREE.load(Ordering::Acquire));
        let utf8 = get();
        let mut converted = if utf8.is_null() { Vec::new() } else { game_text::from_str(&String::from_utf8_lossy(c_bytes(utf8, 1 << 20))) };
        if !utf8.is_null() {
            free(utf8);
        }
        converted.push(0);
        PASTE.with(|p| {
            *p.borrow_mut() = converted;
            p.borrow().as_ptr()
        })
    })
}

/// Puts the selection (`text`, in the game's encoding) on SDL's clipboard.
extern "C" fn copy_text(text: *const u8) -> i32 {
    crate::guarded("edit box copy", || unsafe {
        let set: SetText = std::mem::transmute(SET_CLIPBOARD.load(Ordering::Acquire));
        let mut utf8 = game_text::to_utf8(c_bytes(text, 1 << 20)).into_bytes();
        utf8.push(0);
        set(utf8.as_ptr())
    })
}

/// Replaces `movzx eax, word [rcx+0x54]; mov rdi, rcx; test ax, ax; je;
/// dec ax; mov [rcx+0x54], ax`.
#[unsafe(naked)]
extern "C" fn left_stub() {
    std::arch::naked_asm!(
        "mov rdi, rcx",
        "movzx eax, word ptr [rcx + 0x54]",
        "test ax, ax",
        "jnz 2f",
        "jmp qword ptr [rip + {line_start}]",
        "2:",
        call_rust!(
            "mov rcx, rdi",
            call column,
            "mov eax, dword ptr [rsp + 0x48]",
        ),
        "mov word ptr [rdi + 0x54], ax",
        "jmp qword ptr [rip + {done}]",
        line_start = sym LEFT_AT_LINE_START,
        column = sym left_column,
        done = sym LEFT_DONE,
    );
}

/// Replaces `mov ebx, 0x63; mov ecx, ebx`, where the key handler goes on to
/// its Ctrl shortcuts once no plain key matched (key code at `[rsi]`,
/// modifiers at `[rsi+8]`, 4 for Shift; the editor in `rdi`). Shift with
/// Left or Right selects a character with the editor's own methods at vtable
/// +0xd0 and +0xe0, which the game never called, so those keys used to fall
/// through to the map.
#[unsafe(naked)]
extern "C" fn shift_arrow_stub() {
    std::arch::naked_asm!(
        "cmp dword ptr [rsi + 8], 4",
        "jne 2f",
        "mov eax, dword ptr [rsi]",
        "cmp eax, 0x40000050",
        "je 3f",
        "cmp eax, 0x4000004f",
        "jne 2f",
        "mov rax, qword ptr [rdi]",
        "mov rcx, rdi",
        "call qword ptr [rax + 0xe0]",
        "jmp qword ptr [rip + {handled}]",
        "3:",
        "mov rax, qword ptr [rdi]",
        "mov rcx, rdi",
        "call qword ptr [rax + 0xd0]",
        "jmp qword ptr [rip + {handled}]",
        "2:",
        "mov ebx, 0x63",
        "mov ecx, ebx",
        "jmp qword ptr [rip + {next}]",
        handled = sym KEY_HANDLED_AT,
        next = sym KEY_NEXT_AT,
    );
}

/// Replaces `mov rcx, rdi; lea edx, [rax+1]` before SetCaretFromIndex.
#[unsafe(naked)]
extern "C" fn right_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdi",
            "mov edx, eax",
            call index,
            "mov edx, dword ptr [rsp + 0x48]",
        ),
        "mov rcx, rdi",
        "jmp qword ptr [rip + {done}]",
        index = sym right_index,
        done = sym RIGHT_DONE,
    );
}

/// Replaces `mov rcx, rdi; lea edx, [rax-1]` before EraseAt.
#[unsafe(naked)]
extern "C" fn backspace_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdi",
            "mov edx, eax",
            call prepare,
            "mov edx, dword ptr [rsp + 0x48]",
        ),
        "mov rcx, rdi",
        "jmp qword ptr [rip + {done}]",
        prepare = sym backspace_prepare,
        done = sym BACKSPACE_DONE,
    );
}

/// Replaces `mov edx, eax; mov rcx, rbx` before EraseAt.
#[unsafe(naked)]
extern "C" fn delete_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rbx",
            "mov edx, eax",
            call prepare,
            "mov edx, dword ptr [rsp + 0x48]",
        ),
        "mov rcx, rbx",
        "jmp qword ptr [rip + {done}]",
        prepare = sym delete_prepare,
        done = sym DELETE_DONE,
    );
}

/// Replaces `mov [r15+0x56], ax; mov [r15+0x54], si`: line in `ax`, byte
/// column in `si`, the line's record (`std::string` first) in `rdi`.
#[unsafe(naked)]
extern "C" fn click_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, rdi",
            "mov edx, esi",
            call column,
            "mov esi, dword ptr [rsp + 0x48]",
        ),
        "mov word ptr [r15 + 0x56], ax",
        "mov word ptr [r15 + 0x54], si",
        "jmp qword ptr [rip + {done}]",
        column = sym line_column,
        done = sym CLICKED,
    );
}

/// The three length cuts replace `cmp edx, ecx; cmovle ecx, edx; mov r8d,
/// ecx`, with the text being cut (`std::string*`) in `rdi` or `rbx`.
macro_rules! cut_stub {
    ($name:ident, $text:literal, $done:ident) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            std::arch::naked_asm!(
                "cmp edx, ecx",
                "cmovle ecx, edx",
                call_rust!(
                    "mov edx, ecx",
                    $text,
                    call cut,
                    "mov ecx, dword ptr [rsp + 0x48]",
                ),
                "mov r8d, ecx",
                "jmp qword ptr [rip + {done}]",
                cut = sym cut,
                done = sym $done,
            );
        }
    };
}
cut_stub!(insert_cut_stub, "mov rcx, rdi", INSERT_DONE);
cut_stub!(append_cut_stub, "mov rcx, rbx", APPEND_DONE);
cut_stub!(prepend_cut_stub, "mov rcx, rbx", PREPEND_DONE);

/// Replaces `call SDL_GetClipboardText`: the text goes to `rax`.
#[unsafe(naked)]
extern "C" fn paste_stub() {
    std::arch::naked_asm!(
        call_rust!(call text, "mov rax, [rsp + 0x48]"),
        "jmp qword ptr [rip + {done}]",
        text = sym paste_text,
        done = sym PASTED,
    );
}

/// Replaces `call SDL_SetClipboardText` (text in `rcx`); the result goes to
/// `rax`.
#[unsafe(naked)]
extern "C" fn copy_stub() {
    std::arch::naked_asm!(
        call_rust!(call copy, "mov rax, [rsp + 0x48]"),
        "jmp qword ptr [rip + {done}]",
        copy = sym copy_text,
        done = sym COPIED,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(LINE_COL_TO_INDEX, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "editor line and column to index"),
        Point::keep(SET_CARET, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 4c", "editor caret from index"),
        Point::keep(ERASE_AT, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 4c", "editor erase at"),
        Point::jump(LEFT, "0f b7 41 54 48 8b f9 66 85 c0 74 09 66 ff c8 66 89 41 54", "editor left", left_stub as *const () as usize, &LEFT_RESUME),
        Point::keep(LEFT_LINE_START, "48 8b 01 48 89 5c 24 30", "editor left at line start"),
        Point::keep(LEFT_MOVED, "0f b7 47 54 48 8d 4f 70", "editor left moved"),
        Point::jump(RIGHT, "48 8b cf 8d 50 01", "editor right", right_stub as *const () as usize, &RIGHT_RESUME),
        Point::keep(RIGHT_SET, "e8 2f e9 ff ff", "editor right sets the caret"),
        Point::jump(BACKSPACE, "48 8b cf 8d 50 ff", "editor backspace", backspace_stub as *const () as usize, &BACKSPACE_RESUME),
        Point::keep(BACKSPACE_ERASE, "e8 26 1d 00 00 48 8b 07 48 8b cf ff 90 d8 00 00 00", "editor backspace erases and moves left"),
        Point::jump(DELETE, "8b d0 48 8b cb", "editor delete", delete_stub as *const () as usize, &DELETE_RESUME),
        Point::keep(DELETE_ERASE, "e8 c8 1b 00 00", "editor delete erases"),
        Point::jump(CLICK, "66 41 89 47 56 66 41 89 77 54", "editor click", click_stub as *const () as usize, &CLICK_RESUME),
        Point::keep(CLICK_DONE, "4c 8d 5c 24 60", "editor click done"),
        // The click loop's line record: its length is the string size at +0x10.
        Point::keep(0x153645busize, "8b 47 10", "editor click line length"),
        Point::observe(UP, "40 56 48 83 ec 20 0f b7 41 56 48 8b f1 66 85 c0", "editor up", 1, None, Some(snap_caret)),
        Point::observe(DOWN, "48 89 5c 24 18 56 48 83 ec 20 48 8b f1 48 8d 15", "editor down", 1, None, Some(snap_caret)),
        Point::jump(INSERT_CUT, "3b d1 0f 4e ca 44 8b c1", "editor insert length cut", insert_cut_stub as *const () as usize, &INSERT_RESUME),
        Point::keep(INSERT_CUT_DONE, "4c 39 47 10", "editor insert cut done"),
        Point::jump(APPEND_CUT, "3b d1 0f 4e ca 44 8b c1", "editor append length cut", append_cut_stub as *const () as usize, &APPEND_RESUME),
        Point::keep(APPEND_CUT_DONE, "4c 39 43 10", "editor append cut done"),
        Point::jump(PREPEND_CUT, "3b d1 0f 4e ca 44 8b c1", "editor prepend length cut", prepend_cut_stub as *const () as usize, &PREPEND_RESUME),
        Point::keep(PREPEND_CUT_DONE, "4c 39 43 10", "editor prepend cut done"),
        Point::replace(FIT_PREFIX, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "editor line fit", fit_prefix as *const () as usize, &ORIGINAL_FIT),
        Point::replace(REMOVE_CHARS, "40 53 56 57 41 54 48 83 ec 48 48 83 7a 18 10 4c", "remove forbidden characters", remove_chars as *const () as usize, &ORIGINAL_REMOVE),
        Point::jump(PASTE_GET, "e8 3b b2 1f 00", "editor paste reads the clipboard", paste_stub as *const () as usize, &PASTE_RESUME),
        Point::keep(PASTE_GOT, "48 c7 44 24 20 00 00 00 00", "editor paste copies the text"),
        Point::jump(COPY_SET, "e8 b7 c1 1f 00", "editor copy writes the clipboard", copy_stub as *const () as usize, &COPY_RESUME),
        Point::keep(COPY_DONE, "48 8d 4c 24 20", "editor copy done"),
        // The key handler (0x15366c0): plain Home and End called the methods
        // that select to the line's start and end, whose "select" argument is
        // ignored, so they always selected. They now call the ones that move.
        Point::keep(0x15366dausize, "40 b5 01 8b 02 85 c0", "editor key handled flag and key code"),
        Point::write(0x15369d9usize, "ff 90 d8 01 00 00", "editor Home moves", "ff 90 e0 01 00 00"),
        Point::write(0x1536a26usize, "ff 90 e8 01 00 00", "editor End moves", "ff 90 f0 01 00 00"),
        Point::jump(KEY_UNMATCHED, "bb 63 00 00 00 8b cb", "editor Shift with Left or Right", shift_arrow_stub as *const () as usize, &KEY_RESUME),
        Point::keep(KEY_NEXT, "e8 ff 43 51 00", "editor Ctrl+C key"),
        Point::keep(KEY_HANDLED, "40 0f b6 c5", "editor key handled"),
        // The methods that select a character to the left and to the right.
        Point::keep(0x1538560usize, "40 53 48 83 ec 20 48 8b 01 48 8b d9", "editor select left"),
        Point::keep(0x1538670usize, "40 53 48 83 ec 20 48 8b 01 48 8b d9", "editor select right"),
        Point::keep(c"SDL_GetClipboardText", "48 ff 25 59 bd 87 00", "SDL clipboard read").recorded(&GET_CLIPBOARD),
        Point::keep(c"SDL_SetClipboardText", "48 ff 25 21 aa 87 00", "SDL clipboard write").recorded(&SET_CLIPBOARD),
        Point::keep(c"SDL_free", "48 ff 25 99 ac 87 00", "SDL free").recorded(&SDL_FREE),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the game's Backspace does with the hooks: erase all but the last
    /// byte of the character before the caret and move the caret back by as
    /// many, erase that byte, then call Left.
    fn backspace(text: &mut Vec<u8>, column: &mut usize) {
        let (start, len) = backspace_in(text, *column);
        text.drain(start..start + len - 1);
        *column -= len - 1;
        text.remove(start);
        *column = left_column_in(text, 0, *column);
    }

    #[test]
    fn backspace_erases_whole_characters_at_the_end_and_in_the_middle() {
        // "朝鲜" with two Backspaces left "œ" (the 0x9C of 朝) when Left
        // measured the character before a caret past the end of the text.
        let mut text = "朝鲜".as_bytes().to_vec();
        let mut column = text.len();
        backspace(&mut text, &mut column);
        assert_eq!((text.as_slice(), column), ("朝".as_bytes(), 3));
        backspace(&mut text, &mut column);
        assert_eq!((text.as_slice(), column), (&b""[..], 0));
        // In the middle, before a Chinese and before a Latin character.
        for (before, after) in [("朝鲜中", "朝中"), ("朝鲜abc", "朝abc")] {
            let mut text = before.as_bytes().to_vec();
            let mut column = "朝鲜".len();
            backspace(&mut text, &mut column);
            assert_eq!((text.as_slice(), column), (after.as_bytes(), 3));
        }
    }

    #[test]
    fn left_and_right_step_over_whole_characters() {
        let text = "朝a鲜".as_bytes();
        assert_eq!(left_column_in(text, 0, 7), 4);
        assert_eq!(left_column_in(text, 0, 4), 3);
        assert_eq!(left_column_in(text, 0, 3), 0);
        // On a later line, past the end of the text.
        assert_eq!(left_column_in(text, 3, 9), 4);
        assert_eq!([0, 3, 4, 7].map(|at| right_index_in(text, at)), [3, 4, 7, 8]);
        assert_eq!(delete_in(text, 0), 3);
        assert_eq!(delete_in(text, 3), 1);
    }

    #[test]
    fn a_line_fits_whole_characters_and_breaks_where_it_may() {
        let line = game_text::from_str("由于Yuanzhang被");
        // The game's count cuts inside 由: at least that character.
        assert_eq!(fit_in(&line, 1), 3);
        // Inside "Yuanzhang": back to before it.
        assert_eq!(fit_in(&line, 8), 6);
        // All of it fits.
        assert_eq!(fit_in(&line, line.len() as i32), line.len() as i32);
    }

    #[test]
    fn forbidden_characters_go_whole_and_continuation_bytes_stay() {
        // 称 is E7 A7 B0: its A7 is not the forbidden § (also A7).
        let mut text = game_text::from_str("§Y称号{x}");
        let len = remove_in(&mut text, &game_text::from_str("§{}\0ignored"));
        assert_eq!(&text[..len], game_text::from_str("Y称号x").as_slice());
    }
}
