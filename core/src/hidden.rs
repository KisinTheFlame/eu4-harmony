//! The hidden start (GLOSSARY): the game started with `-harmony_hidden` on
//! its command line runs with nothing of it on the player's desktop, and with
//! no sound unless `-harmony_sound` is there too (docs/hidden-start.md). The
//! switches are read once, as the framework starts; this module answers
//! whether the game is hidden now, and its points keep the game from showing
//! itself:
//!
//! - the window is made with `SDL_WINDOW_HIDDEN`, without which SDL shows it,
//!   and the application's Init shows it cloaked, as a tool window, without
//!   the focus: DWM draws nothing of a cloaked window and lets no click reach
//!   it, a tool window has no taskbar button and is not in Alt+Tab, and for
//!   the rest of Windows it is shown. A window not shown at all makes
//!   NVIDIA's Direct3D 9 driver keep 3 KB of the game's heap at most of its
//!   frames, 20 MB a minute and more (docs/research/headless.md); hiding it
//!   again conceals it so, and showing it ends that;
//! - the game's own error messages are left out, as `-suppresserrormessages`
//!   leaves them out;
//! - the input device moves remote control's held cursor instead of Windows'
//!   cursor;
//! - after a crash, the window is not minimized, which would put it on the
//!   taskbar, and the crash reporter is not started.
//!
//! Sound needs no point: before SDL's audio starts, the `dummy` driver is
//! chosen through the environment, which SDL reads then
//! (docs/research/headless.md).
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The switches on the game's command line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Switches {
    /// `-harmony_hidden`.
    pub hidden: bool,
    /// `-harmony_sound`, which only a hidden start looks at.
    pub sound: bool,
}

impl Switches {
    /// The switches among a command line's words, in any case.
    pub fn of(command_line: &str) -> Self {
        let has = |switch: &str| command_line.split_whitespace().any(|word| word.trim_matches('"').eq_ignore_ascii_case(switch));
        Self { hidden: has("-harmony_hidden"), sound: has("-harmony_sound") }
    }
}

static SWITCHES: OnceLock<Switches> = OnceLock::new();
/// Whether the game's window is hidden now.
static HIDDEN: AtomicBool = AtomicBool::new(false);

/// Reads the switches from the game's command line, once as the framework
/// starts, and turns the sound off for a hidden start without
/// `-harmony_sound`.
pub fn read_switches() {
    use windows_sys::Win32::System::Environment::SetEnvironmentVariableW;
    let switches = Switches::of(crate::paths::command_line());
    let _ = SWITCHES.set(switches);
    if !switches.hidden {
        return;
    }
    HIDDEN.store(true, Ordering::Relaxed);
    if switches.sound {
        log!("hidden start, with sound");
    } else {
        let name: Vec<u16> = "SDL_AUDIODRIVER\0".encode_utf16().collect();
        let value: Vec<u16> = "dummy\0".encode_utf16().collect();
        unsafe { SetEnvironmentVariableW(name.as_ptr(), value.as_ptr()) };
        log!("hidden start, without sound");
    }
}

/// Whether the game was started hidden.
pub fn hidden_start() -> bool {
    SWITCHES.get().is_some_and(|s| s.hidden)
}

/// Whether the game plays sound: a hidden start does only with
/// `-harmony_sound`.
pub fn sound() -> bool {
    SWITCHES.get().is_none_or(|s| !s.hidden || s.sound)
}

/// Whether the game's window is hidden now.
pub fn hidden() -> bool {
    HIDDEN.load(Ordering::Relaxed)
}

/// What [`show`] and [`hide`] refuse.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The game was not started hidden.
    NotHiddenStart,
    /// The game has no window yet.
    NoWindow,
}

/// Shows the game's window, without taking the focus; from then on the
/// player's mouse and keyboard reach it as usual.
pub fn show() -> Result<(), Refused> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    let window = window()?;
    if HIDDEN.swap(false, Ordering::Relaxed) {
        conceal(window, false);
        // Shown already, unless Init has not shown it yet.
        unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
        log!("hidden start: the window is shown");
    }
    Ok(())
}

/// Hides the game's window again, cloaked, holding the cursor as at a
/// hidden start.
pub fn hide() -> Result<(), Refused> {
    let window = window()?;
    if !HIDDEN.swap(true, Ordering::Relaxed) {
        crate::game::input::hold(None);
        conceal(window, true);
        log!("hidden start: the window is hidden again");
    }
    Ok(())
}

