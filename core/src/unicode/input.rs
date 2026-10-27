//! Typing: what the player types reaches edit boxes as whole characters, and
//! the system's input method shows its own windows next to the caret
//! (docs/research/text-input.md §1, §4).
//!
//! The game's event pump (0x1569c70) handles SDL's text event at 0x1569f91.
//! It kept only the first character of the text, dropped anything of three
//! bytes or more and cut two-byte sequences to one wrong byte, then posted one
//! "char" event to the GUI (0x58 bytes: the character at +0x10, the constant
//! at 0x1dc1b00 at +0x38, type 2 at +0x50). The framework posts one event per
//! character, in the game's encoding and NUL-terminated at +0x10, and the
//! edit box's OnChar (0x15354e0) inserts the whole character, not its first
//! byte.
//!
//! SDL's input-method code (IME_HandleMessage 0x1764940) hid the system's
//! composition and candidate windows so SDL could draw them itself, which the
//! game never does, and SDL was never told where the caret is. The framework
//! lets Windows show its windows, turns off TSF's UI-less mode (0x1764e7e),
//! and places the windows at the caret of the focused edit box from
//! UpdateCaret (0x1534250). There is no input method of the framework's own.
use super::{encoding, scalar};
use crate::points::{Point, Resume};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, WM_IME_COMPOSITION, WM_IME_NOTIFY, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION};

const TEXT_EVENT: usize = 0x1569f91;
const NEXT_EVENT: usize = 0x156a22a;
const CHAR_EVENT_CONSTANT: usize = 0x1dc1b00;
const ON_CHAR_INSERT: usize = 0x15355f7;
const ON_CHAR_INSERTED: usize = 0x153560a;
const IME_MESSAGE: usize = 0x1764940;
const TSF_CREATE: usize = 0x1764e7e;
const CARET: usize = 0x15343bc;
const CARET_DONE: usize = 0x15343c3;

static NEXT: AtomicUsize = AtomicUsize::new(0);
static INSERTED: AtomicUsize = AtomicUsize::new(0);
static CARET_PLACED: AtomicUsize = AtomicUsize::new(0);
static TEXT_RESUME: [Resume; 1] = [Resume { rva: NEXT_EVENT, address: &NEXT }];
static INSERT_RESUME: [Resume; 1] = [Resume { rva: ON_CHAR_INSERTED, address: &INSERTED }];
static CARET_RESUME: [Resume; 1] = [Resume { rva: CARET_DONE, address: &CARET_PLACED }];
static CONSTANT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_IME: AtomicUsize = AtomicUsize::new(0);
/// Where the input method's windows were last put: x, y, line height.
static PLACED: AtomicU64 = AtomicU64::new(u64::MAX);

#[repr(C, align(16))]
struct CharEvent([u8; 0x58]);

type Post = unsafe extern "C" fn(usize, *const CharEvent) -> usize;
type Record = unsafe extern "C" fn(usize, u32, u32, usize) -> usize;

/// For SDL's text event at `event` (the text at +0xc): records it as the game
/// did, then posts each character to `receiver`.
extern "C" fn post_text(receiver: usize, recorder: usize, event: *const u8) {
    crate::guarded("text input", || unsafe {
        let record: Record = std::mem::transmute(*(*(recorder as *const *const usize)).add(0x20 / 8));
        record(recorder, 0x303, *(event.add(0x14) as *const u32), 0);
        let post: Post = std::mem::transmute(*(*(receiver as *const *const usize)).add(0x18 / 8));
        let constant = *(CONSTANT.load(Ordering::Acquire) as *const [u8; 16]);
        let raw = std::slice::from_raw_parts(event.add(0xc), 32);
        let text = String::from_utf8_lossy(&raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())]).into_owned();
        for c in text.chars() {
            let bytes = encoding::game_text(c.encode_utf8(&mut [0; 4]));
            let mut e = CharEvent([0; 0x58]);
            e.0[0x10..0x10 + bytes.len()].copy_from_slice(&bytes);
            e.0[0x38..0x48].copy_from_slice(&constant);
            e.0[0x50..0x54].copy_from_slice(&2u32.to_le_bytes());
            post(receiver, &e);
        }
    })
}

/// Copies the character at `from` (NUL-terminated) to `to` with a NUL after
/// it; returns its length. An empty one is a single NUL, as the game had it.
extern "C" fn copy_char(from: *const u8, to: *mut u8) -> usize {
    crate::guarded("text input character", || unsafe {
        let mut bytes = [0u8; 4];
        let mut n = 0;
        while n < 4 && *from.add(n) != 0 {
            bytes[n] = *from.add(n);
            n += 1;
        }
        let len = scalar::decode(&bytes[..n]).1.max(1);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), to, len);
        *to.add(len) = 0;
        len
    })
}

type ImeMessage = unsafe extern "C" fn(HWND, u32, usize, *mut isize, usize) -> i32;

/// The result bits of WM_IME_COMPOSITION, which make DefWindowProc send the
/// committed text again as WM_IME_CHAR.
const RESULT_BITS: isize = (GCS_RESULTREADSTR | GCS_RESULTREADCLAUSE | GCS_RESULTSTR | GCS_RESULTCLAUSE) as isize;

