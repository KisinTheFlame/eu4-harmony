//! Window visibility and audio mute switches (GLOSSARY): `-harmony_hidden`
//! starts the game with nothing of its window on the player's desktop, and
//! `-harmony_mute` disables audio independently via SDL's `dummy` audio driver
//! (docs/game/program/headless.md). Both switches are read once as the
//! framework starts; an unattended run (`-harmony_hidden` or, in a test-games
//! build, `-harmony_test_game=<name>`) keeps dialogs and power throttling out
//! of the way and allows toggling visibility through remote control:
//!
//! - when `-harmony_hidden` is given, the window is made with
//!   `SDL_WINDOW_HIDDEN`, without which SDL shows it, and the application's
//!   Init shows it cloaked, as a tool window, without the focus: DWM draws
//!   nothing of a cloaked window and lets no click reach it, a tool window has
//!   no taskbar button and is not in Alt+Tab, and for the rest of Windows it
//!   is shown. A window not shown at all makes NVIDIA's Direct3D 9 driver keep
//!   3 KB of the game's heap at most of its frames, 20 MB a minute and more
//!   (docs/game/program/headless.md); hiding it again conceals it so, and
//!   showing it ends that;
//! - in an unattended run, the game's own error messages are left out, as
//!   `-suppresserrormessages` leaves them out, and after a crash the crash
//!   reporter is not started;
//! - while the window is hidden, the input device moves remote control's held
//!   cursor instead of Windows' cursor, and after a crash the window is not
//!   minimized, which would put it on the taskbar.
//!
//! Muting needs no point: when `-harmony_mute` is given, before SDL's audio
//! starts, the `dummy` driver is chosen through the environment, which SDL
//! reads then (docs/game/program/headless.md).
//!
//! Windows throttles a process whose windows the player cannot see
//! (EcoQoS): its threads run seldom and slowly, and its timer resolution
//! requests are ignored, so a sleep of a millisecond takes a tick of 15.6 ms.
//! A hidden game got 5% of a core where it needed a whole one
//! (docs/research/performance-evidence.md). So an unattended run opts the
//! process out of both, once as the framework starts. A game shown at startup
//! or later stays opted out: it is a game started for remote control or a
//! test, which is to run as fast when its window is behind others, and a
//! shown window is throttled by Windows as any other then anyway.
use crate::base::called_by_game;
use crate::native::points::{Function, Point};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The visibility, audio and test-game switches on the game's command line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Switches {
    /// `-harmony_hidden`: start with the game window hidden.
    pub hidden: bool,
    /// `-harmony_mute`: silence audio via SDL's `dummy` driver.
    pub mute: bool,
    /// `-harmony_test_game=<name>`: isolated test game (only in `dev-test-games` builds).
    pub test_game: bool,
}

impl Switches {
    /// The switches among a command line's words, in any case and with one
    /// dash or two, as the game writes them again when it restarts.
    pub fn of(command_line: &str) -> Self {
        let has = |switch: &str| command_line.split_whitespace().any(|word| eu4_harmony_folders::same_switch(word.trim_matches('"'), switch));
        #[cfg(feature = "dev-test-games")]
        let test_game = eu4_harmony_folders::switch_value(command_line, "-harmony_test_game").is_some();
        #[cfg(not(feature = "dev-test-games"))]
        let test_game = false;
        Self { hidden: has("-harmony_hidden"), mute: has("-harmony_mute"), test_game }
    }

    /// Whether this run is unattended (a hidden start or a test game).
    pub fn unattended(self) -> bool {
        self.hidden || self.test_game
    }
}

static SWITCHES: OnceLock<Switches> = OnceLock::new();
/// Whether the game created its window in exclusive fullscreen mode.
static FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub(crate) struct HiddenHooks {
    pub(crate) on_early_sync: fn(),
}

static HOOKS: OnceLock<HiddenHooks> = OnceLock::new();

pub(crate) fn set_hooks(hooks: HiddenHooks) {
    let _ = HOOKS.set(hooks);
}

/// Reads the switches from the game's command line, once as the framework
/// starts, turning audio off for `-harmony_mute`, starting hidden for
/// `-harmony_hidden`, and opting unattended runs out of Windows power throttling.
pub fn read_switches() {
    use windows_sys::Win32::System::Environment::SetEnvironmentVariableW;
    let switches = Switches::of(crate::base::paths::command_line());
    let _ = SWITCHES.set(switches);
    let is_unattended = switches.unattended();
    crate::base::set_unattended(is_unattended);
    if switches.hidden {
        crate::base::set_hidden(true);
        log!("hidden start: the window starts hidden");
    }
    if switches.mute {
        let name: Vec<u16> = "SDL_AUDIODRIVER\0".encode_utf16().collect();
        let value: Vec<u16> = "dummy\0".encode_utf16().collect();
        unsafe { SetEnvironmentVariableW(name.as_ptr(), value.as_ptr()) };
        log!("audio muted (-harmony_mute)");
    }
    if is_unattended {
        full_speed();
    }
}

