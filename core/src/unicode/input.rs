//! Typing: what the player types reaches edit boxes as whole characters, and
//! the system's input method shows its own windows next to the caret
//! (docs/research/text-input.md §1, §3).
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
//!
//! SDL turns every key press into a game key from its scan code, even one the
//! input method took (`VK_PROCESSKEY`): Enter that picks a candidate also
//! closed the search box, and Backspace that edits the composition also
//! deleted text. While an edit box has the keyboard, such presses go to the
//! input method alone. Elsewhere they still reach the game, so its shortcuts
//! keep working with a Chinese input method switched on.
use crate::resume;
use super::game_text;
use crate::objects::{
    gui::{self, edit_box, editor, events, manager},
    text::font,
};
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, VK_PROCESSKEY};
use windows_sys::Win32::UI::WindowsAndMessaging::{DefWindowProcW, WM_IME_COMPOSITION, WM_IME_NOTIFY, WM_IME_SETCONTEXT, WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP};

const TEXT_EVENT: usize = 0x1569f91;
const NEXT_EVENT: usize = 0x156a22a;
const CHAR_EVENT_CONSTANT: usize = 0x1dc1b00;
const ON_CHAR_INSERT: usize = 0x15355f7;
const ON_CHAR_INSERTED: usize = 0x153560a;
const IME_MESSAGE: usize = 0x1764940;
const TSF_CREATE: usize = 0x1764e7e;
const CARET: usize = 0x15343bc;
const CARET_DONE: usize = 0x15343c3;

resume!(TEXT_RESUME: NEXT = NEXT_EVENT);
resume!(INSERT_RESUME: INSERTED = ON_CHAR_INSERTED);
resume!(CARET_RESUME: CARET_PLACED = CARET_DONE);
static CONSTANT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_IME: Function<ImeMessage> = Function::new();
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
        let record: Record = std::mem::transmute(*(*(recorder as *const *const usize)).add(events::RECORD / 8));
        record(recorder, 0x303, *(event.add(0x14) as *const u32), 0);
        let post: Post = std::mem::transmute(*(*(receiver as *const *const usize)).add(events::POST / 8));
        let constant = *(CONSTANT.load(Ordering::Acquire) as *const [u8; 16]);
        let raw = std::slice::from_raw_parts(event.add(0xc), 32);
        let text = String::from_utf8_lossy(&raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())]).into_owned();
        for bytes in typed_chars(&text) {
            let mut e = CharEvent([0; 0x58]);
            e.0[0x10..0x10 + bytes.len()].copy_from_slice(&bytes);
            e.0[0x38..0x48].copy_from_slice(&constant);
            e.0[0x50..0x54].copy_from_slice(&2u32.to_le_bytes());
            post(receiver, &e);
        }
    })
}

/// SDL's typed text as one character event each, in the game's encoding.
fn typed_chars(text: &str) -> Vec<Vec<u8>> {
    text.chars().map(|c| game_text::from_str(c.encode_utf8(&mut [0; 4]))).collect()
}

/// The bytes OnChar inserts for the NUL-terminated `typed`: the character it
/// starts, or a single NUL for none, as the game had it.
fn typed_char(typed: &[u8]) -> &[u8] {
    let typed = &typed[..typed.iter().position(|&b| b == 0).unwrap_or(typed.len())];
    match game_text::len_at(typed, 0) {
        0 => &[0],
        len => &typed[..len],
    }
}

/// Makes the inline `std::string` at `string` the character at `from`.
extern "C" fn copy_char(from: *const u8, string: *mut u8) {
    crate::guarded("text input character", || unsafe {
        crate::game_string::set_inline(string, typed_char(game_text::up_to_nul(from, 4)));
    })
}

type ImeMessage = unsafe extern "C" fn(HWND, u32, usize, *mut isize, usize) -> i32;

/// The result bits of WM_IME_COMPOSITION, which make DefWindowProc send the
/// committed text again as WM_IME_CHAR.
const RESULT_BITS: isize = (GCS_RESULTREADSTR | GCS_RESULTREADCLAUSE | GCS_RESULTSTR | GCS_RESULTCLAUSE) as isize;

/// WM_IME_COMPOSITION's flags with the result ones cleared.
fn without_result(lparam: isize) -> isize {
    lparam & !RESULT_BITS
}

