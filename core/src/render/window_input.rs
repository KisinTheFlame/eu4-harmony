//! Mouse and keyboard for the framework UI. The game reads input through its
//! window procedure; the framework puts its own in front of it, and keeps a
//! message from the game when the framework UI takes it
//! (ADR framework-draws-its-own-ui): every input message while a modal window
//! is open, the input method's text included, and mouse messages over the
//! framework UI otherwise.
//!
//! Remote control's input (`crate::capabilities::game::input`) comes through here too, as
//! the messages Windows would send ([`inject`]); a message Windows sends
//! itself is first asked whether it hands the cursor back to the player.
use crate::base::called_by_game;
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
    /// The game window lost keyboard focus (`WM_KILLFOCUS`): cancel any
    /// in-progress drag or slider capture so controls cannot stick.
    FocusLost,
}

static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
/// A message [`inject`] hands over is going through the procedure.
static INJECTING: AtomicBool = AtomicBool::new(false);

/// Decides for each event whether the framework UI takes it.
pub type Handler = fn(Event) -> bool;
static HANDLER: std::sync::OnceLock<Handler> = std::sync::OnceLock::new();

pub(crate) struct InputHooks {
    pub(crate) is_modal: fn() -> bool,
    pub(crate) on_wheel: fn(i16),
}

static HOOKS: std::sync::OnceLock<InputHooks> = std::sync::OnceLock::new();

pub(crate) fn set_hooks(hooks: InputHooks) {
    let _ = HOOKS.set(hooks);
}

/// How far the wheel turns in a notch.
const WHEEL_DELTA: i32 = 120;

/// What the wheel turned of a notch since its last whole one, while a modal
/// window is open: a precision touchpad turns it by fractions of a notch.
static WHEEL_REST: AtomicI32 = AtomicI32::new(0);
/// Whether a modal consumed `VK_ESCAPE` on key-down, so its auto-repeats and
/// matching key-up are swallowed rather than leaking to the in-game menu.
static ESCAPE_SWALLOWED: AtomicBool = AtomicBool::new(false);
/// Whether the framework UI swallowed `WM_LBUTTONDOWN`, so its matching
/// `WM_LBUTTONUP` is swallowed while a game drag started before a modal opened
/// still receives its release.
static LBUTTON_SWALLOWED: AtomicBool = AtomicBool::new(false);

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

/// What the window procedure should do with a Windows message after the
/// framework UI has been offered its translated event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MessageRoute {
    /// Consume the message and return `0` without passing it to the game.
    Swallow,
    /// Pass IME composition messages to `DefWindowProcW` while a modal window
    /// is open so typed text never reaches SDL.
    DefaultProc,
    /// Forward the message to the game's original window procedure.
    ForwardToGame,
}

/// Translates a raw Windows message into a framework UI [`Event`], resetting
/// accumulated precision-wheel fractions whenever no modal window is open.
fn translate_message(message: u32, wparam: WPARAM, lparam: LPARAM, wheel_rest: &mut i32, modal: bool) -> Option<Event> {
    match message {
        WM_MOUSEMOVE => Some(Event::Move(position(lparam).0, position(lparam).1)),
        WM_LBUTTONDOWN => Some(Event::Down(position(lparam).0, position(lparam).1)),
        WM_LBUTTONUP => Some(Event::Up(position(lparam).0, position(lparam).1)),
        WM_MOUSEWHEEL => {
            if !modal {
                *wheel_rest = 0;
            }
            let turned = notches(wheel_rest, (wparam >> 16) as u16 as i16 as i32);
            (turned != 0).then_some(Event::Wheel(turned))
        }
        WM_KEYDOWN if wparam == 0x1b => Some(Event::Escape),
        _ => None,
    }
}

