//! The drawing thread (GLOSSARY; docs/plans/drawing-thread.md, T1): the
//! in-game frame's drawing done by a thread of the framework's, at a pace of
//! its own, while the main thread goes on with the day.
//!
//! The in-game frame calls its drawing (`0x8183b0`) last, right after the
//! GUI's update. With the drawing thread on, the main thread does there only
//! what of the drawing reads the world or draws into the interface's textures
//! ([`prepare`]); then it notes the frame as prepared, and goes on. The
//! drawing thread draws the prepared frame again and again, at the display's
//! refresh rate or with no limit, through the game's own drawing, whose first
//! steps it skips, with the map icons moved to its camera ([`icons`]).
//!
//! The two take turns through the presentation lock (GLOSSARY), a lock that
//! serves whoever asked first. The drawing thread holds it for each frame it
//! draws, but for Direct3D's Present, which may wait for the display. The
//! main thread holds it through each application frame, but lets it go while
//! the daily update runs, taking it again for a frame inside the day, at the
//! day's end, and at each crossing (GLOSSARY) the day reaches, which keeps it
//! until the next of the game's checks for a frame inside a day. So the
//! interface's own objects are never used by both at once ([`turns`]: the
//! hooks here tell it the moments). After a crossing
//! the drawing thread draws nothing until the next update, which finishes
//! what the crossing began ([`crossings`]).
//!
//! The day's calls into the interface that answer nothing the day reads are
//! deferred to the day's end or the next application frame, so that the day
//! neither waits for the lock there nor holds the frames back ([`deferred`]).
//!
//! The game's frames are paced by Present; a frame that runs no day waits
//! for the drawing thread to draw the last one instead.
//!
//! The main thread draws a frame itself, as the game does, when the
//! drawing thread is off, when the map's names are to be made again (which
//! reads the countries), when the console's debug text is on (which works a
//! country's values out as it is drawn), and while the framework holds frames
//! back.
mod course;
mod crossings;
mod deferred;
mod icons;
mod lock_order;
mod marks;
mod prepare;
mod tooltip;
mod turns;

pub(crate) use crossings::SCENE_BEGINS;

use crate::called_by_game;
use crate::hooks::Call;
use crate::memory::Memory;
use crate::points::{GameFunction, Point};
use crate::speed::SpeedUp;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};
use turns::{At, DrawingSide, MainSide};

pub const SPEED_UP: SpeedUp = SpeedUp { name: "drawing-thread", group: "frames", points, checkable: false };

/// The in-game frame's call of its drawing: `mov rcx, r13; call 0x8183b0`,
/// `r13` the in-game idler.
const FRAME_DRAWS: usize = 0x815919;
const FRAME_DRAWN: usize = 0x815921;
/// The drawing, given the in-game idler.
const DRAWING: GameFunction<1> = GameFunction::at(0x8183b0);
/// In the drawing: `mov rax, [settings]`, where its first steps begin, and
/// where they end (`mov rcx, [r15 + 0x11f8]`).
const HEAD: usize = 0x8185e7;
const HEAD_GOES_ON: usize = 0x8185ee;
const HEAD_END: usize = 0x818661;
/// The settings' global, which the head's first instruction reads.
const SETTINGS: usize = 0x242bde8;
/// The application, and its graphical map.
const APP: usize = 0x242bf50;
const APP_MAP: usize = 0x360;
/// The application's current idler.
const APP_IDLER: usize = 0x40;
/// The console's debug country, whose values the debug text works out.
const DEBUG_COUNTRY: usize = 0x2340040;
/// The application's frame: the application and whether it is a normal one
/// (0 for a frame inside a day).
const APP_FRAME: usize = 0x14c25a0;
/// What ends the application's run after a frame (`0x14c2d00`, its loop's
/// checks at `0x14c2dd0` and `0x14c2e18`): `+0x83`, set when the game quits,
/// and `+0x2f8`, the event pump's answer that it is to (`0x14c267c`). The run
/// then takes the game's interface and graphics down (`[app]+0x50` at
/// `0x14c2e27`).
const APP_QUITS: usize = 0x83;
const APP_PUMP_QUITS: usize = 0x2f8;
/// The game's check for a frame inside a day, between the day's phases.
const FRAME_CHECK: usize = 0x769ea0;
/// The camera's update in the in-game frame's input (docs/plans/drawing-thread.md,
/// T2): `movss xmm0, [r14 + 0x1658]`, the frame's length it is given, and
/// where it is stored as the update's fifth argument.
const CAMERA_LENGTH: usize = 0x831192;
const CAMERA_LENGTH_TAKEN: usize = 0x83119b;
/// The camera's update: the camera, the idler's `+0x68` and `+0x60`, the
/// frame's events, and the frame's length in seconds (a float, on the stack).
const CAMERA_UPDATE: GameFunction<5> = GameFunction::at(0x109bee0);
const IDLER_FRAME_LENGTH: usize = 0x1658;
/// The in-game idler's camera and graphical map.
const IDLER_CAMERA: usize = 0x1200;
const IDLER_MAP: usize = 0x11c0;
/// In the drawing: `movss xmm3, [r15 + 0x1658]`, the frame's length the 3D
/// map is given, by which it moves its fades on (`0x10a59a0`: the map's
/// layers by the camera's height, the time at its `+0xf8`), and where it
/// goes on.
const MAP_LENGTH: usize = 0x8188f4;
const MAP_LENGTH_TAKEN: usize = 0x8188fd;
const IDLER_CAMERA_A: usize = 0x68;
const IDLER_CAMERA_B: usize = 0x60;
/// What keeps the camera from being updated: the idler's `[+0x1b80]+0x18`
/// and `[+0x3b0]+0xc8`, a global switch for another camera, and the
/// application's focus (`+0xb8`).
const IDLER_HOLDS_CAMERA: usize = 0x1b80;
const IDLER_HOLDS_CAMERA_2: usize = 0x3b0;
use crate::objects::gui::globals::OTHER_CAMERA;
const APP_FOCUS: usize = 0xb8;

