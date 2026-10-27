//! Mouse and keyboard for the framework UI. The game reads input through its
//! window procedure; the framework puts its own in front of it, and keeps a
//! message from the game when the framework UI takes it (ADR 0013): every
//! message while a modal window is open, and mouse messages over the framework
//! UI otherwise.
use std::sync::atomic::{AtomicIsize, Ordering};
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
    Escape,
}

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static ORIGINAL: AtomicIsize = AtomicIsize::new(0);

/// Decides for each event whether the framework UI takes it.
pub type Handler = fn(Event) -> bool;
static HANDLER: std::sync::OnceLock<Handler> = std::sync::OnceLock::new();

fn position(lparam: LPARAM) -> (f32, f32) {
    ((lparam & 0xffff) as i16 as f32, ((lparam >> 16) & 0xffff) as i16 as f32)
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handler = HANDLER.get().copied();
    let event = match message {
        WM_MOUSEMOVE => Some(Event::Move(position(lparam).0, position(lparam).1)),
        WM_LBUTTONDOWN => Some(Event::Down(position(lparam).0, position(lparam).1)),
        WM_LBUTTONUP => Some(Event::Up(position(lparam).0, position(lparam).1)),
        WM_KEYDOWN if wparam == 0x1b => Some(Event::Escape),
        _ => None,
    };
    let taken = match (handler, event) {
        (Some(h), Some(e)) => crate::guarded("framework UI input", || h(e)),
        _ => false,
    };
    // While a modal window is open the game gets no input at all.
    let modal = super::modal();
    let input = matches!(message, WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_MOUSEWHEEL | WM_KEYDOWN | WM_KEYUP | WM_CHAR | WM_SYSKEYDOWN | WM_SYSKEYUP);
    if taken || (modal && input && message != WM_MOUSEMOVE) {
        return 0;
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
    let previous = unsafe { SetWindowLongPtrW(window, GWLP_WNDPROC, procedure as *const () as isize) };
    if previous == 0 {
        return false;
    }
    ORIGINAL.store(previous, Ordering::Release);
    WINDOW.store(window as isize, Ordering::Release);
    log!("framework UI: input attached to window {window:?}");
    true
}