/// What a hidden game asks of Windows' power throttling: the execution
/// speed and the timer resolution controlled, and neither throttled.
fn not_throttled() -> windows_sys::Win32::System::Threading::PROCESS_POWER_THROTTLING_STATE {
    use windows_sys::Win32::System::Threading::{
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        PROCESS_POWER_THROTTLING_STATE,
    };
    PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        StateMask: 0,
    }
}

/// Opts the process out of Windows' power throttling; a refusal is logged.
fn full_speed() {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation};
    let state = not_throttled();
    let size = std::mem::size_of_val(&state) as u32;
    if unsafe { SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, &state as *const _ as *const _, size) } != 0 {
        log!("hidden start: opted out of Windows' power throttling");
    } else {
        log!("hidden start: Windows' power throttling stays: {}", std::io::Error::last_os_error());
    }
}

/// Whether the game was started with `-harmony_hidden`.
pub fn hidden_start() -> bool {
    SWITCHES.get().is_some_and(|s| s.hidden)
}

/// Whether this run is unattended (`-harmony_hidden` or `-harmony_test_game`).
pub fn unattended() -> bool {
    SWITCHES.get().is_some_and(|s| s.unattended())
}

/// Whether the game plays sound (`false` when started with `-harmony_mute`).
pub fn sound() -> bool {
    SWITCHES.get().is_none_or(|s| !s.mute)
}

/// Whether the game's window is hidden now.
pub fn hidden() -> bool {
    crate::base::is_hidden()
}

/// What [`show`] and [`hide`] refuse.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The game was neither started hidden nor started as a test game.
    NotControllable,
    /// The game's window is exclusive fullscreen and cannot be hidden safely.
    Fullscreen,
    /// The game has no window yet.
    NoWindow,
}

/// Shows the game's window, without taking the focus; from then on the
/// player's mouse and keyboard reach it as usual.
pub fn show() -> Result<(), Refused> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    let window = window()?;
    if crate::base::swap_hidden(false) {
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
    if !crate::base::swap_hidden(true) {
        crate::render::input::hold(None);
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
    if !unattended() {
        return Err(Refused::NotControllable);
    }
    if FULLSCREEN.load(Ordering::Relaxed) {
        return Err(Refused::Fullscreen);
    }
    crate::base::game_window().ok_or(Refused::NoWindow)
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

resume!(WINDOW_MADE_RESUME: WINDOW_MADE_AT = WINDOW_MADE_NEXT);
resume!(INIT_SHOWS_RESUME: INIT_SHOWS_AT = INIT_SHOWS_NEXT);
resume!(SUPPRESS_RESUME: SUPPRESS_AT = SUPPRESS_STORED_NEXT);
resume!(CRASH_MINIMIZES_RESUME: CRASH_MINIMIZES_AT = CRASH_MINIMIZES_NEXT);

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
/// `SDL_WINDOW_SHOWN` when starting hidden. A full screen device needs the
/// focus, which a hidden game never has, so a hidden start in full screen ends
/// the game.
#[called_by_game]
extern "C" fn window_flags(flags: u32) -> u32 {
    let is_fullscreen = flags & SDL_WINDOW_FULLSCREEN != 0;
    FULLSCREEN.store(is_fullscreen, Ordering::Relaxed);
    if !hidden() {
        return flags;
    }
    if is_fullscreen {
        crate::base::fatal("a hidden start needs a windowed or borderless game, and the settings have it full screen");
    }
    flags & !SDL_WINDOW_SHOWN | SDL_WINDOW_HIDDEN
}

/// [`window_flags`] for the flags at the game's `[rsp+0x28]`, which is
/// `[rbp+0x30]` in the frame; rax is the game's and stays.
#[unsafe(naked)]
extern "C" fn window_made_stub() {
    std::arch::naked_asm!(
        call_rust!("mov ecx, [rbp + 0x30]", call window_flags, "push rax", "mov eax, [rsp + 0x50]", "mov [rbp + 0x30], eax", "pop rax"),
        "mov edx, 0x2fff0000",
        "jmp qword ptr [rip + {next}]",
        window_flags = sym window_flags,
        next = sym WINDOW_MADE_AT,
    );
}

/// Init's `ShowWindow`: while the game is hidden, cloaked first and without
/// the focus.
#[called_by_game]
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
        call_rust!(call init_shows),
        "jmp qword ptr [rip + {next}]",
        init_shows = sym init_shows,
        next = sym INIT_SHOWS_AT,
    );
}