#[cfg(feature = "dev-performance")]
pub fn performance_points() -> Vec<Point> {
    vec![Point::observe(
        DRAWING,
        "48 8b c4 48 89 58 08 55 56 57 41 54 41 55 41 56 41 57",
        "performance: main-thread drawing",
        1,
        Some(crate::performance_hooks::main_draws),
        None,
    )]
}

fn points() -> Vec<Point> {
    vec![
        Point::jump(FRAME_DRAWS, "49 8b cd e8 8f 2a 00 00", "the in-game frame calls its drawing", frame_draws_stub as *const () as usize, &FRAME_RESUME),
        Point::keep(DRAWING, "48 8b c4 48 89 58 08 55 56 57 41 54 41 55 41 56 41 57", "the in-game frame's drawing"),
        Point::jump(HEAD, "48 8b 05 fa 37 c1 01", "the drawing's first steps", head_stub as *const () as usize, &HEAD_RESUME)
            .uses(&[SETTINGS])
            .recorded(&HEAD_AT),
        Point::keep(HEAD_END, "49 8b 8f f8 11 00 00", "where the drawing's first steps end"),
        // mov rcx, [debug country]: where the debug text begins.
        Point::keep(0x818aff, "48 8b 0d 3a 75 b2 01", "the debug text's country").uses(&[DEBUG_COUNTRY]),
        Point::jump(
            CAMERA_LENGTH,
            "f3 41 0f 10 86 58 16 00 00",
            "the frame's length the camera's update is given",
            camera_length_stub as *const () as usize,
            &CAMERA_RESUME,
        ),
        Point::keep(CAMERA_LENGTH_TAKEN, "f3 0f 11 44 24 20", "the camera's update's frame length stored"),
        Point::jump(MAP_LENGTH, "f3 41 0f 10 9f 58 16 00 00", "the frame's length the 3D map is given", map_length_stub as *const () as usize, &MAP_RESUME)
            .uses(&[IDLER_FRAME_LENGTH]),
        Point::keep(MAP_LENGTH_TAKEN, "4d 8b 87 00 12 00 00", "the camera the 3D map is given"),
        Point::keep(CAMERA_UPDATE, "48 89 5c 24 08 57 48 83 ec 70", "the camera's update"),
        // mov rcx, [r14 + 0x1b80]; cmp byte ptr [rcx + 0x18], 0; ...;
        // mov rax, [r14 + 0x3b0]; cmp byte ptr [rax + 0xc8], 0; ...;
        // cmp byte ptr [other camera], 0
        Point::keep(0x831137, "49 8b 8e 80 1b 00 00 80 79 18 00", "what holds the camera, first").uses(&[IDLER_HOLDS_CAMERA]),
        Point::keep(0x831144, "49 8b 86 b0 03 00 00 80 b8 c8 00 00 00 00", "what holds the camera, second").uses(&[IDLER_HOLDS_CAMERA_2]),
        Point::keep(0x831154, "80 3d f4 ed b0 01 00", "the other camera's switch").uses(&[OTHER_CAMERA]),
        // mov rcx, [application]: the in-game frame's GUI update takes it.
        Point::keep(0x81590d, "48 8b 0d 3c 66 c1 01", "the application, in the in-game frame").uses(&[APP]),
        Point::observe(APP_FRAME, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18", "the application's frame", 2, Some(app_frame_before), Some(app_frame_after)),
        // cmp [rsi + 0x2f8], r14b; cmp [rsi + 0x83], r14b: the run's loop.
        Point::keep(0x14c2dd0, "44 38 b6 f8 02 00 00 75 48", "the run's loop, the pump's quit").uses(&[APP_PUMP_QUITS]),
        Point::keep(0x14c2e18, "44 38 b6 83 00 00 00 74 af", "the run's loop, the quit").uses(&[APP_QUITS]),
        Point::observe(
            FRAME_CHECK,
            "48 8b c4 48 89 58 10 4c 89 40 18 55 56 57",
            "the check for a frame inside a day",
            4,
            Some(frame_check_before),
            Some(frame_check_after),
        ),
        // lea rbx, [rdi + 0x78]; cmp rbx, rsi: the application's frame
        // switches its idler, the pending one at hand.
        Point::jump(IDLER_SWITCH, "48 8d 5f 78 48 3b de", "the application switches its idler", idler_switch_stub as *const () as usize, &SWITCH_RESUME),
        Point::keep(IDLER_SWITCH_GOES_ON, "74 19 48 8b 06", "the idler switch goes on"),
    ]
    .into_iter()
    .chain(prepare::points())
    .chain(marks::points())
    .chain(crossings::points())
    .chain(lock_order::points())
    .chain(deferred::points())
    .chain(tooltip::points())
    .chain(icons::points())
    .collect()
}

/// The application's frame switching its idler (docs/research/main-menu.md,
/// "The application's idler switch"), which lets the current one go: and
/// where it goes on.
const IDLER_SWITCH: usize = 0x14c2839;
const IDLER_SWITCH_GOES_ON: usize = 0x14c2840;
crate::resume!(SWITCH_RESUME: IDLER_SWITCH_GOES_ON_AT = IDLER_SWITCH_GOES_ON);

#[unsafe(naked)]
extern "C" fn idler_switch_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call idler_switches),
        "lea rbx, [rdi + 0x78]",
        "cmp rbx, rsi",
        "jmp qword ptr [rip + {goes_on}]",
        idler_switches = sym idler_switches,
        goes_on = sym IDLER_SWITCH_GOES_ON_AT,
    );
}

