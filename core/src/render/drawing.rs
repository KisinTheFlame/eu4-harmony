//! The drawing thread (GLOSSARY): the
//! in-game frame's drawing done by a thread of the framework's, at a pace of
//! its own, while the main thread goes on with the day.
//!
//! The in-game frame calls its drawing (`0x8183b0`) last, right after the
//! GUI's update. With the drawing thread on, the main thread does there only
//! what of the drawing reads the world or draws into the interface's textures
//! ([`prepare`]); then it notes the frame as prepared, and goes on. The
//! drawing thread draws the prepared frame again and again, at the display's
//! refresh rate, the frame rate limit chosen in the settings, or with no
//! limit, through the game's own drawing, whose first steps it skips, with the
//! map icons moved to its camera ([`icons`]).
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
mod crossings;
mod deferred;
mod icons;
mod marks;
mod prepare;
mod tooltip;
mod turns;

pub(crate) use crossings::SCENE_BEGINS;

use crate::base::called_by_game;
use crate::native::hooks::Call;
use crate::native::memory::Memory;
use crate::native::points::{GameFunction, Point};
use crate::native::speed_up::SpeedUp;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};
use turns::{At, DrawingSide, MainSide};

pub const SPEED_UP: SpeedUp = SpeedUp::new("drawing-thread", "frames", points, false);

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
pub(crate) const SETTINGS: usize = 0x242bde8;
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
/// The camera's update in the in-game frame's input (docs/plans/README.md#drawing,
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
use crate::native::objects::gui::globals::OTHER_CAMERA;
const APP_FOCUS: usize = 0xb8;

#[cfg(feature = "dev")]
pub fn main_draw_point(before: fn(&mut Call)) -> Point {
    Point::observe(DRAWING, "48 8b c4 48 89 58 08 55 56 57 41 54 41 55 41 56 41 57", "performance: main-thread drawing", 1, Some(before), None)
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
    .chain(deferred::points())
    .chain(tooltip::points())
    .chain(icons::points())
    .collect()
}

/// The application's frame switching its idler (docs/game/interface/main-menu.md,
/// "The application's idler switch"), which lets the current one go: and
/// where it goes on.
const IDLER_SWITCH: usize = 0x14c2839;
const IDLER_SWITCH_GOES_ON: usize = 0x14c2840;
resume!(SWITCH_RESUME: IDLER_SWITCH_GOES_ON_AT = IDLER_SWITCH_GOES_ON);