/// Whether an edit box has the keyboard.
fn edit_box_focused(m: &crate::memory::Memory) -> bool {
    let focused = || -> crate::Result<bool> {
        let listener = m.ptr(m.ptr(m.at(gui::globals::MANAGER))? + manager::FOCUSED_LISTENER)?;
        let class = m.ptr(listener)? == m.at(gui::classes::EDIT_BOX_LISTENER);
        Ok(listener != 0 && class && m.u8(listener - edit_box::LISTENER + edit_box::FOCUSED)? != 0)
    };
    focused().unwrap_or(false)
}

/// Whether `message` is a key the input method took (see the module notes).
fn taken_by_input_method(message: u32, wparam: usize) -> bool {
    matches!(message, WM_KEYDOWN | WM_KEYUP) && wparam == VK_PROCESSKEY as usize
}

/// SDL's handling, then what Windows needs to draw its own windows. Returns
/// non-zero when SDL's window procedure must stop there: neither SDL nor
/// DefWindowProc then sees the message.
unsafe extern "C" fn ime_message(window: HWND, message: u32, wparam: usize, lparam: *mut isize, video: usize) -> i32 {
    if taken_by_input_method(message, wparam) && crate::memory().is_some_and(|m| edit_box_focused(m)) {
        return 1;
    }
    let original = ORIGINAL_IME.get();
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
            DefWindowProcW(window, message, wparam, without_result(sent));
            1
        }
        _ => handled,
    })
}

/// Where on screen, in GUI units, the caret of `edit_box` is, `offset` being
/// its place in the text (x low, y high). UpdateCaret positions the caret
/// sprite relative to the box's parent: the box's place there, plus the text
/// insets, plus a shift (see [`edit_box`]), plus the offset. The box's place
/// on screen is what its method [`edit_box::PLACE`] gives, as its click
/// handling (0x1534560) takes it from the mouse.
fn caret_on_screen(m: &crate::memory::Memory, edit_box: usize, offset: u32) -> crate::Result<(i32, i32)> {
    let mut place = [0i32; 2];
    m.call(m.vslot(edit_box, edit_box::PLACE)?, &[edit_box, place.as_mut_ptr() as usize])?;
    let shift = m.ptr(edit_box + edit_box::SHIFT_HOLDER)? + edit_box::SHIFT;
    let inset = |[a, b]: [usize; 2]| Ok::<_, String>(m.i32(edit_box + a)? + m.i32(edit_box + b)?);
    let x = place[0] + inset(edit_box::INSET_X)? + m.i32(shift)? + (offset & 0xffff) as i32;
    let y = place[1] + inset(edit_box::INSET_Y)? + m.i32(shift + 4)? + (offset >> 16) as i32;
    Ok((x, y))
}

/// A caret at (`x`, `y`) on a line `line` high, all in GUI units, in window
/// pixels at the GUI scale `scale` (across, down): (x, y, line height), the
/// height at least one pixel.
fn caret_pixels((x, y): (i32, i32), line: i32, (across, down): (f32, f32)) -> (i32, i32, i32) {
    ((x as f32 * across) as i32, (y as f32 * down) as i32, ((line as f32 * down) as i32).max(1))
}

/// The caret of edit box `edit_box` is at `offset` in its text (x low, y
/// high, GUI units); puts the input method's windows there when the box has
/// the keyboard.
extern "C" fn caret_at(edit_box: usize, offset: u32) {
    crate::guarded("input method position", || unsafe {
        if *((edit_box + edit_box::FOCUSED) as *const u8) == 0 {
            return;
        }
        let window = GetFocus();
        let m = crate::hooked_memory();
        let (Some((across, down)), Ok(font)) = (crate::game::gui_scale(&m), m.ptr(edit_box + edit_box::EDITOR + editor::FONT)) else { return };
        if window.is_null() || font == 0 {
            return;
        }
        let Ok((x, y)) = caret_on_screen(&m, edit_box, offset) else { return };
        // The font's line height, as its method at 0x159ba20 works it out.
        let line = (*((font + font::LINE_HEIGHT) as *const i32) as f32 * *((font + font::SCALE) as *const f32)) as i32;
        let (px, py, ph) = caret_pixels((x, y), line, (across, down));
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
/// case: the receiver is in `r14`, the recorder in `r15`, the event at the
/// game frame's `rbp-0x50`.
#[unsafe(naked)]
extern "C" fn text_event_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rax, [rbp]",
            "lea r8, [rax - 0x50]",
            "mov rcx, r14",
            "mov rdx, r15",
            call post,
        ),
        "jmp qword ptr [rip + {next}]",
        post = sym post_text,
        next = sym NEXT,
    );
}