/// The drawing thread is in a frame of its own, from the lock taken for it
/// to its drawing's return.
static IN_FRAME: AtomicBool = AtomicBool::new(false);

fn clear_staged_frame() {
    FRAMES.clear_idler();
    icons::clear();
    tooltip::clear();
    marks::clear();
    crossings::clear();
}

/// The idler is about to be switched, by the main thread holding the lock. A
/// frame of the drawing thread's may be in Present, without the lock, and is
/// to end with the idler it began with: the main thread lets the lock go
/// until it has. What was deferred is done before the idler goes.
#[called_by_game]
extern "C" fn idler_switches() {
    clear_staged_frame();
    main_side(|s| s.drawing_frame_ends(&MainLock, || IN_FRAME.load(Ordering::Acquire)));
    deferred::done(&crate::hooked_memory());
    crossings::clear();
}

crate::resume!(FRAME_RESUME: FRAME_DRAWN_AT = FRAME_DRAWN);
crate::resume!(HEAD_RESUME: HEAD_GOES_ON_AT = HEAD_GOES_ON, HEAD_END_AT = HEAD_END);
static HEAD_AT: AtomicUsize = AtomicUsize::new(0);
crate::resume!(CAMERA_RESUME: CAMERA_LENGTH_TAKEN_AT = CAMERA_LENGTH_TAKEN);
crate::resume!(MAP_RESUME: MAP_LENGTH_TAKEN_AT = MAP_LENGTH_TAKEN);
/// The length of the drawing thread's frame, in seconds (a float's bits).
static DRAWING_LENGTH: AtomicU32 = AtomicU32::new(0);

/// The frame's length for the 3D map: the drawing thread's own on the
/// drawing thread, which draws one update's frame many times. `rax` holds the
/// application, which the drawing goes on to read.
#[unsafe(naked)]
extern "C" fn map_length_stub() {
    std::arch::naked_asm!(
        "push rax",
        "mov eax, dword ptr gs:[0x48]",
        "cmp eax, dword ptr [rip + {drawing}]",
        "pop rax",
        "je 2f",
        "movss xmm3, dword ptr [r15 + {length}]",
        "jmp qword ptr [rip + {taken}]",
        "2:",
        "movss xmm3, dword ptr [rip + {own}]",
        "jmp qword ptr [rip + {taken}]",
        drawing = sym DRAWING_THREAD,
        length = const IDLER_FRAME_LENGTH,
        own = sym DRAWING_LENGTH,
        taken = sym MAP_LENGTH_TAKEN_AT,
    );
}
/// Set while the drawing thread moves the camera: the main thread's update of
/// it then takes the frame's events with no time passed.
static CAMERA_MOVED_THERE: AtomicU32 = AtomicU32::new(0);

/// The frame's length for the main thread's camera update: none while the
/// drawing thread moves the camera.
#[unsafe(naked)]
extern "C" fn camera_length_stub() {
    std::arch::naked_asm!(
        "cmp dword ptr [rip + {there}], 0",
        "jne 2f",
        "movss xmm0, dword ptr [r14 + {length}]",
        "jmp qword ptr [rip + {taken}]",
        "2:",
        "xorps xmm0, xmm0",
        "jmp qword ptr [rip + {taken}]",
        there = sym CAMERA_MOVED_THERE,
        length = const IDLER_FRAME_LENGTH,
        taken = sym CAMERA_LENGTH_TAKEN_AT,
    );
}

/// An event queue with no events, for the drawing thread's camera update.
static NO_EVENTS: [usize; 2] = [0, 0];