/// Conceals the window, or ends that: cloaked, a window is shown to
/// Windows and drawn by nobody, and as a tool window it has no taskbar
/// button. The taskbar looks at the style only as a window is shown, so a
/// window shown already is hidden and shown again, for a moment.
fn conceal(window: windows_sys::Win32::Foundation::HWND, on: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, IsWindowVisible, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };
    let value = i32::from(on);
    let result = unsafe { DwmSetWindowAttribute(window, DWMWA_CLOAK as u32, (&value as *const i32).cast(), 4) };
    if result != 0 {
        log!("hidden start: the window's cloak could not be {}: {result:#x}", if on { "put on" } else { "taken off" });
    }
    let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } as u32;
    let wanted = if on { style & !WS_EX_APPWINDOW | WS_EX_TOOLWINDOW } else { style & !WS_EX_TOOLWINDOW };
    if wanted != style {
        unsafe { SetWindowLongPtrW(window, GWL_EXSTYLE, wanted as isize) };
        if unsafe { IsWindowVisible(window) } != 0 {
            unsafe {
                ShowWindow(window, SW_HIDE);
                ShowWindow(window, SW_SHOWNOACTIVATE);
            }
        }
    }
}

fn window() -> Result<windows_sys::Win32::Foundation::HWND, Refused> {
    if !hidden_start() {
        return Err(Refused::NotHiddenStart);
    }
    crate::framework_ui::input::window().ok_or(Refused::NoWindow)
}

const SDL_WINDOW_FULLSCREEN: u32 = 0x1;
const SDL_WINDOW_SHOWN: u32 = 0x4;
/// SDL shows every window made without it, whatever `SDL_WINDOW_SHOWN` says.
const SDL_WINDOW_HIDDEN: u32 = 0x8;

/// Where the window is made: the flags it is made with are at `[rsp+0x28]`,
/// and this instruction (`mov edx, SDL_WINDOWPOS_CENTERED`) is the last
/// before the arguments are moved into place.
const WINDOW_MADE: usize = 0x14c40af;
const WINDOW_MADE_NEXT: usize = 0x14c40b4;
/// The application's Init: `ShowWindow(window, SW_SHOWDEFAULT)`.
const INIT_SHOWS: usize = 0x14c02ca;
const INIT_SHOWS_NEXT: usize = 0x14c02d0;
/// `ShowWindow`'s import slot.
const SHOW_WINDOW_IMPORT: usize = 0x1b66c38;
/// `main` stores whether `-suppresserrormessages` is on the command line.
const SUPPRESS_STORED: usize = 0x876b37;
const SUPPRESS_STORED_NEXT: usize = 0x876b3d;
/// The flag the game's error messages look at.
const SUPPRESS_ERROR_MESSAGES: usize = 0x2357b33;
/// The input device's slot `+0x40` (vtable `0x1da3530`): moves Windows'
/// cursor to a position in the window, through `SDL_WarpMouseInWindow`.
const WARP_CURSOR: usize = 0x162e4d0;
/// The pointer to SDL's window the warp reads.
const SDL_WINDOW_HOLDER: usize = 0x23495a8;
/// After a crash: `SDL_MinimizeWindow(window)`, before the window is
/// destroyed and the crash reporter started.
const CRASH_MINIMIZES: usize = 0x1654696;
const CRASH_MINIMIZES_NEXT: usize = 0x165469b;
/// `SDL_MinimizeWindow`'s jump into SDL.
const MINIMIZE_WINDOW: usize = 0x1735340;
/// Starts `CrashReporter.exe` and waits for it.
const START_CRASH_REPORTER: usize = 0x164f950;

crate::resume!(WINDOW_MADE_RESUME: WINDOW_MADE_AT = WINDOW_MADE_NEXT);
crate::resume!(INIT_SHOWS_RESUME: INIT_SHOWS_AT = INIT_SHOWS_NEXT);
crate::resume!(SUPPRESS_RESUME: SUPPRESS_AT = SUPPRESS_STORED_NEXT);
crate::resume!(CRASH_MINIMIZES_RESUME: CRASH_MINIMIZES_AT = CRASH_MINIMIZES_NEXT);

type WarpCursor = unsafe extern "C" fn(usize, *const [i32; 2]);
type MinimizeWindow = unsafe extern "C" fn(usize);
type StartCrashReporter = unsafe extern "C" fn(usize, usize) -> usize;

static ORIGINAL_WARP: Function<WarpCursor> = Function::new();
static MINIMIZE: Function<MinimizeWindow> = Function::new();
static ORIGINAL_REPORTER: Function<StartCrashReporter> = Function::new();

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(WINDOW_MADE, "ba 00 00 ff 2f", "window flags", window_made_stub as *const () as usize, &WINDOW_MADE_RESUME),
        Point::jump(INIT_SHOWS, "ff 15 68 69 6a 00", "init shows the window", init_shows_stub as *const () as usize, &INIT_SHOWS_RESUME)
            .uses(&[SHOW_WINDOW_IMPORT]),
        Point::jump(SUPPRESS_STORED, "88 05 f6 0f ae 01", "suppress error messages", suppress_stub as *const () as usize, &SUPPRESS_RESUME)
            .uses(&[SUPPRESS_ERROR_MESSAGES]),
        Point::replace(WARP_CURSOR, "40 53 48 83 ec 20 48 8b 05 cb b0 d1 00", "warp cursor", warp as WarpCursor, &ORIGINAL_WARP).uses(&[SDL_WINDOW_HOLDER]),
        Point::jump(CRASH_MINIMIZES, "e8 a5 0c 0e 00", "crash minimizes the window", crash_minimizes_stub as *const () as usize, &CRASH_MINIMIZES_RESUME),
        Point::keep(MINIMIZE_WINDOW, "48 ff 25 41 bc 87 00", "SDL minimize window").recorded(MINIMIZE.slot()),
        Point::replace(
            START_CRASH_REPORTER,
            "48 89 5c 24 18 48 89 4c 24 08 55 56 57 41 54",
            "start crash reporter",
            start_crash_reporter as StartCrashReporter,
            &ORIGINAL_REPORTER,
        ),
    ]
}

