//! Mouse and keyboard for the framework UI. The game reads input through its
//! window procedure; the framework puts its own in front of it, and keeps a
//! message from the game when the framework UI takes it (ADR 0013): every
//! input message while a modal window is open, the input method's text
//! included, and mouse messages over the framework UI otherwise.
//!
//! Remote control's input (`crate::game::input`) comes through here too, as
//! the messages Windows would send ([`inject`]); a message Windows sends
//! itself is first asked whether it hands the cursor back to the player.
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// What the framework UI hears from the window procedure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Move(f32, f32),
    Down(f32, f32),
    Up(f32, f32),
    /// The wheel turned, in notches (up is positive), with the mouse where
    /// it last moved.
    Wheel(i32),
    Escape,
}

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
/// A message [`inject`] hands over is going through the procedure.
static INJECTING: AtomicBool = AtomicBool::new(false);

/// Decides for each event whether the framework UI takes it.
pub type Handler = fn(Event) -> bool;
static HANDLER: std::sync::OnceLock<Handler> = std::sync::OnceLock::new();

fn position(lparam: LPARAM) -> (f32, f32) {
    ((lparam & 0xffff) as i16 as f32, ((lparam >> 16) & 0xffff) as i16 as f32)
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if !INJECTING.load(Ordering::Relaxed) && crate::guarded("input from Windows", || crate::game::input::real(message)) {
        return 0;
    }
    let handler = HANDLER.get().copied();
    let event = match message {
        WM_MOUSEMOVE => Some(Event::Move(position(lparam).0, position(lparam).1)),
        WM_LBUTTONDOWN => Some(Event::Down(position(lparam).0, position(lparam).1)),
        WM_LBUTTONUP => Some(Event::Up(position(lparam).0, position(lparam).1)),
        WM_MOUSEWHEEL => Some(Event::Wheel((wparam >> 16) as u16 as i16 as i32 / 120)),
        WM_KEYDOWN if wparam == 0x1b => Some(Event::Escape),
        _ => None,
    };
    let taken = match (handler, event) {
        (Some(h), Some(e)) => crate::guarded("framework UI input", || h(e)),
        _ => false,
    };
    // While a modal window is open the game gets no input at all.
    let modal = super::modal();
    let input = matches!(
        message,
        WM_MOUSEMOVE
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_LBUTTONDBLCLK
            | WM_RBUTTONDOWN
            | WM_RBUTTONUP
            | WM_RBUTTONDBLCLK
            | WM_MBUTTONDOWN
            | WM_MBUTTONUP
            | WM_MBUTTONDBLCLK
            | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL
            | WM_KEYDOWN
            | WM_KEYUP
            | WM_CHAR
            | WM_UNICHAR
            | WM_SYSKEYDOWN
            | WM_SYSKEYUP
    );
    if taken || (modal && input && message != WM_MOUSEMOVE) {
        return 0;
    }
    // Text the input method makes would reach the game's edit box through
    // SDL: Windows' own handling turns it into characters, which the modal
    // window keeps as it keeps typed ones.
    if modal && matches!(message, WM_IME_STARTCOMPOSITION | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION | WM_IME_CHAR) {
        return DefWindowProcW(window, message, wparam, lparam);
    }
    let original: WNDPROC = std::mem::transmute(ORIGINAL.load(Ordering::Acquire));
    CallWindowProcW(original, window, message, wparam, lparam)
}

unsafe extern "system" fn find(window: HWND, found: LPARAM) -> BOOL {
    let mut process = 0;
    GetWindowThreadProcessId(window, &mut process);
    if process == GetCurrentProcessId() && IsWindowVisible(window) != 0 && GetWindow(window, GW_OWNER).is_null() {
        *(found as *mut HWND) = window;
        return 0;
    }
    1
}

/// The game's window, once the framework's procedure is in front of the
/// game's.
pub fn window() -> Option<HWND> {
    let window = WINDOW.load(Ordering::Acquire);
    (window != 0).then_some(window as HWND)
}

/// Hands the window procedure a message as Windows would send it, on the
/// main thread: the framework UI has it first, then the game. False before
/// the procedure is in place.
pub fn inject(message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let Some(window) = window() else { return false };
    INJECTING.store(true, Ordering::Relaxed);
    unsafe { procedure(window, message, wparam, lparam) };
    INJECTING.store(false, Ordering::Relaxed);
    true
}

/// Puts the framework's window procedure in front of the game's, once the
/// game has its window. Returns whether it is in place.
pub fn attach(handler: Handler) -> bool {
    if WINDOW.load(Ordering::Acquire) != 0 {
        return true;
    }
    let _ = HANDLER.set(handler);
    let mut window: HWND = std::ptr::null_mut();
    unsafe { EnumWindows(Some(find), &mut window as *mut HWND as LPARAM) };
    if window.is_null() {
        return false;
    }
    // The game's procedure is known before a message can reach ours.
    let game = unsafe { GetWindowLongPtrW(window, GWLP_WNDPROC) };
    if game == 0 {
        return false;
    }
    ORIGINAL.store(game, Ordering::Release);
    let previous = unsafe { SetWindowLongPtrW(window, GWLP_WNDPROC, procedure as *const () as isize) };
    if previous == 0 {
        return false;
    }
    ORIGINAL.store(previous, Ordering::Release);
    WINDOW.store(window as isize, Ordering::Release);
    log!("framework UI: input attached to window {window:?}");
    true
}
