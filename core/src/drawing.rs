//! The drawing thread (GLOSSARY; docs/plans/drawing-thread.md, T1): the
//! in-game frame's drawing done by a thread of the framework's, at a pace of
//! its own, while the main thread goes on with the day.
//!
//! The in-game frame calls its drawing (`0x8183b0`) last, right after the
//! GUI's update. With the drawing thread on, the main thread does there only
//! the drawing's two first steps, which read the world: the ambient sound by
//! the terrain under the camera, and the map mode's colors when they changed
//! (docs/research/drawing-beside-the-day.md, "The drawing's first steps");
//! then it notes the frame as prepared, and goes on. The drawing thread draws
//! the prepared frame again and again, at the display's refresh rate or with
//! no limit, through the game's own drawing, whose first steps it skips.
//!
//! The two take turns through the presentation lock (GLOSSARY), a lock that
//! serves whoever asked first. The drawing thread holds it for each frame it
//! draws. The main thread holds it through each application frame, but lets
//! it go while the daily update runs, taking it again for a frame inside the
//! day, at the day's end, and at each crossing (GLOSSARY) the day reaches,
//! which keeps it until the next of the game's checks for a frame inside a
//! day. So the interface's own objects are never used by both at once.
//!
//! The main thread draws a frame itself, as the game does, when the
//! drawing thread is off, when the map's names are to be made again (which
//! reads the countries), when the console's debug text is on (which works a
//! country's values out as it is drawn), and while the framework holds frames
//! back.
use crate::called_by_game;
use crate::hooks::Call;
use crate::memory::Memory;
use crate::points::{GameFunction, Point};
use crate::speed::SpeedUp;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

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
/// The settings' master volume, and the floor above which the ambient sound
/// is worked out.
const MASTER_VOLUME: usize = 0xb4;
const VOLUME_FLOOR: usize = 0x1dc19d8;
/// The ambient sound: the idler's sound object, the graphical map, the camera.
const AMBIENT: GameFunction<3> = GameFunction::at(0x6e7a50);
/// The map mode's colors: the graphical map, the map mode, a second value.
const COLORS: GameFunction<3> = GameFunction::at(crate::map::RENDERER);
/// The clock the colors' refresh is timed by: writes the time at its
/// argument and returns it.
const CLOCK: GameFunction<1> = GameFunction::at(0x5bf840);
/// The in-game idler's fields the first steps read.
const IDLER_SOUND: usize = 0x5a0;
const IDLER_MAP: usize = 0x11c0;
const IDLER_CAMERA: usize = 0x1200;
const IDLER_COLORS_DUE: usize = 0x34;
const IDLER_COLORS_ASKED: usize = 0x44;
const IDLER_COLORS_AT: usize = 0x38;
const IDLER_MAP_MODE: usize = 0x2c;
const IDLER_MAP_MODE_ARG: usize = 0x40;
/// The application, and its graphical map.
const APP: usize = 0x242bf50;
const APP_MAP: usize = 0x360;
/// The graphical map's flag that its names are to be made again.
const MAP_NAMES_DUE: usize = 0x25a0;
/// The application's current idler.
const APP_IDLER: usize = 0x40;
/// The console's debug country, whose values the debug text works out.
const DEBUG_COUNTRY: usize = 0x2340040;
/// The application's frame: the application and whether it is a normal one
/// (0 for a frame inside a day).
const APP_FRAME: usize = 0x14c25a0;
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
const IDLER_CAMERA_A: usize = 0x68;
const IDLER_CAMERA_B: usize = 0x60;
/// What keeps the camera from being updated: the idler's `[+0x1b80]+0x18`
/// and `[+0x3b0]+0xc8`, a global switch for another camera, and the
/// application's focus (`+0xb8`).
const IDLER_HOLDS_CAMERA: usize = 0x1b80;
const IDLER_HOLDS_CAMERA_2: usize = 0x3b0;
const OTHER_CAMERA: usize = 0x233ff4f;
const APP_FOCUS: usize = 0xb8;