/// The camera's update on the drawing thread, with its own frame's length
/// and no events, as the in-game frame's input does it (`0x831137`–`0x8311b3`).
fn move_camera(m: &Memory, idler: usize, length: f32) -> crate::Result<()> {
    let holds = m.u8(m.ptr(idler + IDLER_HOLDS_CAMERA)? + 0x18)? != 0 || m.u8(m.ptr(idler + IDLER_HOLDS_CAMERA_2)? + 0xc8)? != 0;
    let other = m.u8(m.at(OTHER_CAMERA))? != 0;
    let focus = m.u8(m.ptr(m.at(APP))? + APP_FOCUS)? != 0;
    if holds || other || !focus {
        return Ok(());
    }
    let camera = m.ptr(idler + IDLER_CAMERA)?;
    let update: unsafe extern "C" fn(usize, usize, usize, usize, f32) -> usize = unsafe { std::mem::transmute(m.at(CAMERA_UPDATE.rva())) };
    let (a, b) = (m.ptr(idler + IDLER_CAMERA_A)?, m.ptr(idler + IDLER_CAMERA_B)?);
    unsafe { update(camera, a, b, NO_EVENTS.as_ptr() as usize, length) };
    Ok(())
}

/// Replaces the in-game frame's call of its drawing.
#[unsafe(naked)]
extern "C" fn frame_draws_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, r13", call frame_draws),
        "jmp qword ptr [rip + {drawn}]",
        frame_draws = sym frame_draws,
        drawn = sym FRAME_DRAWN_AT,
    );
}

/// The drawing's first steps: skipped on the drawing thread, as the game's
/// elsewhere. The thread's ID is the TEB's at `gs:[0x48]`; the settings'
/// global is reached from the head's own address, as its `mov rax,
/// [rip + rel32]` did.
#[unsafe(naked)]
extern "C" fn head_stub() {
    std::arch::naked_asm!(
        "mov eax, dword ptr gs:[0x48]",
        "cmp eax, dword ptr [rip + {drawing}]",
        "je 2f",
        "mov rax, qword ptr [rip + {head}]",
        "add rax, {to_settings}",
        "mov rax, qword ptr [rax]",
        "jmp qword ptr [rip + {goes_on}]",
        "2:",
        "jmp qword ptr [rip + {end}]",
        drawing = sym DRAWING_THREAD,
        head = sym HEAD_AT,
        to_settings = const SETTINGS - HEAD,
        goes_on = sym HEAD_GOES_ON_AT,
        end = sym HEAD_END_AT,
    );
}

/// The lock between the main thread and the drawing thread, which serves
/// whoever asked first.
struct Tickets {
    tickets: Mutex<(u64, u64)>,
    turn: Condvar,
}

impl Tickets {
    const fn new() -> Self {
        Self { tickets: Mutex::new((0, 0)), turn: Condvar::new() }
    }

    fn take(&self) -> Duration {
        let began = Instant::now();
        let mut t = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        let mine = t.0;
        t.0 += 1;
        while t.1 != mine {
            t = self.turn.wait(t).unwrap_or_else(|e| e.into_inner());
        }
        began.elapsed()
    }

    fn give(&self) {
        let mut t = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        t.1 += 1;
        self.turn.notify_all();
    }
}

static LOCK: Tickets = Tickets::new();

/// The main thread's way to the lock ([`turns::Lock`]): its waits counted
/// by where it waited, and [`MAIN_HOLDS`] kept.
struct MainLock;

impl turns::Lock for MainLock {
    fn take(&self, at: At) -> Duration {
        let waited = LOCK.take();
        let place = match at {
            At::Crossing => AT_CROSSING,
            At::DayEnd => AT_DAY_END,
            At::Frame | At::DrawingFrame | At::AfterPresent => AT_FRAME,
        };
        MAIN_WAITED.fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        WAITED_AT[place].fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        MAIN_HOLDS.store(true, Ordering::Release);
        waited
    }

    fn give(&self) {
        MAIN_HOLDS.store(false, Ordering::Release);
        LOCK.give();
    }
}

/// The drawing thread's way to the lock: its waits counted.
struct DrawingLock;

impl turns::Lock for DrawingLock {
    fn take(&self, _: At) -> Duration {
        let waited = LOCK.take();
        DRAWING_WAITED.fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        waited
    }

    fn give(&self) {
        LOCK.give();
    }
}

