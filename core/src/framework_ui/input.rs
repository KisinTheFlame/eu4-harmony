//! Mouse and keyboard for the framework UI. The game reads input through its
//! window procedure; the framework puts its own in front of it, and keeps a
//! message from the game when the framework UI takes it
//! (ADR framework-draws-its-own-ui): every input message while a modal window
//! is open, the input method's text included, and mouse messages over the
//! framework UI otherwise.
//!
//! Remote control's input (`crate::game::input`) comes through here too, as
//! the messages Windows would send ([`inject`]); a message Windows sends
//! itself is first asked whether it hands the cursor back to the player.
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
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

/// How far the wheel turns in a notch.
const WHEEL_DELTA: i32 = 120;

/// What the wheel turned of a notch since its last whole one, while a modal
/// window is open: a precision touchpad turns it by fractions of a notch.
static WHEEL_REST: AtomicI32 = AtomicI32::new(0);

/// The whole notches the wheel turned, `delta` in [`WHEEL_DELTA`]ths of one,
/// with what `rest` kept of a notch; `rest` keeps what is left. A turn the
/// other way starts from nothing.
fn notches(rest: &mut i32, delta: i32) -> i32 {
    if rest.signum() * delta.signum() < 0 {
        *rest = 0;
    }
    let turned = *rest + delta;
    *rest = turned % WHEEL_DELTA;
    turned / WHEEL_DELTA
}

/// The wheel's event for a turn of `delta`, if it made a whole notch. Only a
/// modal window scrolls, so what is left of a notch is kept only while one is
/// open.
fn wheel(delta: i32) -> Option<Event> {
    let mut rest = if super::modal() { WHEEL_REST.load(Ordering::Relaxed) } else { 0 };
    let turned = notches(&mut rest, delta);
    WHEEL_REST.store(rest, Ordering::Relaxed);
    (turned != 0).then_some(Event::Wheel(turned))
}

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
        WM_MOUSEWHEEL => wheel((wparam >> 16) as u16 as i16 as i32),
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

/// The class of the game's window, SDL's.
const GAME_CLASS: &str = "SDL_app";

/// Whether `window` is of the game's class.
unsafe fn of_game_class(window: HWND) -> bool {
    let mut class = [0u16; 64];
    let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    len > 0 && String::from_utf16_lossy(&class[..len as usize]) == GAME_CLASS
}

/// The game's window: a window of this process without an owner, which is
/// shown, or, in a hidden start, of the game's class.
unsafe extern "system" fn find(window: HWND, found: LPARAM) -> BOOL {
    let mut process = 0;
    GetWindowThreadProcessId(window, &mut process);
    let seen = IsWindowVisible(window) != 0 || (crate::hidden::hidden_start() && of_game_class(window));
    if process == GetCurrentProcessId() && seen && GetWindow(window, GW_OWNER).is_null() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_turned_by_fractions_of_a_notch_adds_them_up() {
        let mut rest = 0;
        // A precision touchpad's small turns, up.
        let turned: Vec<i32> = [40, 40, 30, 60, 8].iter().map(|d| notches(&mut rest, *d)).collect();
        assert_eq!(turned, [0, 0, 0, 1, 0]);
        assert_eq!(rest, 58);
        // A turn the other way starts from nothing.
        assert_eq!(notches(&mut rest, -60), 0);
        assert_eq!(notches(&mut rest, -60), -1);
        assert_eq!(rest, 0);
        // A mouse's notches, and more than one at once.
        assert_eq!(notches(&mut rest, 120), 1);
        assert_eq!(notches(&mut rest, -360), -3);
        assert_eq!(notches(&mut rest, -130), -1);
        assert_eq!(rest, -10);
    }
}