fn points() -> Vec<Point> {
    vec![
        Point::jump(FRAME_DRAWS, "49 8b cd e8 8f 2a 00 00", "the in-game frame calls its drawing", frame_draws_stub as *const () as usize, &FRAME_RESUME),
        Point::keep(DRAWING, "48 8b c4 48 89 58 08 55 56 57 41 54 41 55 41 56 41 57", "the in-game frame's drawing"),
        Point::jump(HEAD, "48 8b 05 fa 37 c1 01", "the drawing's first steps", head_stub as *const () as usize, &HEAD_RESUME)
            .uses(&[SETTINGS])
            .recorded(&HEAD_AT),
        // cmp byte ptr [r15 + 0x34], 0; jne; cmp byte ptr [r15 + 0x44], 0; je
        Point::keep(0x818619, "41 80 7f 34 00 75 07 41 80 7f 44 00 74 3a", "the map mode's colors' flags").uses(&[IDLER_COLORS_DUE, IDLER_COLORS_ASKED]),
        Point::keep(HEAD_END, "49 8b 8f f8 11 00 00", "where the drawing's first steps end"),
        // comiss xmm0, [floor]: the ambient sound's floor of the master volume.
        Point::keep(0x8185f6, "0f 2f 05 db 93 5a 01", "the ambient sound's volume floor").uses(&[VOLUME_FLOOR]),
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
        Point::keep(CAMERA_UPDATE, "48 89 5c 24 08 57 48 83 ec 70", "the camera's update"),
        // mov rcx, [r14 + 0x1b80]; cmp byte ptr [rcx + 0x18], 0; ...;
        // mov rax, [r14 + 0x3b0]; cmp byte ptr [rax + 0xc8], 0; ...;
        // cmp byte ptr [other camera], 0
        Point::keep(0x831137, "49 8b 8e 80 1b 00 00 80 79 18 00", "what holds the camera, first").uses(&[IDLER_HOLDS_CAMERA]),
        Point::keep(0x831144, "49 8b 86 b0 03 00 00 80 b8 c8 00 00 00 00", "what holds the camera, second").uses(&[IDLER_HOLDS_CAMERA_2]),
        Point::keep(0x831154, "80 3d f4 ed b0 01 00", "the other camera's switch").uses(&[OTHER_CAMERA]),
        // mov rcx, [application]: the in-game frame's GUI update takes it.
        Point::keep(0x81590d, "48 8b 0d 3c 66 c1 01", "the application, in the in-game frame").uses(&[APP]),
        Point::keep(AMBIENT, "48 8b c4 4c 89 40 18 48 89 50 10 48 89 48 08 55 53 56 57", "the ambient sound"),
        Point::keep(CLOCK, "48 89 5c 24 08 57 48 83 ec 20 48 8b d9", "the colors' clock"),
        Point::observe(APP_FRAME, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18", "the application's frame", 2, Some(app_frame_before), Some(app_frame_after)),
        Point::observe(
            FRAME_CHECK,
            "48 8b c4 48 89 58 10 4c 89 40 18 55 56 57",
            "the check for a frame inside a day",
            4,
            Some(frame_check_before),
            Some(frame_check_after),
        ),
    ]
    .into_iter()
    .chain(CROSSINGS_AT.iter().map(|&(rva, bytes, what, argc)| Point::observe(rva, bytes, what, argc, Some(crossed), None)))
    .collect()
}

/// The crossings the day reaches, at their entries (docs/research/drawing-beside-the-day.md,
/// "Where the day reaches the interface and the graphics", "The crossings a
/// late day reaches"): the RVA, its first bytes, what it is, the arguments it
/// reads.
const CROSSINGS_AT: [(usize, &str, &str, u32); 16] = [
    (0x8339f0, "4c 8b dc 49 89 5b 18", "the interface's daily refresh", 1),
    (0x81e280, "40 55 53 56 57 41 54", "the interface's monthly refresh", 1),
    (0x6e9860, "48 89 5c 24 10", "a text floating on the map", 6),
    (0x1448880, "48 8b c4 48 89 58 10", "a unit's model", 2),
    (0x8c1430, "4c 89 44 24 18", "a message's popup", 3),
    (0x10a03f0, "48 89 5c 24 18", "a dialog", 5),
    (0x821ff0, "48 89 5c 24 08", "the map mode set", 4),
    (0x1347a50, "48 89 5c 24 10", "the map's markers of a province", 3),
    (0x13238b0, "48 8b c4 4c 89 40 18", "a port's model", 4),
    (0xded0f0, "48 89 5c 24 18", "the new age's window", 1),
    (0xde6300, "48 89 5c 24 10", "a combat's result drawn into a texture", 2),
    (0xe428b0, "48 89 5c 24 08", "a country's dialog", 4),
    (0x10b6c50, "48 8b c4 48 89 58 10", "the minimap", 2),
    (0x123f800, "48 89 5c 24 10", "an item of the message list", 4),
    (0x1449da0, "40 53 48 83 ec 20", "a unit's model let go", 1),
    (0x10af990, "4c 8b c9 83 fa ff", "the map told of a province's new owner", 2),
];

fn crossed(_: &mut Call) {
    crossing();
}

crate::resume!(FRAME_RESUME: FRAME_DRAWN_AT = FRAME_DRAWN);
crate::resume!(HEAD_RESUME: HEAD_GOES_ON_AT = HEAD_GOES_ON, HEAD_END_AT = HEAD_END);
static HEAD_AT: AtomicUsize = AtomicUsize::new(0);
crate::resume!(CAMERA_RESUME: CAMERA_LENGTH_TAKEN_AT = CAMERA_LENGTH_TAKEN);
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
    CAMERA_UPDATE.call(m, [camera, m.ptr(idler + IDLER_CAMERA_A)?, m.ptr(idler + IDLER_CAMERA_B)?, NO_EVENTS.as_ptr() as usize, length.to_bits() as usize])?;
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
struct Lock {
    tickets: Mutex<(u64, u64)>,
    turn: Condvar,
}

impl Lock {
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

static LOCK: Lock = Lock::new();

/// The drawing thread's ID, 0 until it runs; the head's stub compares it.
static DRAWING_THREAD: AtomicU32 = AtomicU32::new(0);
/// The in-game idler whose frame the main thread prepared last, 0 for none.
static PREPARED: AtomicUsize = AtomicUsize::new(0);
/// How many frames the main thread prepared, and the last of them the
/// drawing thread has drawn: the next frame's update waits until it has, as
/// it reads what a drawing worked out of the last update (the map's markers'
/// lists), which the game's frame always drew between two updates.
static PREPARED_SEQ: AtomicU64 = AtomicU64::new(0);
/// A crossing changed the interface's state since the last frame prepared:
/// what it began (a unit's model made, its type's list grown) the next
/// update finishes, and the game never draws between the two, so neither
/// does the drawing thread.
static CHANGED: AtomicBool = AtomicBool::new(false);
static DRAWN_SEQ: AtomicU64 = AtomicU64::new(0);
/// The drawing thread failed: the main thread draws from then on.
static FAILED: AtomicBool = AtomicBool::new(false);
/// Whether the setting has the drawing thread on, as last told.
static ON: AtomicBool = AtomicBool::new(false);
static UNLIMITED: AtomicBool = AtomicBool::new(false);
/// The frames the drawing thread drew, and the main thread's waits for the
/// lock in microseconds, for the log.
static DRAWN: AtomicU64 = AtomicU64::new(0);
static MAIN_WAITED: AtomicU64 = AtomicU64::new(0);
static DRAWING_WAITED: AtomicU64 = AtomicU64::new(0);
static CROSSINGS: AtomicU64 = AtomicU64::new(0);
/// The drawing thread's frames that faulted, and how many it may before the
/// main thread draws again.
static FAULTS: AtomicU64 = AtomicU64::new(0);
const MOST_FAULTS: u64 = 5;
/// The frames the main thread drew itself, the drawing thread not having.
static HERE: AtomicU64 = AtomicU64::new(0);
/// The back buffer's size, as the drawing thread's last frame presented it.
static SCREEN: AtomicU64 = AtomicU64::new(0);

/// The main thread's side of the lock: what only the main thread changes.
struct MainSide {
    holds: bool,
    /// For each application frame on the way, whether it took the lock.
    frames: Vec<bool>,
    /// The day let the lock go as it began.
    day_gave: bool,
    in_day: bool,
    /// A crossing took the lock, kept until the next check for a frame.
    crossing: bool,
}

static MAIN: Mutex<MainSide> = Mutex::new(MainSide { holds: false, frames: Vec::new(), day_gave: false, in_day: false, crossing: false });

fn main_side<T>(f: impl FnOnce(&mut MainSide) -> T) -> T {
    f(&mut MAIN.lock().unwrap_or_else(|e| e.into_inner()))
}

fn main_take(s: &mut MainSide) {
    if !s.holds {
        let waited = LOCK.take();
        MAIN_WAITED.fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        s.holds = true;
    }
}

fn main_give(s: &mut MainSide) {
    if s.holds {
        s.holds = false;
        LOCK.give();
    }
}

/// Whether the drawing thread draws the frames now.
fn drawing_on() -> bool {
    ON.load(Ordering::Relaxed) && !FAILED.load(Ordering::Relaxed) && DRAWING_THREAD.load(Ordering::Relaxed) != 0
}

fn on_main() -> bool {
    crate::thread_id() == crate::main_thread()
}

/// Tells the drawing thread the settings it runs with: at startup, and when
/// settings are saved. Starts the thread the first time it is turned on.
pub fn set(on: bool, unlimited: bool) {
    UNLIMITED.store(unlimited, Ordering::Relaxed);
    ON.store(on, Ordering::Relaxed);
    if on && DRAWING_THREAD.load(Ordering::Relaxed) == 0 && !FAILED.load(Ordering::Relaxed) {
        let spawned = std::thread::Builder::new().name("eu4-harmony drawing".into()).stack_size(32 << 20).spawn(|| crate::guarded("drawing thread", run));
        if let Err(e) = spawned {
            log!("drawing: no drawing thread ({e}): the main thread draws");
            FAILED.store(true, Ordering::Relaxed);
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
    wait_for_drawn();
    main_side(|s| {
        let took = !s.holds;
        main_take(s);
        s.frames.push(took);
    });
    if drawing_on() {
        draw_if_not_drawn(&crate::hooked_memory());
    }
}

fn app_frame_after(_: &mut Call) {
    if !on_main() {
        return;
    }
    main_side(|s| {
        if s.frames.pop() == Some(true) {
            s.crossing = false;
            main_give(s);
        }
    });
}

/// Waits a while for the drawing thread to draw the last frame prepared,
/// which it can, the main thread not holding the lock then.
fn wait_for_drawn() {
    let wanted = PREPARED_SEQ.load(Ordering::Acquire);
    if DRAWN_SEQ.load(Ordering::Acquire) >= wanted || !drawing_on() || CHANGED.load(Ordering::Acquire) || main_side(|s| s.holds) {
        return;
    }
    let began = Instant::now();
    while DRAWN_SEQ.load(Ordering::Acquire) < wanted && began.elapsed() < Duration::from_millis(30) {
        std::thread::sleep(Duration::from_micros(200));
    }
    MAIN_WAITED.fetch_add(began.elapsed().as_micros() as u64, Ordering::Relaxed);
}

/// The last frame prepared, drawn by the main thread, which holds the lock,
/// when the drawing thread has not drawn it: it skipped it (the map's names
/// to be made again) or did not come to it.
fn draw_if_not_drawn(m: &Memory) {
    let wanted = PREPARED_SEQ.load(Ordering::Acquire);
    if DRAWN_SEQ.load(Ordering::Acquire) >= wanted {
        return;
    }
    // Changed since it was prepared: the update this frame begins finishes
    // what changed, and its own frame is drawn.
    if CHANGED.load(Ordering::Acquire) {
        DRAWN_SEQ.store(wanted, Ordering::Release);
        return;
    }
    let idler = PREPARED.load(Ordering::Acquire);
    let current = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
    if idler != 0 && idler == current {
        HERE.fetch_add(1, Ordering::Relaxed);
        if let Err(e) = DRAWING.call(m, [idler]) {
            log!("drawing: the frame's drawing failed: {e}");
        }
    }
    DRAWN_SEQ.store(wanted, Ordering::Release);
}

/// The daily update begins (`game::events`): it lets the lock go.
pub fn day_begins() {
    if !on_main() || !drawing_on() || std::env::var_os("EU4H_DRAWING_NO_DAY").is_some() {
        return;
    }
    main_side(|s| {
        s.in_day = true;
        s.day_gave = s.holds;
        main_give(s);
    });
}

/// The daily update has returned (`game::events`): the frame it ran in has
/// the lock again.
pub fn day_ends() {
    if !on_main() {
        return;
    }
    main_side(|s| {
        if !s.in_day {
            return;
        }
        s.in_day = false;
        s.crossing = false;
        if s.day_gave {
            main_take(s);
            s.day_gave = false;
        }
    });
}

fn frame_check_before(call: &mut Call) {
    #[cfg(feature = "dev-crossings")]
    crate::crossings::check_before(call);
    let _ = call;
    if on_main() {
        main_side(|s| {
            if s.crossing {
                s.crossing = false;
                main_give(s);
            }
        });
    }
}

fn frame_check_after(call: &mut Call) {
    #[cfg(feature = "dev-crossings")]
    crate::crossings::check_after(call);
    let _ = call;
}

/// The day reaches the interface or the graphics: the lock first, kept until
/// the next check for a frame inside the day.
pub fn crossing() {
    if !on_main() {
        return;
    }
    main_side(|s| {
        if s.in_day {
            CHANGED.store(true, Ordering::Release);
        }
        if s.in_day && !s.holds {
            CROSSINGS.fetch_add(1, Ordering::Relaxed);
            main_take(s);
            s.crossing = true;
        }
    });
}

/// The in-game frame calls its drawing, on the main thread, which holds the
/// lock.
#[called_by_game]
extern "C" fn frame_draws(idler: usize) {
    let m = crate::hooked_memory();
    // This frame's update has finished whatever the day's crossings began.
    CHANGED.store(false, Ordering::Release);
    if !drawing_on() || must_draw_here(&m) {
        if let Err(e) = DRAWING.call(&m, [idler]) {
            log!("drawing: the frame's drawing failed: {e}");
        }
        return;
    }
    if let Err(e) = first_steps(&m, idler) {
        log!("drawing: the drawing's first steps failed, the main thread draws from now on: {e}");
        FAILED.store(true, Ordering::Relaxed);
        return;
    }
    // The first frame of an idler is drawn here: the frame's update reads
    // what a drawing worked out (the map's views for its markers), and none
    // has been drawn for it yet.
    if PREPARED.swap(idler, Ordering::AcqRel) != idler {
        if let Err(e) = DRAWING.call(&m, [idler]) {
            log!("drawing: the frame's drawing failed: {e}");
        }
        return;
    }
    CHANGED.store(false, Ordering::Release);
    PREPARED_SEQ.fetch_add(1, Ordering::AcqRel);
    CAMERA_MOVED_THERE.store(1, Ordering::Relaxed);
    // What the main thread does as it presents a frame: the safe point of a
    // frame outside a game, and the input taken for the next.
    crate::safe_point::presented();
    let screen = SCREEN.load(Ordering::Relaxed);
    if screen != 0 {
        crate::game::input::presenting(((screen >> 32) as u32, screen as u32));
    }
}

/// Whether the main thread draws this frame itself.
fn must_draw_here(m: &Memory) -> bool {
    let names_due = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_MAP)).and_then(|map| m.u8(map + MAP_NAMES_DUE)).ok().is_none_or(|due| due != 0);
    let debug_text = m.ptr(m.at(DEBUG_COUNTRY)).ok().is_none_or(|c| c != 0);
    names_due || debug_text || crate::framework_ui::holding()
}

/// The drawing's first steps, as the drawing does them (`0x8185e7`–`0x81865c`).
fn first_steps(m: &Memory, idler: usize) -> crate::Result<()> {
    let settings = m.ptr(m.at(SETTINGS))?;
    let volume = f32::from_bits(m.u32(settings + MASTER_VOLUME)?);
    let floor = f32::from_bits(m.u32(m.at(VOLUME_FLOOR))?);
    if volume > floor {
        AMBIENT.call(m, [idler + IDLER_SOUND, m.ptr(idler + IDLER_MAP)?, m.ptr(idler + IDLER_CAMERA)?])?;
    }
    if m.u8(idler + IDLER_COLORS_DUE)? != 0 || m.u8(idler + IDLER_COLORS_ASKED)? != 0 {
        let mut now = 0i64;
        let at = CLOCK.call(m, [&mut now as *mut i64 as usize])?;
        let now = if at != 0 { m.u64(at)? as i64 } else { now };
        if now >= m.u64(idler + IDLER_COLORS_AT)? as i64 {
            let map = m.ptr(m.ptr(m.at(APP))? + APP_MAP)?;
            COLORS.call(m, [map, m.u32(idler + IDLER_MAP_MODE)? as usize, m.u32(idler + IDLER_MAP_MODE_ARG)? as usize])?;
            m.write(idler + IDLER_COLORS_DUE, &[0])?;
            m.write(idler + IDLER_COLORS_ASKED, &[0])?;
        }
    }
    Ok(())
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
    let interval = Duration::from_secs_f64(1.0 / rate as f64);
    log!("drawing: the drawing thread runs, at {rate} frames a second unless no limit");
    let mut next = Instant::now();
    let mut logged = Instant::now();
    let mut last_drawn = 0u64;
    let mut last_frame = Instant::now();
    loop {
        if !drawing_on() {
            CAMERA_MOVED_THERE.store(0, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(20));
            next = Instant::now();
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
        let waited = LOCK.take();
        DRAWING_WAITED.fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        let seq = PREPARED_SEQ.load(Ordering::Acquire);
        let idler = PREPARED.load(Ordering::Acquire);
        let current = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
        // The camera moves at this thread's pace, also while a frame waits
        // for its update; the main thread's update of it then passes no time.
        if idler != 0 && current == idler && CAMERA_MOVED_THERE.load(Ordering::Relaxed) != 0 {
            let length = last_frame.elapsed().as_secs_f32().min(0.1);
            last_frame = Instant::now();
            if let Err(e) = move_camera(&m, idler, length) {
                log!("drawing: the camera's update failed, the main thread moves it from now on: {e}");
                CAMERA_MOVED_THERE.store(0, Ordering::Relaxed);
            }
        }
        if idler != 0 && current == idler && !CHANGED.load(Ordering::Acquire) && !must_draw_here(&m) {
            match DRAWING.call(&m, [idler]) {
                Ok(_) => {
                    DRAWN.fetch_add(1, Ordering::Relaxed);
                    DRAWN_SEQ.store(seq, Ordering::Release);
                }
                Err(e) => {
                    // Most likely a structure of the interface's a crossing not
                    // caught began to change: the frame waits for the update
                    // that finishes it, as after a crossing.
                    CHANGED.store(true, Ordering::Release);
                    let faults = FAULTS.fetch_add(1, Ordering::Relaxed) + 1;
                    if faults >= MOST_FAULTS {
                        log!("drawing: the drawing thread's frame failed {faults} times, the main thread draws from now on: {e}");
                        FAILED.store(true, Ordering::Relaxed);
                    } else {
                        log!("drawing: the drawing thread's frame failed, left out: {e}");
                    }
                }
            }
        }
        LOCK.give();
        if logged.elapsed() >= Duration::from_secs(10) {
            let drawn = DRAWN.load(Ordering::Relaxed);
            log!("drawing: prepared {idler:#x}, current {current:#x}, drawn here {}", must_draw_here(&m));
            log!(
                "drawing: {:.1} frames a second, {} drawn by the main thread; the main thread waited {:.0} ms for the lock, the drawing thread {:.0} ms; {} crossing(s)",
                (drawn - last_drawn) as f64 / logged.elapsed().as_secs_f64(),
                HERE.swap(0, Ordering::Relaxed),
                MAIN_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                DRAWING_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                CROSSINGS.swap(0, Ordering::Relaxed)
            );
            last_drawn = drawn;
            logged = Instant::now();
        }
    }
}