/// The drawing thread's ID, 0 until it runs; the head's stub compares it.
static DRAWING_THREAD: AtomicU32 = AtomicU32::new(0);
static FRAMES: course::FrameCoordinator = course::FrameCoordinator::new();
/// Wakes the drawing thread when the first frame of a session is prepared (or
/// when the game quits), so that it sleeps at startup and on the main menu.
static PREPARED_WAKE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());
/// The longest a frame waits so.
const PACE_AT_MOST: Duration = Duration::from_millis(50);
/// Whether the drawing thread draws the frames: its speed-up not left to the
/// game.
static ON: AtomicBool = AtomicBool::new(false);
static UNLIMITED: AtomicBool = AtomicBool::new(false);
/// The frames the drawing thread drew, and the main thread's waits for the
/// lock in microseconds, for the log.
static DRAWN: AtomicU64 = AtomicU64::new(0);
static MAIN_WAITED: AtomicU64 = AtomicU64::new(0);
/// The main thread's waits for a frame to be drawn, in microseconds.
static PACED: AtomicU64 = AtomicU64::new(0);
/// The main thread's waits by where it waited: at an application frame, at
/// a crossing, at the day's end.
static WAITED_AT: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
const AT_FRAME: usize = 0;
const AT_CROSSING: usize = 1;
const AT_DAY_END: usize = 2;
static DRAWING_WAITED: AtomicU64 = AtomicU64::new(0);
/// The frames the main thread drew itself, the drawing thread not having.
static HERE: AtomicU64 = AtomicU64::new(0);
/// The back buffer's size, as the drawing thread's last frame presented it.
static SCREEN: AtomicU64 = AtomicU64::new(0);

/// The main thread's side of the lock ([`turns`]).
static MAIN: Mutex<MainSide> = Mutex::new(MainSide::new());

fn main_side<T>(f: impl FnOnce(&mut MainSide) -> T) -> T {
    f(&mut MAIN.lock().unwrap_or_else(|e| e.into_inner()))
}

thread_local! {
    /// The drawing thread's side of the lock ([`turns`]).
    static DRAWING_SIDE: RefCell<DrawingSide> = RefCell::new(DrawingSide::default());
}

fn drawing_side<T>(f: impl FnOnce(&mut DrawingSide) -> T) -> T {
    DRAWING_SIDE.with(|d| f(&mut d.borrow_mut()))
}

/// The main thread holds the presentation lock: [`MainSide::holds`], read
/// with no lock taken, for what cannot take one (the write watch's exception
/// handler).
static MAIN_HOLDS: AtomicBool = AtomicBool::new(false);

/// The drawing's RVA, for the write watch's report.
pub const fn drawing_rva() -> usize {
    DRAWING.rva()
}

/// Whether the main thread holds the presentation lock now.
pub fn main_holds() -> bool {
    MAIN_HOLDS.load(Ordering::Acquire)
}

/// Whether the current main-thread operation excludes concurrent drawing.
/// The game's drawing path needs no presentation lock; threaded drawing does.
pub(crate) fn main_owns_presentation() -> bool {
    on_main() && (!drawing_on() || main_holds())
}

/// The game quits, and the drawing thread draws no more ([`stop`]).
static STOPPED: AtomicBool = AtomicBool::new(false);

/// Whether the drawing thread draws the frames now.
fn drawing_on() -> bool {
    ON.load(Ordering::Relaxed) && DRAWING_THREAD.load(Ordering::Relaxed) != 0 && !STOPPED.load(Ordering::Acquire)
}

fn on_main() -> bool {
    crate::thread_id() == crate::main_thread()
}

/// Tells the drawing thread the setting it runs with: at startup, and when
/// settings are saved. Starts the thread the first time, unless the speed-up
/// is left to the game, whose points are then not installed.
pub fn set(unlimited: bool) {
    let on = !crate::speed::game_path(&SPEED_UP);
    UNLIMITED.store(unlimited, Ordering::Relaxed);
    ON.store(on, Ordering::Relaxed);
    if on && DRAWING_THREAD.load(Ordering::Relaxed) == 0 {
        let spawned = std::thread::Builder::new().name("eu4-harmony drawing".into()).stack_size(32 << 20).spawn(|| crate::guarded("drawing thread", run));
        if let Err(e) = spawned {
            log!("drawing: no drawing thread ({e}): the main thread draws");
            ON.store(false, Ordering::Relaxed);
        }
    }
    log!(
        "drawing: {}, {}",
        if on { "on a thread of its own" } else { "on the main thread" },
        if unlimited { "no limit" } else { "at the display's refresh rate" }
    );
}

fn app_frame_before(_: &mut Call) {
    if !on_main() {
        return;
    }
    if !FRAMES.take_day_ran() {
        pace();
    }
    main_side(|s| s.frame_begins(&MainLock));
}

fn app_frame_after(call: &mut Call) {
    if !on_main() {
        return;
    }
    let m = call.memory();
    let app = call.arg(0);
    let quits = [APP_QUITS, APP_PUMP_QUITS].iter().any(|&at| m.u8(app + at).is_ok_and(|b| b != 0));
    main_side(|s| s.frame_ends(&MainLock));
    if quits {
        stop();
    }
}

/// The game quits: its run takes the interface and the graphics down after
/// this frame, which the drawing thread must not draw meanwhile. It ends the
/// frame it is in, as at an idler's switch, and draws no more; the main
/// thread keeps the lock to the end, so that a frame the drawing thread
/// began to wait for before it was stopped never begins.
fn stop() {
    STOPPED.store(true, Ordering::Release);
    clear_staged_frame();
    let _guard = PREPARED_WAKE.0.lock().unwrap_or_else(|e| e.into_inner());
    PREPARED_WAKE.1.notify_all();
    drop(_guard);
    main_side(|s| s.quits(&MainLock, || IN_FRAME.load(Ordering::Acquire)));
    deferred::done(&crate::hooked_memory());
    crossings::clear();
    log!("drawing: the game quits; the drawing thread draws no more");
}