#[unsafe(naked)]
extern "C" fn idler_switch_stub() {
    std::arch::naked_asm!(
        call_rust!(call idler_switches),
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

/// The idler is about to be switched, by the main thread holding the lock. A
/// frame of the drawing thread's may be in Present, without the lock, and is
/// to end with the idler it began with: the main thread lets the lock go
/// until it has. What was deferred is done before the idler goes.
#[called_by_game]
extern "C" fn idler_switches() {
    deferred::clear_minimap();
    let m = crate::native::hooked_memory();
    let mut stage = LiveStage(&m);
    FRAMES.clear_across_deferred(&mut stage, |_| {
        main_side(|s| s.drawing_frame_ends(&MainLock, || IN_FRAME.load(Ordering::Acquire)));
    });
    deferred::clear_minimap();
}

/// Clears any staged minimap texture upload before a Direct3D device reset.
pub(super) fn device_reset() {
    deferred::clear_minimap();
}

resume!(FRAME_RESUME: FRAME_DRAWN_AT = FRAME_DRAWN);
resume!(HEAD_RESUME: HEAD_GOES_ON_AT = HEAD_GOES_ON, HEAD_END_AT = HEAD_END);
static HEAD_AT: AtomicUsize = AtomicUsize::new(0);
resume!(CAMERA_RESUME: CAMERA_LENGTH_TAKEN_AT = CAMERA_LENGTH_TAKEN);
resume!(MAP_RESUME: MAP_LENGTH_TAKEN_AT = MAP_LENGTH_TAKEN);
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
fn move_camera(m: &Memory, idler: usize, length: f32) -> crate::base::Result<()> {
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
        call_rust!("mov rcx, r13", call frame_draws),
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

/// Owns the fair ticket presentation lock, the main thread's lock state, its
/// lock-free atomic mirrors (`main_holds`, `main_protects_changes`), and lock
/// wait accounting in one place so the mirrors never drift from `MainSide`.
struct PresentationLock {
    tickets: Tickets,
    main: Mutex<MainSide>,
    main_holds: AtomicBool,
    main_protects_changes: AtomicBool,
    main_waited: AtomicU64,
    waited_at: [AtomicU64; 3],
    drawing_waited: AtomicU64,
}

impl PresentationLock {
    const fn new() -> Self {
        Self {
            tickets: Tickets::new(),
            main: Mutex::new(MainSide::new()),
            main_holds: AtomicBool::new(false),
            main_protects_changes: AtomicBool::new(false),
            main_waited: AtomicU64::new(0),
            waited_at: [const { AtomicU64::new(0) }; 3],
            drawing_waited: AtomicU64::new(0),
        }
    }

    fn take_main(&self, at: At) -> Duration {
        let waited = self.tickets.take();
        let place = match at {
            At::Crossing => AT_CROSSING,
            At::DayEnd => AT_DAY_END,
            At::Frame | At::DrawingFrame | At::AfterPresent => AT_FRAME,
        };
        let micros = waited.as_micros() as u64;
        self.main_waited.fetch_add(micros, Ordering::Relaxed);
        self.waited_at[place].fetch_add(micros, Ordering::Relaxed);
        self.main_holds.store(true, Ordering::Release);
        waited
    }

    fn give_main(&self) {
        self.main_holds.store(false, Ordering::Release);
        self.tickets.give();
    }

    fn take_drawing(&self) -> Duration {
        let waited = self.tickets.take();
        self.drawing_waited.fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        waited
    }

    fn give_drawing(&self) {
        self.tickets.give();
    }

    fn with_main<T>(&self, f: impl FnOnce(&mut MainSide) -> T) -> T {
        let mut guard = self.main.lock().unwrap_or_else(|e| e.into_inner());
        let out = f(&mut guard);
        self.main_holds.store(guard.holds(), Ordering::Release);
        self.main_protects_changes.store(guard.protects_changes(), Ordering::Release);
        out
    }

    fn main_holds(&self) -> bool {
        self.main_holds.load(Ordering::Acquire)
    }

    fn main_protects_changes(&self) -> bool {
        self.main_protects_changes.load(Ordering::Acquire)
    }

    fn swap_waits(&self) -> (u64, [u64; 3], u64) {
        (
            self.main_waited.swap(0, Ordering::Relaxed),
            [
                self.waited_at[AT_FRAME].swap(0, Ordering::Relaxed),
                self.waited_at[AT_CROSSING].swap(0, Ordering::Relaxed),
                self.waited_at[AT_DAY_END].swap(0, Ordering::Relaxed),
            ],
            self.drawing_waited.swap(0, Ordering::Relaxed),
        )
    }
}

static PRESENTATION: PresentationLock = PresentationLock::new();

/// The main thread's way to the lock ([`turns::Lock`]).
struct MainLock;

impl turns::Lock for MainLock {
    fn take(&self, at: At) -> Duration {
        PRESENTATION.take_main(at)
    }

    fn give(&self) {
        PRESENTATION.give_main();
    }
}

/// The drawing thread's way to the lock.
struct DrawingLock;

impl turns::Lock for DrawingLock {
    fn take(&self, _: At) -> Duration {
        PRESENTATION.take_drawing()
    }

    fn give(&self) {
        PRESENTATION.give_drawing();
    }
}

/// Coordinates publication, invalidation, pacing, and drawing eligibility of
/// prepared frames between the main thread and the drawing thread.
struct FrameCoordinator {
    prepared_idler: AtomicUsize,
    prepared_seq: AtomicU64,
    drawn_seq: AtomicU64,
    day_ran: AtomicBool,
    invalidated_by_main: AtomicBool,
}

impl FrameCoordinator {
    const fn new() -> Self {
        Self {
            prepared_idler: AtomicUsize::new(0),
            prepared_seq: AtomicU64::new(0),
            drawn_seq: AtomicU64::new(0),
            day_ran: AtomicBool::new(false),
            invalidated_by_main: AtomicBool::new(false),
        }
    }

    fn note_day_ran(&self) {
        self.day_ran.store(true, Ordering::Release);
    }

    fn take_day_ran(&self) -> bool {
        self.day_ran.swap(false, Ordering::AcqRel)
    }

    fn prepared_idler(&self) -> usize {
        self.prepared_idler.load(Ordering::Acquire)
    }

    fn drawn_caught_up(&self, wanted: u64) -> bool {
        self.drawn_seq.load(Ordering::Acquire) >= wanted
    }

    fn pace_target(&self, drawing_on: bool, holding_back: bool, may_pace: bool) -> Option<u64> {
        let wanted = self.prepared_seq.load(Ordering::Acquire);
        if !drawing_on
            || holding_back
            || !may_pace
            || self.prepared_idler.load(Ordering::Acquire) == 0
            || self.invalidated_by_main.load(Ordering::Acquire)
            || self.drawn_caught_up(wanted)
        {
            return None;
        }
        Some(wanted)
    }

    fn main_draws_frame(&self) {
        self.invalidated_by_main.store(true, Ordering::Release);
    }

    fn publish_prepared(&self, idler: usize) -> u64 {
        self.invalidated_by_main.store(false, Ordering::Release);
        self.prepared_idler.store(idler, Ordering::Release);
        self.prepared_seq.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn clear_idler(&self) {
        self.prepared_idler.store(0, Ordering::Release);
        self.invalidated_by_main.store(false, Ordering::Release);
    }

    fn drawable(&self, current_idler: usize, holding_back: Option<usize>, must_draw_here: bool) -> Option<(usize, u64)> {
        let idler = self.prepared_idler.load(Ordering::Acquire);
        let seq = self.prepared_seq.load(Ordering::Acquire);
        if idler == 0 || current_idler != idler || holding_back.is_some() || must_draw_here || self.invalidated_by_main.load(Ordering::Acquire) {
            return None;
        }
        Some((idler, seq))
    }

    fn note_drawn(&self, seq: u64) {
        self.drawn_seq.store(seq, Ordering::Release);
    }

    fn stage_frame(&self, idler: usize, ops: &mut impl StageOps) {
        ops.run_deferred();
        if ops.draw_on_main_only() {
            self.main_draws_frame();
            ops.draw_inline(idler, FallbackMode::BeforeTooltips);
            return;
        }
        if let Err((mode, reason)) = ops.prepare_for_drawing(idler) {
            self.clear_idler();
            self.main_draws_frame();
            ops.clear_staged_artifacts();
            ops.log(&format!("drawing: {reason}"));
            ops.draw_inline(idler, mode);
            return;
        }
        let was_idle = self.prepared_idler() == 0;
        self.publish_prepared(idler);
        ops.commit_prepared(was_idle);
    }

    fn clear_across_deferred<S: StageOps>(&self, ops: &mut S, wait_drawing_end: impl FnOnce(&mut S)) {
        self.clear_idler();
        ops.clear_staged_artifacts();
        wait_drawing_end(ops);
        ops.run_deferred();
        self.clear_idler();
        ops.clear_staged_artifacts();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FallbackMode {
    BeforeTooltips,
    AfterTooltips,
}

trait StageOps {
    fn run_deferred(&mut self);
    fn draw_on_main_only(&self) -> bool;
    fn prepare_for_drawing(&mut self, idler: usize) -> Result<(), (FallbackMode, String)>;
    fn draw_inline(&mut self, idler: usize, mode: FallbackMode);
    fn clear_staged_artifacts(&mut self);
    fn commit_prepared(&mut self, was_idle: bool);
    fn log(&mut self, line: &str);
}

const LATE_BY: [&str; 6] = ["held back", "the lock before", "drawing", "Present", "the lock after Present", "other"];

/// Late-frame gap accounting across ten-second windows.
#[derive(Default)]
struct Gaps {
    last: Option<Instant>,
    held: Option<usize>,
    parts: [Duration; 4],
    late: [u32; 6],
    by: Vec<(usize, u32)>,
    longest: Duration,
    drawing: (Duration, u32, Duration),
}

impl Gaps {
    fn held_by(&mut self, into: usize) {
        self.held.get_or_insert(into);
    }

    fn waited(&mut self, waited: Duration) {
        self.parts[0] += waited;
    }

    fn drawn(&mut self, now: Instant, interval: Duration, drawing: Duration, present: Duration, lock_after: Duration) {
        let own = drawing.saturating_sub(present + lock_after);
        self.parts[1] += own;
        self.drawing = (self.drawing.0 + own, self.drawing.1 + 1, self.drawing.2.max(own));
        self.parts[2] += present;
        self.parts[3] += lock_after;
        if let Some(last) = self.last.replace(now) {
            let gap = now.saturating_duration_since(last);
            if gap > interval * 2 {
                let rest = gap.saturating_sub(self.parts.iter().sum());
                let why = match self.held {
                    Some(_) => 0,
                    None => {
                        let parts = [self.parts[0], self.parts[1], self.parts[2], self.parts[3], rest];
                        1 + (0..parts.len()).max_by_key(|&i| parts[i]).unwrap_or(4)
                    }
                };
                self.late[why] += 1;
                if let Some(into) = self.held {
                    match self.by.iter_mut().find(|b| b.0 == into) {
                        Some(b) => b.1 += 1,
                        None => self.by.push((into, 1)),
                    }
                }
            }
            self.longest = self.longest.max(gap);
        }
        self.held = None;
        self.parts = [Duration::ZERO; 4];
    }

    fn told(&mut self, base: usize) -> String {
        self.by.sort_by_key(|b| std::cmp::Reverse(b.1));
        let by: Vec<String> = self.by.iter().take(3).map(|(into, n)| format!("{:#x} {n}", into.wrapping_sub(base))).collect();
        let others: Vec<String> = (1..LATE_BY.len()).map(|i| format!("{} {}", LATE_BY[i], self.late[i])).collect();
        let (all, frames, longest) = self.drawing;
        let told = format!(
            "{} late (held back {} by {}, {}), the longest gap {:.0} ms; a frame's drawing {:.1} ms, the longest {:.1} ms",
            self.late.iter().sum::<u32>(),
            self.late[0],
            by.join(", "),
            others.join(", "),
            self.longest.as_secs_f64() * 1000.0,
            all.as_secs_f64() * 1000.0 / frames.max(1) as f64,
            longest.as_secs_f64() * 1000.0
        );
        self.drawing = Default::default();
        self.late = [0; 6];
        self.by.clear();
        self.longest = Duration::ZERO;
        told
    }
}

/// The drawing thread's ID, 0 until it runs; the head's stub compares it.
use crate::base::runtime::DRAWING_THREAD;
static FRAMES: FrameCoordinator = FrameCoordinator::new();
/// Wakes the drawing thread when the first frame of a session is prepared (or
/// when the game quits), so that it sleeps at startup and on the main menu.
static PREPARED_WAKE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());
/// The longest a frame waits so.
const PACE_AT_MOST: Duration = Duration::from_millis(50);
/// Whether the drawing thread draws the frames: its speed-up not left to the
/// game.
static ON: AtomicBool = AtomicBool::new(false);
/// Sentinel in [`FRAME_RATE_LIMIT`] for unlimited drawing (no wait after a
/// frame drawn).
const UNLIMITED: u32 = u32::MAX;
/// The frame rate limit chosen in the settings: 0 to follow the display's
/// refresh rate, a rate within `FRAME_RATE_BOUNDS`, or [`UNLIMITED`].
static FRAME_RATE_LIMIT: AtomicU32 = AtomicU32::new(0);
/// The frames the drawing thread drew, for the log.
static DRAWN: AtomicU64 = AtomicU64::new(0);
/// The main thread's waits for a frame to be drawn, in microseconds.
static PACED: AtomicU64 = AtomicU64::new(0);
const AT_FRAME: usize = 0;
const AT_CROSSING: usize = 1;
const AT_DAY_END: usize = 2;
/// The frames the main thread drew itself, the drawing thread not having.
static HERE: AtomicU64 = AtomicU64::new(0);
/// The back buffer's size, as the drawing thread's last frame presented it.
static SCREEN: AtomicU64 = AtomicU64::new(0);

fn main_side<T>(f: impl FnOnce(&mut MainSide) -> T) -> T {
    PRESENTATION.with_main(f)
}

pub(super) fn main_protects_changes() -> bool {
    PRESENTATION.main_protects_changes()
}

thread_local! {
    /// The drawing thread's side of the lock ([`turns`]).
    static DRAWING_SIDE: RefCell<DrawingSide> = RefCell::new(DrawingSide::default());
}

fn drawing_side<T>(f: impl FnOnce(&mut DrawingSide) -> T) -> T {
    DRAWING_SIDE.with(|d| f(&mut d.borrow_mut()))
}

/// The drawing's RVA, for the write watch's report.
#[cfg(feature = "dev")]
pub const fn drawing_rva() -> usize {
    DRAWING.rva()
}

/// Whether the main thread holds the presentation lock now.
pub fn main_holds() -> bool {
    PRESENTATION.main_holds()
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
    crate::base::thread_id() == crate::base::main_thread()
}

/// Tells the drawing thread the setting it runs with: at startup, and when
/// settings are saved. Starts the thread the first time, unless the speed-up
/// is left to the game, whose points are then not installed.
pub fn set(frame_rate_limit: crate::base::settings::FrameRateLimit) {
    use crate::base::settings::FrameRateLimit;
    let on = !crate::native::speed_up::game_path(&SPEED_UP);
    let limit = match frame_rate_limit {
        FrameRateLimit::Hz(r) if !crate::base::settings::FRAME_RATE_BOUNDS.contains(&r) => FrameRateLimit::Display,
        other => other,
    };
    let encoded = match limit {
        FrameRateLimit::Display => 0,
        FrameRateLimit::Hz(r) => r,
        FrameRateLimit::Unlimited => UNLIMITED,
    };
    FRAME_RATE_LIMIT.store(encoded, Ordering::Relaxed);
    TARGET_RATE.store(
        match limit {
            FrameRateLimit::Display => display_refresh_rate(),
            FrameRateLimit::Hz(r) => r,
            FrameRateLimit::Unlimited => 0,
        },
        Ordering::Relaxed,
    );
    ON.store(on, Ordering::Relaxed);
    if on && DRAWING_THREAD.load(Ordering::Relaxed) == 0 {
        let spawned = std::thread::Builder::new().name("eu4-harmony drawing".into()).stack_size(32 << 20).spawn(|| crate::base::guarded("drawing thread", run));
        if let Err(e) = spawned {
            log!("drawing: no drawing thread ({e}): the main thread draws");
            ON.store(false, Ordering::Relaxed);
        }
    }
    let where_drawn = if on { "on a thread of its own" } else { "on the main thread" };
    match limit {
        FrameRateLimit::Display => log!("drawing: {where_drawn}, at the display's refresh rate"),
        FrameRateLimit::Hz(rate) => log!("drawing: {where_drawn}, up to {rate} frames a second"),
        FrameRateLimit::Unlimited => log!("drawing: {where_drawn}, no limit"),
    }
}

fn app_frame_before(_: &mut Call) {
    if !on_main() {
        return;
    }
    if main_side(|s| s.may_pace()) && !FRAMES.take_day_ran() {
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
    deferred::clear_minimap();
    let m = crate::native::hooked_memory();
    let mut stage = LiveStage(&m);
    FRAMES.clear_across_deferred(&mut stage, |_| {
        let _guard = PREPARED_WAKE.0.lock().unwrap_or_else(|e| e.into_inner());
        PREPARED_WAKE.1.notify_all();
        drop(_guard);
        main_side(|s| s.quits(&MainLock, || IN_FRAME.load(Ordering::Acquire)));
    });
    deferred::clear_minimap();
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

pub(super) fn prepare_scene_hook(m: &Memory) -> crate::base::Result<()> {
    super::present::prepare_scene(m)
}

/// The daily update begins (`game::events`): it lets the lock go.
pub fn day_begins() {
    if !on_main() || !drawing_on() {
        return;
    }
    FRAMES.note_day_ran();
    crate::native::probes::drawing_day_begins(&crate::native::hooked_memory());
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
    crate::native::probes::frame_check_before(call);
    if on_main() {
        main_side(|s| s.frame_check(&MainLock));
    }
}

fn frame_check_after(call: &mut Call) {
    crate::native::probes::frame_check_after(call);
}

struct LiveStage<'a>(&'a Memory);

impl StageOps for LiveStage<'_> {
    fn run_deferred(&mut self) {
        deferred::done(self.0);
    }

    fn draw_on_main_only(&self) -> bool {
        !drawing_on() || main_draws(self.0)
    }

    fn prepare_for_drawing(&mut self, idler: usize) -> std::result::Result<(), (FallbackMode, String)> {
        if let Err(e) = prepare::scene(self.0) {
            return Err((FallbackMode::BeforeTooltips, format!("the GUI scene could not be prepared: {e}")));
        }
        prepare::tooltips(self.0, idler);
        if let Err(e) = prepare::frame(self.0, idler) {
            return Err((FallbackMode::AfterTooltips, format!("the frame could not be prepared: {e}")));
        }
        if let Err(e) = marks::keep(self.0, idler) {
            log!("drawing: the frame's temporary map highlights could not be kept: {e}");
        }
        Ok(())
    }

    fn draw_inline(&mut self, idler: usize, mode: FallbackMode) {
        icons::discard_placing();
        draw_here(self.0, idler);
        if mode == FallbackMode::BeforeTooltips {
            prepare::tooltips(self.0, idler);
            tooltip::clear();
        }
    }

    fn clear_staged_artifacts(&mut self) {
        icons::clear();
        tooltip::clear();
        marks::clear();
        crossings::clear();
    }

    fn commit_prepared(&mut self, was_idle: bool) {
        crossings::frame_prepared();
        icons::prepared();
        crate::native::probes::frame_prepared();
        CAMERA_MOVED_THERE.store(1, Ordering::Relaxed);
        if was_idle {
            let _guard = PREPARED_WAKE.0.lock().unwrap_or_else(|e| e.into_inner());
            PREPARED_WAKE.1.notify_all();
        }
        // Run main-thread frame-boundary maintenance (font page growth, safe
        // point, text costs, and remote input playback) through `present` so
        // inline main-thread frames and drawing-thread frames share one path.
        let screen = SCREEN.load(Ordering::Relaxed);
        let screen = (screen != 0).then_some(((screen >> 32) as u32, screen as u32));
        super::present::main_frame_prepared(screen);
    }

    fn log(&mut self, line: &str) {
        log!("{line}");
    }
}

/// The in-game frame calls its drawing, on the main thread, which holds the
/// lock.
#[called_by_game]
extern "C" fn frame_draws(idler: usize) {
    let m = crate::native::hooked_memory();
    let mut stage = LiveStage(&m);
    FRAMES.stage_frame(idler, &mut stage);
}

/// Whether the main thread draws this frame itself: the console's debug
/// text, which works a country's values out as it is drawn, or the framework
/// holding frames back.
fn main_draws(m: &Memory) -> bool {
    let debug_text = m.ptr(m.at(DEBUG_COUNTRY)).ok().is_none_or(|c| c != 0);
    debug_text || super::present::holding()
}

/// Whether the drawing thread must not draw the frame prepared: what makes
/// the main thread draw, or the map's names due since, which reads the world.
fn must_draw_here(m: &Memory) -> bool {
    prepare::names_due(m) || main_draws(m)
}

/// Whether the current thread is the drawing thread.
pub fn is_drawing_thread() -> bool {
    crate::base::is_drawing_thread()
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
    deferred::flush_minimap(m);
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

static TARGET_RATE: AtomicU32 = AtomicU32::new(0);

/// The actual target interval used by the drawing thread; the native
/// main-thread path and unlimited drawing have no declared target.
#[cfg(feature = "dev")]
pub fn target_interval() -> Option<Duration> {
    let rate = TARGET_RATE.load(Ordering::Relaxed);
    (drawing_on() && rate != 0).then(|| Duration::from_secs_f64(1.0 / rate as f64))
}

/// The refresh rate of the display showing the game's window, or the primary
/// display's before the window exists; 60 when Windows does not say.
pub(crate) fn display_refresh_rate() -> u32 {
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, DEVMODEW, ENUM_CURRENT_SETTINGS, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
    };
    let rate_of = |device: *const u16| -> Option<u32> {
        let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
        mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        let ok = unsafe { EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut mode) } != 0;
        (ok && crate::base::settings::FRAME_RATE_BOUNDS.contains(&mode.dmDisplayFrequency)).then_some(mode.dmDisplayFrequency)
    };
    if let Some(window) = crate::base::game_window() {
        let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
        if !monitor.is_null() {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) } != 0 {
                if let Some(rate) = rate_of(info.szDevice.as_ptr()) {
                    return rate;
                }
            }
        }
    }
    rate_of(std::ptr::null()).unwrap_or(60)
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

fn maybe_log_pace(base: usize, gaps: &mut Gaps, last_drawn: &mut u64, logged: &mut Instant) {
    if logged.elapsed() < Duration::from_secs(10) {
        return;
    }
    let drawn = DRAWN.load(Ordering::Relaxed);
    let (crossings_taken, held_longest) = crossings::logged(base);
    let (main_waited, waited_at, drawing_waited) = PRESENTATION.swap_waits();
    log!(
        "drawing: {:.1} frames a second, {}; {} drawn by the main thread; the main thread waited {:.0} ms for the lock ({:.0} at frames, {:.0} at crossings, {:.0} at days' ends) and {:.0} ms for frames drawn, the drawing thread {:.0} ms; {} crossing(s), {} deferred, the frames held back longest by {}",
        (drawn - *last_drawn) as f64 / logged.elapsed().as_secs_f64(),
        gaps.told(base),
        HERE.swap(0, Ordering::Relaxed),
        main_waited as f64 / 1000.0,
        waited_at[AT_FRAME] as f64 / 1000.0,
        waited_at[AT_CROSSING] as f64 / 1000.0,
        waited_at[AT_DAY_END] as f64 / 1000.0,
        PACED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
        drawing_waited as f64 / 1000.0,
        crossings_taken,
        deferred::logged(),
        held_longest
    );
    *last_drawn = drawn;
    *logged = Instant::now();
}

/// The drawing thread.
fn run() {
    crate::base::set_drawing_thread(crate::base::thread_id());
    let m = crate::native::hooked_memory();
    let mut display_rate = display_refresh_rate();
    let mut display_checked = Instant::now();
    let mut current_perf_rate = match FRAME_RATE_LIMIT.load(Ordering::Relaxed) {
        0 => display_rate,
        UNLIMITED => 0,
        r => r,
    };
    TARGET_RATE.store(current_perf_rate, Ordering::Relaxed);
    log!("drawing: the drawing thread runs ({display_rate} Hz display)");
    let mut next = Instant::now();
    let mut logged = Instant::now();
    let mut last_drawn = 0u64;
    let mut last_frame = Instant::now();
    let mut gaps = Gaps::default();
    loop {
        if !drawing_on() {
            CAMERA_MOVED_THERE.store(0, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(20));
            next = Instant::now();
            continue;
        }
        if FRAMES.prepared_idler() == 0 {
            CAMERA_MOVED_THERE.store(0, Ordering::Relaxed);
            maybe_log_pace(m.at(0usize), &mut gaps, &mut last_drawn, &mut logged);
            let guard = PREPARED_WAKE.0.lock().unwrap_or_else(|e| e.into_inner());
            if FRAMES.prepared_idler() == 0 && drawing_on() {
                let _ = PREPARED_WAKE.1.wait_timeout(guard, Duration::from_millis(200));
            }
            display_rate = display_refresh_rate();
            display_checked = Instant::now();
            next = Instant::now();
            last_frame = next;
            continue;
        }
        let chosen = FRAME_RATE_LIMIT.load(Ordering::Relaxed);
        if (chosen == 0 || chosen == UNLIMITED) && display_checked.elapsed() >= Duration::from_secs(1) {
            display_rate = display_refresh_rate();
            display_checked = Instant::now();
        }
        let perf_rate = match chosen {
            0 => display_rate,
            UNLIMITED => 0,
            r => r,
        };
        let pace_rate = match chosen {
            0 | UNLIMITED => display_rate,
            r => r,
        };
        let interval = Duration::from_secs_f64(1.0 / pace_rate as f64);
        if perf_rate != current_perf_rate {
            current_perf_rate = perf_rate;
            TARGET_RATE.store(perf_rate, Ordering::Relaxed);
            next = Instant::now();
        }
        if chosen != UNLIMITED {
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
        let mut drew = false;
        if let Some((idler, seq)) = FRAMES.drawable(current, held, must_draw_here(&m)) {
            DRAWING_LENGTH.store(length.to_bits(), Ordering::Relaxed);
            if let Err(e) = icons::follow_camera(&m) {
                log!("drawing: the map icons could not follow the camera: {e}");
            }
            if let Err(e) = tooltip::follow_cursor(&m, idler) {
                log!("drawing: the tooltip could not follow the cursor: {e}");
            }
            deferred::flush_minimap(&m);
            IN_FRAME.store(true, Ordering::Release);
            let watched = crate::native::probes::drawing_frame_begins(&m);
            drawing_side(|d| d.frame_begins(watched));
            let began = Instant::now();
            unsafe { direct1(&m, DRAWING)(idler) };
            let drawing = began.elapsed();
            let (present, lock_after) = PRESENT.with(|p| (p.spent.take(), p.lock_after.take()));
            drawing_side(DrawingSide::frame_ends);
            crate::native::probes::drawing_frame_ends(watched);
            IN_FRAME.store(false, Ordering::Release);
            DRAWN.fetch_add(1, Ordering::Relaxed);
            FRAMES.note_drawn(seq);
            gaps.drawn(Instant::now(), interval, drawing, present, lock_after);
            drew = true;
        }
        drawing_side(|d| d.turn_ends(&DrawingLock));
        if chosen == UNLIMITED && !drew {
            wait_until(Instant::now() + interval);
        }
        maybe_log_pace(m.at(0usize), &mut gaps, &mut last_drawn, &mut logged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_lock_keeps_atomic_mirrors_in_sync_with_main_side() {
        struct LocalLock<'a>(&'a PresentationLock);
        impl turns::Lock for LocalLock<'_> {
            fn take(&self, at: At) -> Duration {
                self.0.take_main(at)
            }
            fn give(&self) {
                self.0.give_main();
            }
        }

        let lock = PresentationLock::new();
        let adapter = LocalLock(&lock);
        assert!(!lock.main_holds());
        assert!(!lock.main_protects_changes());

        lock.with_main(|s| s.frame_begins(&adapter));
        assert!(lock.main_holds());
        assert!(lock.main_protects_changes());

        lock.with_main(|s| s.day_begins(&adapter));
        assert!(!lock.main_holds());
        assert!(!lock.main_protects_changes());

        lock.with_main(|s| {
            s.order_scope_begins();
            assert!(s.lock_order(&adapter));
        });
        assert!(lock.main_holds());
        assert!(!lock.main_protects_changes(), "order-only scope must not claim change protection");

        lock.with_main(|s| {
            let _ = s.crossing(&adapter);
        });
        assert!(lock.main_holds());
        assert!(lock.main_protects_changes());

        lock.with_main(|s| s.order_scope_ends(&adapter, false));
        assert!(lock.main_holds(), "promoted crossing keeps ownership past scope exit");
        assert!(lock.main_protects_changes());

        lock.with_main(|s| s.frame_check(&adapter));
        assert!(!lock.main_holds());
        assert!(!lock.main_protects_changes());
    }

    #[test]
    fn frame_coordinator_handles_publication_pacing_main_fallback_and_idler_switches() {
        let frames = FrameCoordinator::new();

        assert_eq!(frames.drawable(0x100, None, false), None);
        let seq1 = frames.publish_prepared(0x100);
        assert_eq!(seq1, 1);
        assert_eq!(frames.pace_target(true, false, true), Some(1));
        assert_eq!(frames.drawable(0x100, None, false), Some((0x100, 1)));

        frames.note_drawn(1);
        assert_eq!(frames.pace_target(true, false, true), None);

        let seq2 = frames.publish_prepared(0x100);
        assert_eq!(seq2, 2);
        assert_eq!(frames.drawable(0x100, Some(0x1400_1234), false), None);
        assert_eq!(frames.pace_target(true, true, true), None);

        frames.main_draws_frame();
        assert_eq!(frames.drawable(0x100, None, false), None);
        assert_eq!(frames.pace_target(true, false, true), None);

        let seq3 = frames.publish_prepared(0x200);
        assert_eq!(frames.drawable(0x200, None, false), Some((0x200, seq3)));
        frames.clear_idler();
        assert_eq!(frames.drawable(0x200, None, false), None);
    }

    #[test]
    fn gaps_attributes_late_frames_to_crossing_or_dominant_stage() {
        let mut gaps = Gaps::default();
        let t0 = Instant::now();
        let interval = Duration::from_millis(10);
        gaps.drawn(t0, interval, Duration::from_millis(2), Duration::from_millis(1), Duration::ZERO);

        gaps.held_by(0x1400_0500);
        gaps.drawn(t0 + Duration::from_millis(30), interval, Duration::from_millis(3), Duration::from_millis(1), Duration::ZERO);

        gaps.drawn(t0 + Duration::from_millis(65), interval, Duration::from_millis(28), Duration::from_millis(25), Duration::from_millis(1));

        let line = gaps.told(0x1400_0000);
        assert!(line.starts_with("2 late (held back 1 by 0x500 1,"), "{line}");
        assert!(line.contains("Present 1"), "{line}");
    }

    #[derive(Default)]
    struct RecordingStage {
        main_only: bool,
        fail_with: Option<(FallbackMode, &'static str)>,
        dirty_on_deferred: bool,
        staged_dirty: bool,
        deferred_runs: usize,
        clears: usize,
        logs: Vec<String>,
        drawn_inline: Vec<(usize, FallbackMode)>,
        commits: Vec<bool>,
    }

    impl StageOps for RecordingStage {
        fn run_deferred(&mut self) {
            self.deferred_runs += 1;
            if self.dirty_on_deferred {
                self.staged_dirty = true;
            }
        }
        fn draw_on_main_only(&self) -> bool {
            self.main_only
        }
        fn prepare_for_drawing(&mut self, _: usize) -> Result<(), (FallbackMode, String)> {
            self.staged_dirty = true;
            match self.fail_with {
                Some((mode, reason)) => Err((mode, reason.into())),
                None => Ok(()),
            }
        }
        fn draw_inline(&mut self, idler: usize, mode: FallbackMode) {
            self.drawn_inline.push((idler, mode));
        }
        fn clear_staged_artifacts(&mut self) {
            self.staged_dirty = false;
            self.clears += 1;
        }
        fn commit_prepared(&mut self, was_idle: bool) {
            self.commits.push(was_idle);
        }
        fn log(&mut self, line: &str) {
            self.logs.push(line.to_string());
        }
    }

    #[test]
    fn stage_frame_clears_prepared_state_and_falls_back_when_prepare_fails() {
        let frames = FrameCoordinator::new();
        let mut stage = RecordingStage::default();

        frames.stage_frame(0x100, &mut stage);
        assert_eq!(frames.prepared_idler(), 0x100);
        assert_eq!(stage.commits, [true]);
        assert_eq!(stage.deferred_runs, 1);
        assert!(stage.staged_dirty);
        assert!(stage.drawn_inline.is_empty());

        frames.stage_frame(0x100, &mut stage);
        assert_eq!(stage.commits, [true, false]);

        stage.fail_with = Some((FallbackMode::AfterTooltips, "the frame could not be prepared: missing idler pointer"));
        frames.stage_frame(0x100, &mut stage);
        assert_eq!(frames.prepared_idler(), 0);
        assert_eq!(frames.drawable(0x100, None, false), None);
        assert_eq!(frames.pace_target(true, false, true), None);
        assert!(!stage.staged_dirty);
        assert_eq!(stage.drawn_inline, [(0x100, FallbackMode::AfterTooltips)]);
        assert_eq!(stage.logs.len(), 1);
        assert!(stage.logs[0].contains("missing idler pointer"));
    }

    #[test]
    fn stage_frame_draws_inline_before_tooltips_on_main_only_and_scene_failure() {
        let frames = FrameCoordinator::new();
        let mut main_only = RecordingStage { main_only: true, ..Default::default() };
        frames.stage_frame(0x100, &mut main_only);
        assert_eq!(frames.prepared_idler(), 0);
        assert_eq!(main_only.drawn_inline, [(0x100, FallbackMode::BeforeTooltips)]);
        assert!(main_only.commits.is_empty());

        let mut scene_fail = RecordingStage::default();
        frames.stage_frame(0x100, &mut scene_fail);
        scene_fail.fail_with = Some((FallbackMode::BeforeTooltips, "the GUI scene could not be prepared: missing GUI scene"));
        frames.stage_frame(0x100, &mut scene_fail);
        assert_eq!(frames.prepared_idler(), 0);
        assert_eq!(frames.drawable(0x100, None, false), None);
        assert!(!scene_fail.staged_dirty);
        assert_eq!(scene_fail.drawn_inline, [(0x100, FallbackMode::BeforeTooltips)]);
        assert!(scene_fail.logs.iter().any(|l| l.contains("missing GUI scene")));
    }

    #[test]
    fn clear_across_deferred_reclears_artifacts_dirtied_during_deferred_work() {
        let frames = FrameCoordinator::new();
        let mut stage = RecordingStage { dirty_on_deferred: true, ..Default::default() };
        frames.stage_frame(0x200, &mut stage);
        let mut waited = false;

        frames.clear_across_deferred(&mut stage, |s| {
            assert!(!s.staged_dirty, "artifacts must be cleared before waiting for the drawing thread");
            waited = true;
        });
        assert!(waited);
        assert_eq!(frames.prepared_idler(), 0);
        assert!(!stage.staged_dirty, "artifacts dirtied during deferred work must be cleared again");
        assert!(stage.clears >= 2);
    }
}