/// Decides whether `message` is swallowed, sent to `DefWindowProcW`, or
/// forwarded to the game's window procedure.
fn route_message(message: u32, taken: bool, modal: bool) -> MessageRoute {
    // System key shortcuts (such as Alt+F4 or Alt+Space) stay responsive via
    // DefWindowProcW while a modal is open, without leaking to game hotkeys.
    if modal && matches!(message, WM_SYSKEYDOWN | WM_SYSKEYUP) {
        return MessageRoute::DefaultProc;
    }
    // Text the input method makes would reach the game's edit box through
    // SDL: Windows' own handling turns it into characters, which the modal
    // window keeps as it keeps typed ones.
    if modal && matches!(message, WM_IME_STARTCOMPOSITION | WM_IME_COMPOSITION | WM_IME_ENDCOMPOSITION | WM_IME_CHAR) {
        return MessageRoute::DefaultProc;
    }
    // Key-up and non-swallowed mouse-up messages are forwarded even while a
    // modal is open so keys/buttons held before the modal appeared never stick
    // inside SDL. Escape key-up and swallowed left-button releases are handled
    // explicitly in `dispatch_message`.
    let modal_swallowed = matches!(
        message,
        WM_LBUTTONDOWN
            | WM_LBUTTONDBLCLK
            | WM_RBUTTONDOWN
            | WM_RBUTTONDBLCLK
            | WM_MBUTTONDOWN
            | WM_MBUTTONDBLCLK
            | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL
            | WM_KEYDOWN
            | WM_CHAR
            | WM_UNICHAR
    );
    if taken || (modal && modal_swallowed) {
        return MessageRoute::Swallow;
    }
    MessageRoute::ForwardToGame
}

fn position(lparam: LPARAM) -> (f32, f32) {
    ((lparam & 0xffff) as i16 as f32, ((lparam >> 16) & 0xffff) as i16 as f32)
}

/// Per-window message pairing state tracked across `procedure` calls.
#[derive(Default)]
struct InputState {
    wheel_rest: i32,
    escape_swallowed: bool,
    lbutton_swallowed: bool,
}

/// Handles Escape key-repeat/key-up suppression, left-button press/release
/// pairing, and focus-loss cancellation around [`translate_message`] and
/// [`route_message`].
fn dispatch_message(
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    state: &mut InputState,
    modal_before: bool,
    mut handle: impl FnMut(Event) -> (bool, bool),
) -> MessageRoute {
    if message == WM_KILLFOCUS {
        state.escape_swallowed = false;
        state.lbutton_swallowed = false;
        let _ = handle(Event::FocusLost);
        return MessageRoute::ForwardToGame;
    }
    if wparam == 0x1b {
        if message == WM_KEYDOWN && state.escape_swallowed {
            return MessageRoute::Swallow;
        }
        if message == WM_KEYUP && state.escape_swallowed {
            state.escape_swallowed = false;
            return MessageRoute::Swallow;
        }
    }
    let event = translate_message(message, wparam, lparam, &mut state.wheel_rest, modal_before);
    let (taken, modal_after) = match event {
        Some(e) => handle(e),
        None => (false, modal_before),
    };
    if message == WM_LBUTTONUP {
        if state.lbutton_swallowed {
            state.lbutton_swallowed = false;
            return MessageRoute::Swallow;
        }
        return MessageRoute::ForwardToGame;
    }
    let route = route_message(message, taken, modal_after);
    if message == WM_KEYDOWN && wparam == 0x1b && route == MessageRoute::Swallow {
        state.escape_swallowed = true;
    }
    if message == WM_LBUTTONDOWN && route == MessageRoute::Swallow {
        state.lbutton_swallowed = true;
    }
    route
}