/// Waits for the drawing thread to draw the last frame prepared, unless it
/// cannot (a crossing or main-drawn frame invalidated what it draws) or the
/// main thread holds the lock (a frame inside another).
fn pace() {
    let Some(wanted) = FRAMES.pace_target(drawing_on(), crossings::holding_back().is_some(), main_side(|s| s.may_pace())) else {
        return;
    };
    let began = Instant::now();
    while !FRAMES.drawn_caught_up(wanted) && began.elapsed() < PACE_AT_MOST && drawing_on() {
        std::thread::sleep(Duration::from_micros(250));
    }
    PACED.fetch_add(began.elapsed().as_micros() as u64, Ordering::Relaxed);
}

/// The daily update begins (`game::events`): it lets the lock go.
pub fn day_begins() {
    if !on_main() || !drawing_on() {
        return;
    }
    FRAMES.note_day_ran();
    #[cfg(feature = "dev-write-watch")]
    crate::write_watch::drawing::day_begins(&crate::hooked_memory());
    main_side(|s| s.day_begins(&MainLock));
}

/// The daily update has returned (`game::events`): the frame it ran in has
/// the lock again. What the day deferred is done as that frame is prepared.
pub fn day_ends() {
    if !on_main() {
        return;
    }
    main_side(|s| s.day_ends(&MainLock));
}

fn frame_check_before(call: &mut Call) {
    #[cfg(feature = "dev-crossings")]
    crate::crossings::check_before(call);
    let _ = call;
    if on_main() {
        main_side(|s| s.frame_check(&MainLock));
    }
}

fn frame_check_after(call: &mut Call) {
    #[cfg(feature = "dev-crossings")]
    crate::crossings::check_after(call);
    let _ = call;
}

/// The in-game frame calls its drawing, on the main thread, which holds the
/// lock.
#[called_by_game]
extern "C" fn frame_draws(idler: usize) {
    let m = crate::hooked_memory();
    // The frame's update has finished: what was deferred is done now, its
    // crossings, if any, forgotten as the frame is prepared.
    deferred::done(&m);
    prepare::tooltips(&m, idler);
    // A frame the main thread draws itself invalidates the earlier prepared
    // frame and drops icons staged during this update without faking a crossing.
    if !drawing_on() || main_draws(&m) {
        FRAMES.main_draws_frame();
        icons::discard_placing();
        draw_here(&m, idler);
        return;
    }
    if let Err(e) = prepare::frame(&m, idler) {
        icons::discard_placing();
        log!("drawing: the frame could not be prepared: {e}");
        return;
    }
    if let Err(e) = marks::keep(&m, idler) {
        log!("drawing: the frame's temporary map highlights could not be kept: {e}");
    }
    // This frame's update, and what of the drawing was done here, which may
    // draw into textures in scenes of their own, have finished whatever the
    // crossings before began.
    crossings::frame_prepared();
    icons::prepared();
    let was_idle = FRAMES.prepared_idler() == 0;
    FRAMES.publish_prepared(idler);
    #[cfg(feature = "dev-performance")]
    crate::performance::prepared();
    CAMERA_MOVED_THERE.store(1, Ordering::Relaxed);
    if was_idle {
        let _guard = PREPARED_WAKE.0.lock().unwrap_or_else(|e| e.into_inner());
        PREPARED_WAKE.1.notify_all();
    }
    // What the main thread does as it presents a frame: the safe point of a
    // frame outside a game, and the input taken for the next.
    crate::safe_point::presented();
    let screen = SCREEN.load(Ordering::Relaxed);
    if screen != 0 {
        crate::game::input::presenting(((screen >> 32) as u32, screen as u32));
    }
}

/// Whether the main thread draws this frame itself: the console's debug
/// text, which works a country's values out as it is drawn, or the framework
/// holding frames back.
fn main_draws(m: &Memory) -> bool {
    let debug_text = m.ptr(m.at(DEBUG_COUNTRY)).ok().is_none_or(|c| c != 0);
    debug_text || crate::framework_ui::holding()
}

/// Whether the drawing thread must not draw the frame prepared: what makes
/// the main thread draw, or the map's names due since, which reads the world.
fn must_draw_here(m: &Memory) -> bool {
    prepare::names_due(m) || main_draws(m)
}

/// Whether the current thread is the drawing thread.
pub fn is_drawing_thread() -> bool {
    let t = DRAWING_THREAD.load(Ordering::Relaxed);
    t != 0 && t == crate::thread_id()
}

/// The drawing thread presented a frame of `width` by `height`.
pub fn presented(width: u32, height: u32) {
    SCREEN.store((width as u64) << 32 | height as u64, Ordering::Relaxed);
}

/// The drawing thread is in Direct3D's Present, which may wait for the
/// display: the main thread does not draw a frame of its own meanwhile.
static PRESENTING: AtomicBool = AtomicBool::new(false);