/// The flags the window is made with: `SDL_WINDOW_HIDDEN` in place of
/// `SDL_WINDOW_SHOWN` for a hidden start. A full screen device needs the focus, which a hidden game
/// never has, so a hidden start in full screen ends the game.
extern "C" fn window_flags(flags: u32) -> u32 {
    crate::guarded("window flags", || {
        if !hidden_start() {
            return flags;
        }
        if flags & SDL_WINDOW_FULLSCREEN != 0 {
            crate::fatal("a hidden start needs a windowed or borderless game, and the settings have it full screen");
        }
        flags & !SDL_WINDOW_SHOWN | SDL_WINDOW_HIDDEN
    })
}

/// [`window_flags`] for the flags at the game's `[rsp+0x28]`, which is
/// `[rbp+0x30]` in the frame; rax is the game's and stays.
#[unsafe(naked)]
extern "C" fn window_made_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov ecx, [rbp + 0x30]", call window_flags, "push rax", "mov eax, [rsp + 0x50]", "mov [rbp + 0x30], eax", "pop rax"),
        "mov edx, 0x2fff0000",
        "jmp qword ptr [rip + {next}]",
        window_flags = sym window_flags,
        next = sym WINDOW_MADE_AT,
    );
}

/// Init's `ShowWindow`: while the game is hidden, cloaked first and without
/// the focus.
extern "C" fn init_shows(window: usize, command: i32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    if hidden() {
        conceal(window as _, true);
        unsafe { ShowWindow(window as _, SW_SHOWNOACTIVATE) };
        log!("hidden start: the window is shown cloaked");
    } else {
        unsafe { ShowWindow(window as _, command) };
    }
}

#[unsafe(naked)]
extern "C" fn init_shows_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call init_shows),
        "jmp qword ptr [rip + {next}]",
        init_shows = sym init_shows,
        next = sym INIT_SHOWS_AT,
    );
}

/// Stores whether the game's error messages are left out: as the command
/// line asks, and always for a hidden start.
extern "C" fn suppress(asked: u8) {
    let on = asked != 0 || hidden_start();
    unsafe { *(crate::hooked_memory().at(SUPPRESS_ERROR_MESSAGES) as *mut u8) = on as u8 };
}

#[unsafe(naked)]
extern "C" fn suppress_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("movzx ecx, al", call suppress),
        "jmp qword ptr [rip + {next}]",
        suppress = sym suppress,
        next = sym SUPPRESS_AT,
    );
}

/// The input device moves Windows' cursor; while the game is hidden, it
/// moves remote control's held cursor instead, and Windows' stays where the
/// player has it.
unsafe extern "C" fn warp(device: usize, at: *const [i32; 2]) {
    if hidden() {
        let at = *at;
        crate::guarded("warp cursor", || crate::game::input::hold(Some(at)));
    } else {
        ORIGINAL_WARP.get()(device, at);
    }
}

/// After a crash, the window is minimized only when it is shown: minimizing a
/// hidden window puts it on the taskbar.
extern "C" fn crash_minimizes(window: usize) {
    if !hidden() {
        unsafe { MINIMIZE.get()(window) };
    }
}

#[unsafe(naked)]
extern "C" fn crash_minimizes_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call crash_minimizes),
        "jmp qword ptr [rip + {next}]",
        crash_minimizes = sym crash_minimizes,
        next = sym CRASH_MINIMIZES_AT,
    );
}

/// The crash reporter is a program with a window: a hidden start does not
/// start it. The crash's files are written before, either way. Runs inside
/// the game's crash handling, so it takes no lock.
unsafe extern "C" fn start_crash_reporter(handler: usize, folder: usize) -> usize {
    if hidden_start() {
        0
    } else {
        ORIGINAL_REPORTER.get()(handler, folder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switches_are_words_of_the_command_line_in_any_case() {
        let both = Switches::of(r#""E:\Steam\Europa Universalis IV\eu4.exe" -harmony_hidden -HARMONY_SOUND"#);
        assert_eq!(both, Switches { hidden: true, sound: true });
        assert_eq!(Switches::of(r#""E:\eu4.exe" "-harmony_hidden""#), Switches { hidden: true, sound: false });
        assert_eq!(Switches::of(r#""E:\harmony_hidden\eu4.exe" -harmony_hiddenx -debug"#), Switches::default());
    }
}