/// SDL's handling, then what Windows needs to draw its own windows. Returns
/// non-zero when the game must not pass the message to DefWindowProc.
unsafe extern "C" fn ime_message(window: HWND, message: u32, wparam: usize, lparam: *mut isize, video: usize) -> i32 {
    let original: ImeMessage = std::mem::transmute(ORIGINAL_IME.load(Ordering::Acquire));
    let sent = *lparam;
    let handled = original(window, message, wparam, lparam, video);
    crate::guarded("input method message", || match message {
        // SDL clears the flags that let the input method show its windows.
        WM_IME_SETCONTEXT => {
            *lparam = sent;
            // A context made active again gets the caret's place anew.
            PLACED.store(u64::MAX, Ordering::Relaxed);
            handled
        }
        WM_IME_STARTCOMPOSITION | WM_IME_NOTIFY => 0,
        // SDL has sent the committed text; Windows still updates its window.
        WM_IME_COMPOSITION => {
            DefWindowProcW(window, message, wparam, sent & !RESULT_BITS);
            1
        }
        _ => handled,
    })
}

/// Where on screen, in GUI units, the caret of `edit_box` is, `offset` being
/// its place in the text (x low, y high). UpdateCaret positions the caret
/// sprite relative to the box's parent: the box's place there, plus the text
/// inset (+0x28c, +0x240), plus a shift kept at `[[box+0x1e0]+0x32604]`, plus
/// the offset. The box's place on screen is what its vtable +0xd8 gives, as
/// its click handling (0x1534560) takes it from the mouse.
fn caret_on_screen(m: &crate::memory::Memory, edit_box: usize, offset: u32) -> crate::Result<(i32, i32)> {
    let mut place = [0i32; 2];
    m.call(m.vslot(edit_box, 0xd8)?, &[edit_box, place.as_mut_ptr() as usize])?;
    let shift = m.ptr(edit_box + 0x1e0)? + 0x32604;
    let x = place[0] + m.i32(edit_box + 0x28c)? + m.i32(edit_box + 0x240)? + m.i32(shift)? + (offset & 0xffff) as i32;
    let y = place[1] + m.i32(edit_box + 0x290)? + m.i32(edit_box + 0x244)? + m.i32(shift + 4)? + (offset >> 16) as i32;
    Ok((x, y))
}

/// The caret of edit box `edit_box` is at `offset` in its text (x low, y
/// high, GUI units); puts the input method's windows there when the box has
/// the keyboard.
extern "C" fn caret_at(edit_box: usize, offset: u32) {
    crate::guarded("input method position", || unsafe {
        if *((edit_box + 0x260) as *const u8) == 0 {
            return;
        }
        let window = GetFocus();
        let m = *crate::memory().expect("hooks run after startup");
        let (Some((across, down)), Ok(font)) = (crate::game::gui_scale(&m), m.ptr(edit_box + 0xc8 + 0x98)) else { return };
        if window.is_null() || font == 0 {
            return;
        }
        let Ok((x, y)) = caret_on_screen(&m, edit_box, offset) else { return };
        // The font's line height, as its method at 0x159ba20 works it out.
        let line = (*((font + 0x960) as *const i32) as f32 * *((font + 0x968) as *const f32)) as i32;
        let (px, py, ph) = ((x as f32 * across) as i32, (y as f32 * down) as i32, ((line as f32 * down) as i32).max(1));
        let key = (px as u16 as u64) | (py as u16 as u64) << 16 | (ph as u16 as u64) << 32;
        if PLACED.swap(key, Ordering::Relaxed) == key {
            return;
        }
        let context = ImmGetContext(window);
        if context.is_null() {
            return;
        }
        let composition = COMPOSITIONFORM { dwStyle: CFS_POINT, ptCurrentPos: POINT { x: px, y: py }, rcArea: RECT { left: 0, top: 0, right: 0, bottom: 0 } };
        ImmSetCompositionWindow(context, &composition);
        // Below the line, never over it.
        let candidates = CANDIDATEFORM {
            dwIndex: 0,
            dwStyle: CFS_EXCLUDE,
            ptCurrentPos: POINT { x: px, y: py + ph },
            rcArea: RECT { left: px, top: py, right: px + 1, bottom: py + ph },
        };
        ImmSetCandidateWindow(context, &candidates);
        ImmReleaseContext(window, context);
    })
}

/// Replaces `mov eax, [rbp-0x44]; xor bl, bl`, the start of the text event's
/// case: the receiver is in `r14`, the recorder in `r15`, the event at
/// `rbp-0x50`.
#[unsafe(naked)]
extern "C" fn text_event_stub() {
    std::arch::naked_asm!(
        "lea r8, [rbp - 0x50]",
        "mov rcx, r14",
        "mov rdx, r15",
        "push rbp",
        "mov rbp, rsp",
        "and rsp, -16",
        "sub rsp, 0x20",
        "call {post}",
        "mov rsp, rbp",
        "pop rbp",
        "jmp qword ptr [rip + {next}]",
        post = sym post_text,
        next = sym NEXT,
    );
}