/// Stores whether the game's error messages are left out: as the command
/// line asks, and always in an unattended run. Also runs Harmony's early proxy
/// sync here (step 3 of `main`, outside the loader lock and before Steam,
/// worker pools or the game window are created).
#[called_by_game]
extern "C" fn suppress(asked: u8) {
    let on = asked != 0 || unattended();
    unsafe { *(crate::native::hooked_memory().at(SUPPRESS_ERROR_MESSAGES) as *mut u8) = on as u8 };
    crate::base::guarded("proxy sync", || {
        if let Some(h) = HOOKS.get() {
            (h.on_early_sync)();
        }
    });
}

#[unsafe(naked)]
extern "C" fn suppress_stub() {
    std::arch::naked_asm!(
        call_rust!("movzx ecx, al", call suppress),
        "jmp qword ptr [rip + {next}]",
        suppress = sym suppress,
        next = sym SUPPRESS_AT,
    );
}

/// The input device moves Windows' cursor; while the game is hidden, it
/// moves remote control's held cursor instead, and Windows' stays where the
/// player has it.
#[called_by_game]
unsafe extern "C" fn warp(device: usize, at: *const [i32; 2]) {
    if hidden() {
        let at = *at;
        crate::base::guarded("warp cursor", || {
            crate::render::input::hold(Some(at));
        });
    } else {
        ORIGINAL_WARP.get()(device, at);
    }
}

/// After a crash, the window is minimized only when it is shown: minimizing a
/// hidden window puts it on the taskbar.
#[called_by_game]
extern "C" fn crash_minimizes(window: usize) {
    if !hidden() {
        unsafe { MINIMIZE.get()(window) };
    }
}

#[unsafe(naked)]
extern "C" fn crash_minimizes_stub() {
    std::arch::naked_asm!(
        call_rust!(call crash_minimizes),
        "jmp qword ptr [rip + {next}]",
        crash_minimizes = sym crash_minimizes,
        next = sym CRASH_MINIMIZES_AT,
    );
}

/// The crash reporter is a program with a window: an unattended run does not
/// start it. The crash's files are written before, either way. Runs inside
/// the game's crash handling, so it takes no lock.
#[called_by_game]
unsafe extern "C" fn start_crash_reporter(handler: usize, folder: usize) -> usize {
    if unattended() {
        0
    } else {
        ORIGINAL_REPORTER.get()(handler, folder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_game_asks_for_neither_throttling() {
        let state = not_throttled();
        assert_eq!((state.Version, state.ControlMask, state.StateMask), (1, 1 | 4, 0));
    }

    #[test]
    fn the_switches_are_words_of_the_command_line_in_any_case() {
        let both = Switches::of(r#""E:\Steam\Europa Universalis IV\eu4.exe" -harmony_hidden -HARMONY_MUTE"#);
        assert_eq!(both, Switches { hidden: true, mute: true, test_game: false });
        assert!(both.unattended());

        assert_eq!(Switches::of(r#""E:\eu4.exe" "-harmony_hidden""#), Switches { hidden: true, mute: false, test_game: false });
        assert_eq!(Switches::of(r#""E:\eu4.exe" -harmony_mute"#), Switches { hidden: false, mute: true, test_game: false });
        assert!(!Switches::of(r#""E:\eu4.exe" -harmony_mute"#).unattended());
        assert_eq!(Switches::of(r#""E:\harmony_hidden\eu4.exe" -harmony_hiddenx -harmony_mutex -debug"#), Switches::default());
        assert_eq!(
            Switches::of(r#"--E:\eu4.exe --harmony_hidden --harmony_mute --userdir="E:\u" -skiplauncher"#),
            Switches { hidden: true, mute: true, test_game: false }
        );

        let test_game = Switches::of(r#""E:\eu4.exe" -harmony_mute -harmony_test_game=qa"#);
        #[cfg(feature = "dev-test-games")]
        assert_eq!(test_game, Switches { hidden: false, mute: true, test_game: true });
        #[cfg(feature = "dev-test-games")]
        assert!(test_game.unattended());
        #[cfg(not(feature = "dev-test-games"))]
        assert_eq!(test_game, Switches { hidden: false, mute: true, test_game: false });
        #[cfg(not(feature = "dev-test-games"))]
        assert!(!test_game.unattended());
    }
}