/// Replaces the one-byte `std::string` OnChar builds at the game's
/// `rsp+0x20` from the character at `r14`.
#[unsafe(naked)]
extern "C" fn on_char_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov rcx, r14",
            "lea rdx, [rbp + 8 + 0x20]",
            call copy,
        ),
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
        call_rust!(
            "mov rcx, rdi",
            "mov edx, ebx",
            call caret,
        ),
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
        )
        .uses(&[events::RECORD]),
        Point::keep(0x1569ff2usize, "c7 45 a0 02 00 00 00 66 0f 6f 05 ff 7a 85 00 48 8d 54 24 50 49 8b 06 49 8b ce 44 89 65 98 f3 0f 7f 45 88 4c 89 65 80 66 44 89 65 a4 c6 45 a6 00 ff 50 18", "char event type and post")
            .uses(&[events::POST]),
        Point::keep(CHAR_EVENT_CONSTANT, "03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00", "char event constant").recorded(&CONSTANT),
        Point::jump(
            ON_CHAR_INSERT,
            "48 c7 44 24 30 01 00 00 00 40 88 7c 24 20 c6 44 24 21 00",
            "edit box character",
            on_char_stub as *const () as usize,
            &INSERT_RESUME,
        ),
        Point::keep(ON_CHAR_INSERTED, "48 8d 54 24 20 ff 90 a0 00 00 00", "edit box inserts the character"),
        Point::replace(IME_MESSAGE, "40 55 56 41 55 41 56 41 57 48 83 ec 20 48 8b 6c", "SDL input method messages", ime_message as ImeMessage, &ORIGINAL_IME),
        // WM_IME_SETCONTEXT: SDL zeroes the flags through the lParam pointer.
        Point::keep(0x1764c7cusize, "4d 89 29", "SDL hides the input method windows"),
        // CoCreateInstance of TSF's thread manager fails: no UI-less mode.
        Point::write(TSF_CREATE, "ff 15 ac 26 40 00", "SDL TSF UI-less setup", "b8 05 40 00 80 90"),
        Point::jump(CARET, "48 8b 8f f0 01 00 00", "edit box caret position", caret_stub as *const () as usize, &CARET_RESUME),
        Point::keep(CARET_DONE, "48 85 c9", "edit box caret sprite"),
        Point::keep(0x15343b0usize, "48 8d 55 30 ff 50 58 8b 18 89 5d 38", "edit box caret offset"),
        // The box's click handling takes its place on screen.
        Point::keep(0x1534588usize, "48 8b 01 48 8d 55 30 ff 90 d8 00 00 00", "edit box place on screen").uses(&[edit_box::PLACE]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_text_goes_one_character_at_a_time_in_the_game_encoding() {
        assert_eq!(typed_chars("中a€"), [vec![0xe4, 0xb8, 0xad], vec![b'a'], vec![0x80]]);
        assert_eq!(typed_char(b"\xe4\xb8\xad\0"), "中".as_bytes());
        assert_eq!(typed_char(b"ab\0"), b"a");
        assert_eq!(typed_char(b"\0"), b"\0");
    }

    #[test]
    fn only_keys_the_input_method_took_are_held_back() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;
        assert!(taken_by_input_method(WM_KEYDOWN, VK_PROCESSKEY as usize));
        assert!(taken_by_input_method(WM_KEYUP, VK_PROCESSKEY as usize));
        assert!(!taken_by_input_method(WM_KEYDOWN, VK_RETURN as usize));
        assert!(!taken_by_input_method(WM_IME_COMPOSITION, VK_PROCESSKEY as usize));
    }

    #[test]
    fn composition_keeps_everything_but_the_result() {
        let flags = (GCS_COMPSTR | GCS_RESULTSTR | GCS_CURSORPOS) as isize;
        assert_eq!(without_result(flags), (GCS_COMPSTR | GCS_CURSORPOS) as isize);
    }

    #[test]
    fn the_caret_scales_to_window_pixels() {
        assert_eq!(caret_pixels((224, 203), 16, (1.5, 1.5)), (336, 304, 24));
        assert_eq!(caret_pixels((10, 10), 0, (1.0, 1.0)), (10, 10, 1));
    }
}