/// Replaces the one-byte string OnChar builds at `rsp+0x20` (size at
/// `rsp+0x30`) from the character at `r14`. Keeps `rax` and `rcx`, which the
/// insertion call that follows uses.
#[unsafe(naked)]
extern "C" fn on_char_stub() {
    std::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rbp",
        "mov rbp, rsp",
        "and rsp, -16",
        "sub rsp, 0x20",
        "mov rcx, r14",
        // The string is at the function's rsp + 0x20: rbp + 0x18 + 0x20 here.
        "lea rdx, [rbp + 0x38]",
        "call {copy}",
        "mov rsp, rbp",
        "pop rbp",
        "mov qword ptr [rsp + 0x40], rax",
        "pop rcx",
        "pop rax",
        "jmp qword ptr [rip + {inserted}]",
        copy = sym copy_char,
        inserted = sym INSERTED,
    );
}

/// Runs after UpdateCaret works out the caret: offset packed in `ebx` (x low,
/// y high), the edit box in `rdi`. Then does the `mov rcx, [rdi+0x1f0]` it
/// replaces.
#[unsafe(naked)]
extern "C" fn caret_stub() {
    std::arch::naked_asm!(
        "push rbp",
        "mov rbp, rsp",
        "and rsp, -16",
        "sub rsp, 0x20",
        "mov rcx, rdi",
        "mov edx, ebx",
        "call {caret}",
        "mov rsp, rbp",
        "pop rbp",
        "mov rcx, qword ptr [rdi + 0x1f0]",
        "jmp qword ptr [rip + {done}]",
        caret = sym caret_at,
        done = sym CARET_PLACED,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(TEXT_EVENT, "8b 45 bc 32 db", "SDL text event", text_event_stub as *const () as usize, &TEXT_RESUME),
        Point::keep(NEXT_EVENT, "48 8d 1d 27 32 75 00", "event pump next event"),
        // How the game builds and posts the char event the stub now builds.
        Point::keep(
            0x1569fb1usize,
            "49 8b 07 45 33 c9 44 8b 45 c4 ba 03 03 00 00 49 8b cf ff 50 20",
            "text event recording",
        ),
        Point::keep(0x1569ff2usize, "c7 45 a0 02 00 00 00 66 0f 6f 05 ff 7a 85 00 48 8d 54 24 50 49 8b 06 49 8b ce", "char event type and post"),
        Point::keep(CHAR_EVENT_CONSTANT, "03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00", "char event constant").recorded(&CONSTANT),
        Point::jump(
            ON_CHAR_INSERT,
            "48 c7 44 24 30 01 00 00 00 40 88 7c 24 20 c6 44 24 21 00",
            "edit box character",
            on_char_stub as *const () as usize,
            &INSERT_RESUME,
        ),
        Point::keep(ON_CHAR_INSERTED, "48 8d 54 24 20 ff 90 a0 00 00 00", "edit box inserts the character"),
        Point::replace(IME_MESSAGE, "40 55 56 41 55 41 56 41 57 48 83 ec 20 48 8b 6c", "SDL input method messages", ime_message as *const () as usize, &ORIGINAL_IME),
        // WM_IME_SETCONTEXT: SDL zeroes the flags through the lParam pointer.
        Point::keep(0x1764c7cusize, "4d 89 29", "SDL hides the input method windows"),
        // CoCreateInstance of TSF's thread manager fails: no UI-less mode.
        Point::write(TSF_CREATE, "ff 15 ac 26 40 00", "SDL TSF UI-less setup", "b8 05 40 00 80 90"),
        Point::jump(CARET, "48 8b 8f f0 01 00 00", "edit box caret position", caret_stub as *const () as usize, &CARET_RESUME),
        Point::keep(CARET_DONE, "48 85 c9", "edit box caret sprite"),
        Point::keep(0x15343b0usize, "48 8d 55 30 ff 50 58 8b 18 89 5d 38", "edit box caret offset"),
        // How UpdateCaret places the caret relative to the box's parent: the
        // text inset and the shift at [[box+0x1e0]+0x32604].
        Point::keep(0x153427eusize, "48 8b 81 e0 01 00 00 4c 8b 98 04 26 03 00", "edit box caret shift"),
        Point::keep(0x15342adusize, "44 8b bf 8c 02 00 00 44 03 bf 40 02 00 00 44 03 7d 30 45 03 fb", "edit box caret x"),
        Point::keep(0x15342c2usize, "8b 87 90 02 00 00 03 87 44 02 00 00 03 45 34", "edit box caret y"),
        // The box's click handling takes its place on screen from vtable +0xd8.
        Point::keep(0x1534588usize, "48 8b 01 48 8d 55 30 ff 90 d8 00 00 00", "edit box place on screen"),
        Point::keep(0x1535266usize, "c6 81 60 02 00 00 01", "edit box focus flag"),
        Point::keep(0x159ba20usize, "66 0f 6e 81 60 09 00 00 0f 5b c0 f3 0f 59 81 68 09 00 00", "font line height"),
    ]
}