#[called_by_game]
unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if !INJECTING.load(Ordering::Relaxed) && crate::base::guarded("input from Windows", || super::input::real(message)) {
        return 0;
    }
    let handler = HANDLER.get().copied();
    let is_modal = || HOOKS.get().is_some_and(|h| (h.is_modal)());
    let mut state = InputState {
        wheel_rest: WHEEL_REST.load(Ordering::Relaxed),
        escape_swallowed: ESCAPE_SWALLOWED.load(Ordering::Relaxed),
        lbutton_swallowed: LBUTTON_SWALLOWED.load(Ordering::Relaxed),
    };
    let modal_before = is_modal();
    let route = dispatch_message(message, wparam, lparam, &mut state, modal_before, |e| {
        let taken = match handler {
            Some(h) => crate::base::guarded("framework UI input", || h(e)),
            None => false,
        };
        (taken, is_modal())
    });
    WHEEL_REST.store(state.wheel_rest, Ordering::Relaxed);
    ESCAPE_SWALLOWED.store(state.escape_swallowed, Ordering::Relaxed);
    LBUTTON_SWALLOWED.store(state.lbutton_swallowed, Ordering::Relaxed);
    // While a modal window is open the game gets no input at all.
    match route {
        MessageRoute::Swallow => 0,
        MessageRoute::DefaultProc => DefWindowProcW(window, message, wparam, lparam),
        MessageRoute::ForwardToGame => {
            if message == WM_MOUSEWHEEL {
                crate::base::guarded("wheel reversal reset", || {
                    if let Some(h) = HOOKS.get() {
                        (h.on_wheel)((wparam >> 16) as u16 as i16);
                    }
                });
            }
            let original: WNDPROC = std::mem::transmute(ORIGINAL.load(Ordering::Acquire));
            CallWindowProcW(original, window, message, wparam, lparam)
        }
    }
}

/// The class of the game's window, SDL's.
const GAME_CLASS: &str = "SDL_app";

/// Whether `window` is of the game's class.
unsafe fn of_game_class(window: HWND) -> bool {
    let mut class = [0u16; 64];
    let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    len > 0 && String::from_utf16_lossy(&class[..len as usize]) == GAME_CLASS
}

struct FoundWindows {
    game: HWND,
    fallback: HWND,
}

/// The game's window: prefers an unowned window of this process with the game's
/// `"SDL_app"` class (visible, or hidden in an unattended run), falling back to
/// any visible unowned window of this process only if no `"SDL_app"` window is
/// found.
#[called_by_game]
unsafe extern "system" fn find(window: HWND, found: LPARAM) -> BOOL {
    let mut process = 0;
    GetWindowThreadProcessId(window, &mut process);
    if process != GetCurrentProcessId() || !GetWindow(window, GW_OWNER).is_null() {
        return 1;
    }
    let visible = IsWindowVisible(window) != 0;
    let is_game = of_game_class(window);
    let out = &mut *(found as *mut FoundWindows);
    if is_game && (visible || crate::base::unattended()) {
        out.game = window;
        return 0;
    }
    if visible && out.fallback.is_null() {
        out.fallback = window;
    }
    1
}