/// The drawing thread's frame goes into Direct3D's Present (the framework's
/// Present observer, `framework_ui::hook`): it lets the lock go for it, as
/// the frame is drawn and Present touches only the device, which serves
/// several threads.
pub fn present_begins() {
    PRESENTING.store(true, Ordering::Release);
    PRESENT.with(|p| p.began.set(Some(Instant::now())));
    drawing_side(|d| d.present_begins(&DrawingLock));
}

/// Present has returned: the drawing thread takes the lock again for what
/// the drawing does after it (the idler's `+0x1608`, its clock).
pub fn present_ends() {
    PRESENTING.store(false, Ordering::Release);
    PRESENT.with(|p| {
        if let Some(began) = p.began.take() {
            p.spent.set(p.spent.get() + began.elapsed());
        }
    });
    let waited = drawing_side(|d| d.present_ends(&DrawingLock));
    PRESENT.with(|p| p.lock_after.set(p.lock_after.get() + waited));
}

/// The drawing thread's time in Present and waiting for the lock after it,
/// in the frame drawn now, for [`Gaps`].
#[derive(Default)]
struct PresentTimes {
    began: std::cell::Cell<Option<Instant>>,
    spent: std::cell::Cell<Duration>,
    lock_after: std::cell::Cell<Duration>,
}

thread_local! {
    static PRESENT: PresentTimes = PresentTimes::default();
}

/// The main thread draws a frame of its own: once the drawing thread's
/// Present has returned.
fn draw_here(m: &Memory, idler: usize) {
    while PRESENTING.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    HERE.fetch_add(1, Ordering::Relaxed);
    unsafe { direct1(m, DRAWING)(idler) };
}

/// A game function the drawing calls, called as the game calls it, with no
/// guard: a fault in it ends the game as the game's own would.
fn direct1(m: &Memory, function: GameFunction<1>) -> unsafe extern "C" fn(usize) -> usize {
    unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize) -> usize>(m.at(function.rva())) }
}

/// [`direct1`], for a function of three arguments.
fn direct3(m: &Memory, function: GameFunction<3>) -> unsafe extern "C" fn(usize, usize, usize) -> usize {
    unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(usize, usize, usize) -> usize>(m.at(function.rva())) }
}

#[cfg(feature = "dev-performance")]
static PERFORMANCE_RATE: AtomicU32 = AtomicU32::new(0);

/// The actual target used by the drawing thread; the native main-thread
/// path and unlimited drawing have no declared target for threshold counts.
#[cfg(feature = "dev-performance")]
pub fn performance_interval() -> Option<Duration> {
    let rate = PERFORMANCE_RATE.load(Ordering::Relaxed);
    (drawing_on() && !UNLIMITED.load(Ordering::Relaxed) && rate != 0).then(|| Duration::from_secs_f64(1.0 / rate as f64))
}

/// The display's refresh rate, 60 when Windows does not say.
fn refresh_rate() -> u32 {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
    mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    let ok = unsafe { EnumDisplaySettingsW(std::ptr::null(), ENUM_CURRENT_SETTINGS, &mut mode) } != 0;
    if ok && (24..=1000).contains(&mode.dmDisplayFrequency) {
        mode.dmDisplayFrequency
    } else {
        60
    }
}

/// Waits until `until`, to the tenth of a millisecond.
fn wait_until(until: Instant) {
    use windows_sys::Win32::System::Threading::{
        CreateWaitableTimerExW, SetWaitableTimer, WaitForSingleObject, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
    };
    thread_local! {
        static TIMER: isize = unsafe { CreateWaitableTimerExW(std::ptr::null(), std::ptr::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS) } as isize;
    }
    let now = Instant::now();
    if until <= now {
        return;
    }
    let timer = TIMER.with(|t| *t);
    if timer == 0 {
        std::thread::sleep(until - now);
        return;
    }
    // Relative, in 100 ns.
    let due = -((until - now).as_nanos() as i64 / 100);
    unsafe {
        if SetWaitableTimer(timer as _, &due, 0, None, std::ptr::null(), 0) != 0 {
            WaitForSingleObject(timer as _, INFINITE);
        }
    }
}