/// The game's window, once the framework's procedure is in front of the
/// game's.
pub fn window() -> Option<HWND> {
    crate::base::game_window()
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
    if crate::base::game_window().is_some() {
        return true;
    }
    let _ = HANDLER.set(handler);
    let mut found = FoundWindows { game: std::ptr::null_mut(), fallback: std::ptr::null_mut() };
    unsafe { EnumWindows(Some(find), &mut found as *mut FoundWindows as LPARAM) };
    let window = if !found.game.is_null() { found.game } else { found.fallback };
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
    crate::base::set_game_window(window);
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

    #[test]
    fn modal_routing_swallows_input_defers_ime_and_preserves_unconsumed_mouse_move() {
        assert_eq!(route_message(WM_LBUTTONDOWN, false, true), MessageRoute::Swallow);
        assert_eq!(route_message(WM_KEYDOWN, false, true), MessageRoute::Swallow);
        assert_eq!(route_message(WM_CHAR, false, true), MessageRoute::Swallow);
        assert_eq!(route_message(WM_KEYUP, false, true), MessageRoute::ForwardToGame);
        assert_eq!(route_message(WM_RBUTTONUP, false, true), MessageRoute::ForwardToGame);
        assert_eq!(route_message(WM_MBUTTONUP, false, true), MessageRoute::ForwardToGame);
        assert_eq!(route_message(WM_SYSKEYDOWN, false, true), MessageRoute::DefaultProc);
        assert_eq!(route_message(WM_SYSKEYUP, false, true), MessageRoute::DefaultProc);
        assert_eq!(route_message(WM_MOUSEMOVE, false, true), MessageRoute::ForwardToGame);
        assert_eq!(route_message(WM_MOUSEMOVE, true, true), MessageRoute::Swallow);

        for ime in [WM_IME_STARTCOMPOSITION, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_CHAR] {
            assert_eq!(route_message(ime, false, true), MessageRoute::DefaultProc);
            assert_eq!(route_message(ime, false, false), MessageRoute::ForwardToGame);
        }

        assert_eq!(route_message(WM_LBUTTONDOWN, true, false), MessageRoute::Swallow);
        assert_eq!(route_message(WM_LBUTTONDOWN, false, false), MessageRoute::ForwardToGame);
    }

    #[test]
    fn wheel_translation_keeps_fractional_remainder_only_while_modal() {
        let mut rest = 0;
        let step = (60u16 as WPARAM) << 16;
        assert_eq!(translate_message(WM_MOUSEWHEEL, step, 0, &mut rest, true), None);
        assert_eq!(rest, 60);
        // Closing the modal clears stale remainder before accumulating again.
        assert_eq!(translate_message(WM_MOUSEWHEEL, step, 0, &mut rest, false), None);
        assert_eq!(rest, 60);
        assert_eq!(translate_message(WM_MOUSEWHEEL, step, 0, &mut rest, true), Some(Event::Wheel(1)));
        assert_eq!(rest, 0);
    }

    #[test]
    fn escape_closing_a_modal_swallows_repeats_and_matching_key_up() {
        let mut state = InputState::default();
        // First Escape closes the modal and is swallowed.
        assert_eq!(dispatch_message(WM_KEYDOWN, 0x1b, 0, &mut state, true, |_| (true, false)), MessageRoute::Swallow);
        assert!(state.escape_swallowed);
        // Auto-repeat before release stays swallowed without invoking the handler.
        assert_eq!(dispatch_message(WM_KEYDOWN, 0x1b, 1 << 30, &mut state, false, |_| panic!("repeat must be suppressed")), MessageRoute::Swallow);
        assert!(state.escape_swallowed);
        // Matching key-up is swallowed and resets state.
        assert_eq!(dispatch_message(WM_KEYUP, 0x1b, 0, &mut state, false, |_| (false, false)), MessageRoute::Swallow);
        assert!(!state.escape_swallowed);
        // Next Escape reaches the game (e.g. to close the in-game menu).
        assert_eq!(dispatch_message(WM_KEYDOWN, 0x1b, 0, &mut state, false, |_| (false, false)), MessageRoute::ForwardToGame);
        assert!(!state.escape_swallowed);
        // Left-button release without a swallowed press reaches the game even while modal is open.
        assert_eq!(dispatch_message(WM_LBUTTONUP, 0, 0, &mut state, true, |_| (true, true)), MessageRoute::ForwardToGame);
        // Left-button press swallowed by modal pairs with its matching release.
        assert_eq!(dispatch_message(WM_LBUTTONDOWN, 0, 0, &mut state, true, |_| (true, true)), MessageRoute::Swallow);
        assert!(state.lbutton_swallowed);
        assert_eq!(dispatch_message(WM_LBUTTONUP, 0, 0, &mut state, true, |_| (true, true)), MessageRoute::Swallow);
        assert!(!state.lbutton_swallowed);
        // Losing focus clears pending suppressions and notifies the framework UI.
        state.escape_swallowed = true;
        state.lbutton_swallowed = true;
        let mut got_focus_lost = false;
        assert_eq!(
            dispatch_message(WM_KILLFOCUS, 0, 0, &mut state, true, |e| {
                got_focus_lost = e == Event::FocusLost;
                (false, true)
            }),
            MessageRoute::ForwardToGame
        );
        assert!(got_focus_lost);
        assert!(!state.escape_swallowed);
        assert!(!state.lbutton_swallowed);
    }
}