/// The drawing thread.
fn run() {
    DRAWING_THREAD.store(crate::thread_id(), Ordering::Relaxed);
    let m = crate::hooked_memory();
    let rate = refresh_rate();
    #[cfg(feature = "dev-performance")]
    PERFORMANCE_RATE.store(rate, Ordering::Relaxed);
    let interval = Duration::from_secs_f64(1.0 / rate as f64);
    log!("drawing: the drawing thread runs, at {rate} frames a second unless no limit");
    let mut next = Instant::now();
    let mut logged = Instant::now();
    let mut last_drawn = 0u64;
    let mut last_frame = Instant::now();
    let mut gaps = course::Gaps::default();
    loop {
        if !drawing_on() {
            CAMERA_MOVED_THERE.store(0, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(20));
            next = Instant::now();
            continue;
        }
        if FRAMES.prepared_idler() == 0 {
            CAMERA_MOVED_THERE.store(0, Ordering::Relaxed);
            if logged.elapsed() >= Duration::from_secs(10) {
                let drawn = DRAWN.load(Ordering::Relaxed);
                let (crossings_taken, held_longest) = crossings::logged(m.at(0usize));
                log!(
                    "drawing: {:.1} frames a second, {}; {} drawn by the main thread; the main thread waited {:.0} ms for the lock ({:.0} at frames, {:.0} at crossings, {:.0} at days' ends) and {:.0} ms for frames drawn, the drawing thread {:.0} ms; {} crossing(s), {} deferred, the frames held back longest by {}",
                    (drawn - last_drawn) as f64 / logged.elapsed().as_secs_f64(),
                    gaps.told(m.at(0usize)),
                    HERE.swap(0, Ordering::Relaxed),
                    MAIN_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                    WAITED_AT[AT_FRAME].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                    WAITED_AT[AT_CROSSING].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                    WAITED_AT[AT_DAY_END].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                    PACED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                    DRAWING_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                    crossings_taken,
                    deferred::logged(),
                    held_longest
                );
                last_drawn = drawn;
                logged = Instant::now();
            }
            let guard = PREPARED_WAKE.0.lock().unwrap_or_else(|e| e.into_inner());
            if FRAMES.prepared_idler() == 0 && drawing_on() {
                let _ = PREPARED_WAKE.1.wait_timeout(guard, Duration::from_millis(200));
            }
            next = Instant::now();
            last_frame = next;
            continue;
        }
        if !UNLIMITED.load(Ordering::Relaxed) {
            wait_until(next);
            next += interval;
            let now = Instant::now();
            if next < now {
                next = now;
            }
        }
        let waited = drawing_side(|d| d.turn_begins(&DrawingLock));
        gaps.waited(waited);
        let idler = FRAMES.prepared_idler();
        let current = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
        // This frame's length, for the camera and the map's fades.
        let length = last_frame.elapsed().as_secs_f32().min(0.1);
        last_frame = Instant::now();
        // The camera moves at this thread's pace, also while a frame waits
        // for its update; the main thread's update of it then passes no time.
        if idler != 0 && current == idler && CAMERA_MOVED_THERE.load(Ordering::Relaxed) != 0 {
            if let Err(e) = move_camera(&m, idler, length) {
                log!("drawing: the camera's update could not read the game: {e}");
            }
        }
        let held = crossings::holding_back();
        if let Some(into) = held {
            gaps.held_by(into);
        }
        if let Some((idler, seq)) = FRAMES.drawable(current, held, must_draw_here(&m)) {
            DRAWING_LENGTH.store(length.to_bits(), Ordering::Relaxed);
            if let Err(e) = icons::follow_camera(&m) {
                log!("drawing: the map icons could not follow the camera: {e}");
            }
            if let Err(e) = tooltip::follow_cursor(&m, idler) {
                log!("drawing: the tooltip could not follow the cursor: {e}");
            }
            IN_FRAME.store(true, Ordering::Release);
            #[cfg(feature = "dev-write-watch")]
            let watched = crate::write_watch::drawing::frame_begins(&m);
            #[cfg(not(feature = "dev-write-watch"))]
            let watched = false;
            drawing_side(|d| d.frame_begins(watched));
            let began = Instant::now();
            unsafe { direct1(&m, DRAWING)(idler) };
            let drawing = began.elapsed();
            let (present, lock_after) = PRESENT.with(|p| (p.spent.take(), p.lock_after.take()));
            drawing_side(DrawingSide::frame_ends);
            #[cfg(feature = "dev-write-watch")]
            crate::write_watch::drawing::frame_ends(watched);
            IN_FRAME.store(false, Ordering::Release);
            DRAWN.fetch_add(1, Ordering::Relaxed);
            FRAMES.note_drawn(seq);
            gaps.drawn(Instant::now(), interval, drawing, present, lock_after);
        }
        drawing_side(|d| d.turn_ends(&DrawingLock));
        if logged.elapsed() >= Duration::from_secs(10) {
            let drawn = DRAWN.load(Ordering::Relaxed);
            let (crossings_taken, held_longest) = crossings::logged(m.at(0usize));
            log!(
                "drawing: {:.1} frames a second, {}; {} drawn by the main thread; the main thread waited {:.0} ms for the lock ({:.0} at frames, {:.0} at crossings, {:.0} at days' ends) and {:.0} ms for frames drawn, the drawing thread {:.0} ms; {} crossing(s), {} deferred, the frames held back longest by {}",
                (drawn - last_drawn) as f64 / logged.elapsed().as_secs_f64(),
                gaps.told(m.at(0usize)),
                HERE.swap(0, Ordering::Relaxed),
                MAIN_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                WAITED_AT[AT_FRAME].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                WAITED_AT[AT_CROSSING].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                WAITED_AT[AT_DAY_END].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                PACED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                DRAWING_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                crossings_taken,
                deferred::logged(),
                held_longest
            );
            last_drawn = drawn;
            logged = Instant::now();
        }
    }
}
