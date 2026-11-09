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
//! draws, but for Direct3D's Present, which may wait for the display. The
//! main thread holds it through each application frame, but lets it go while
//! the daily update runs, taking it again for a frame inside the day, at the
//! day's end, and at each crossing (GLOSSARY) the day reaches, which keeps it
//! until the next of the game's checks for a frame inside a day. So the
//! interface's own objects are never used by both at once. After a crossing
//! the drawing thread draws nothing until the next update, which finishes
//! what the crossing began (a unit's model let go stays on the map's lists
//! until then).
//!
//! The day's two most frequent calls into the interface, a text floating on
//! the map and the interface's daily refresh, answer nothing the day reads:
//! they are deferred to the next application frame, before its update, so
//! that the day neither waits for the lock there nor holds the frames back
//! (docs/plans/drawing-thread.md, C2).
//!
//! The game's frames are paced by Present; a frame that runs no day waits
//! for the drawing thread to draw the last one instead.
//!
//! The main thread draws a frame itself, as the game does, when the
//! drawing thread is off, when the map's names are to be made again (which
//! reads the countries), when the console's debug text is on (which works a
//! country's values out as it is drawn), and while the framework holds frames
//! back.
use crate::called_by_game;
use crate::game_string;
use crate::hooks::Call;
use crate::memory::Memory;
use crate::objects::campaign::globals::WORLD;
use crate::points::{Function, GameFunction, Point};
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
        Point::keep(AMBIENT, "48 8b c4 4c 89 40 18 48 89 50 10 48 89 48 08 55 53 56 57", "the ambient sound"),
        Point::keep(CLOCK, "48 89 5c 24 08 57 48 83 ec 20 48 8b d9", "the colors' clock"),
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
    ]
    .into_iter()
    .chain([
        Point::observe(PLACE_RESET, "48 8b c4 48 89 58 10 48", "a unit's place worked out again", 2, Some(place_reset), None),
        // test dl, dl; je; call; mov rcx, [rbx + 0x70]; test rcx, rcx; je;
        // mov rax, [rcx]; lea edx, [rdi + 1]; call [rax]; mov [rbx + 0x70], rdi
        Point::keep(0x14488c3, "84 d2 74 24 e8 64 f3 ff ff 48 8b 4b 70 48 85 c9 74 0c 48 8b 01 8d 57 01 ff 10 48 89 7b 70", "a unit's place's model let go")
            .uses(&[PLACE_MODEL]),
        Point::replace(MAP_TEXT, "48 89 5c 24 10 4c 89 4c 24 20", "a text floating on the map", map_text as MapText, &ORIGINAL_MAP_TEXT),
        // mov rcx, rsi; call string destructor: the text's type destroyed.
        Point::keep(0x6e9f68, "48 8b ce e8 f0 b6 9a ff 90 48 8b 8d a8 00 00 00", "a floating text's strings destroyed"),
        Point::replace(DAILY_REFRESH, "4c 8b dc 49 89 5b 18 55 56 57", "the interface's daily refresh", daily_refresh as DailyRefresh, &ORIGINAL_DAILY_REFRESH),
        // mov rcx, [r13 + 0x1e00]; test rcx, rcx; je; mov rax, [rcx];
        // call [rax + 0x2a8]: the day's call of the interface's daily refresh.
        Point::keep(0x75c5e7, "49 8b 8d 00 1e 00 00 48 85 c9 74 09 48 8b 01 ff 90 a8 02 00 00", "the day's daily refresh of the interface")
            .uses(&[WORLD_INTERFACE, 0x2a8]),
        Point::observe(SCENE_BEGINS, "48 89 5c 24 08 57 48 83 ec 20 48 8b f9", "the renderer begins a scene", 1, Some(scene_begins), None),
        // mov qword ptr [rdi + 0x108], 0: the vertex declaration it keeps
        // forgotten.
        Point::keep(0x16d4e2f, "48 c7 87 08 01 00 00 00 00 00 00", "a scene forgets the renderer's vertex declaration").uses(&[RENDERER_DECLARATION]),
        Point::observe(MESH_LET_GO, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18", "a mesh object's destructor", 2, Some(mesh_let_go), None),
        // lea rbx, [rdi + 0x78]; cmp rbx, rsi: the application's frame
        // switches its idler, the pending one at hand.
        Point::jump(IDLER_SWITCH, "48 8d 5f 78 48 3b de", "the application switches its idler", idler_switch_stub as *const () as usize, &SWITCH_RESUME),
        Point::keep(IDLER_SWITCH_GOES_ON, "74 19 48 8b 06", "the idler switch goes on"),
        Point::observe(UNIT_DESTROYED, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57", "a unit's destructor", 1, Some(unit_destroyed), None),
        // mov rbx, [rcx + 0x1a8]; ...; mov rcx, [rbx + 0x70]; test rcx, rcx; je
        Point::keep(0xd3b147, "48 8b 99 a8 01 00 00 45 33 ed 48 85 db 74 35 48 8b 4b 70 48 85 c9 74 0d", "a unit's place's model let go with the unit")
            .uses(&[UNIT_PLACE, PLACE_MODEL]),
    ])
    .chain(CROSSINGS_AT.iter().map(|&(rva, bytes, what, argc)| Point::observe(rva, bytes, what, argc, Some(crossed), None)))
    .collect()
}

/// The crossings the day reaches, at their entries (docs/research/drawing-beside-the-day.md,
/// "Where the day reaches the interface and the graphics", "The crossings a
/// late day reaches"): the RVA, its first bytes, what it is, the arguments it
/// reads.
const CROSSINGS_AT: [(usize, &str, &str, u32); 12] = [
    (0x81e280, "40 55 53 56 57 41 54", "the interface's monthly refresh", 1),
    (0x8c1430, "4c 89 44 24 18", "a message's popup", 3),
    (0x10a03f0, "48 89 5c 24 18", "a window made", 5),
    (0x821ff0, "48 89 5c 24 08", "the map mode set", 4),
    (0x1347a50, "48 89 5c 24 10", "the map's markers of a province", 3),
    (0x13238b0, "48 8b c4 4c 89 40 18", "a port's model", 4),
    (0xded0f0, "48 89 5c 24 18", "the new age's window", 1),
    (0xde6300, "48 89 5c 24 10", "a combat's result drawn into a texture", 2),
    (0xe428b0, "48 89 5c 24 08", "a country's window to the player", 4),
    (0x10b6c50, "48 8b c4 48 89 58 10", "the minimap", 2),
    (0x123f800, "48 89 5c 24 10", "an item of the message list", 4),
    (0x10af990, "4c 8b c9 83 fa ff", "the map told of a province's new owner", 2),
];

fn crossed(call: &mut Call) {
    crossing(call.observed().unwrap_or(0));
}

/// A unit's place on the map, kept beside the unit at its `+0x1a8`, and the
/// model the map's markers made for it (`+0x70`), which the drawing draws.
const UNIT_PLACE: usize = 0x1a8;
const PLACE_MODEL: usize = 0x70;
/// A unit's place worked out again from the unit (`0x1448880`): with its
/// second argument set, it first lets the place's model go. The unit's
/// constructor calls it so for a place just made, with no model yet; the
/// map's markers, for a unit moved at once. Only the model let go is a
/// crossing: the rest is the place's own, which the drawing does not read.
const PLACE_RESET: usize = 0x1448880;
/// A unit's destructor (`0xd3b130`), which lets its place's model go
/// before anything else.
const UNIT_DESTROYED: usize = 0xd3b130;

/// A text floating on the map (`AnimatedMapText`, eu3idler.cpp): the in-game
/// idler, a province, the text's type and its text, and a number; it makes
/// the text and puts it on the province's list of the map's, which the
/// drawing draws, and answers nothing its callers read. The two strings are
/// passed by value: it destroys them as it returns (`0x6e9f68`).
const MAP_TEXT: usize = 0x6e9860;
type MapText = unsafe extern "C" fn(usize, u32, usize, usize, usize, usize) -> usize;
static ORIGINAL_MAP_TEXT: Function<MapText> = Function::new();
/// The interface's daily refresh, its method `+0x2a8`, which the day calls on
/// the world's interface (`world+0x1e00`) once a day: the windows' values and
/// the minimap. It answers nothing the day reads.
const DAILY_REFRESH: usize = 0x8339f0;
type DailyRefresh = unsafe extern "C" fn(usize) -> usize;
static ORIGINAL_DAILY_REFRESH: Function<DailyRefresh> = Function::new();
const WORLD_INTERFACE: usize = 0x1e00;

/// What the day asked of the interface and waits for nothing of: done at the
/// next application frame, before its update (docs/plans/drawing-thread.md,
/// C2). Until then the drawing thread draws on, as nothing changed.
enum Deferred {
    MapText { idler: usize, province: u32, kind: Vec<u8>, text: Vec<u8>, number: usize, last: usize },
    DailyRefresh { interface: usize },
}

static DEFERRED: Mutex<Vec<Deferred>> = Mutex::new(Vec::new());
static DEFERRED_COUNT: AtomicU64 = AtomicU64::new(0);

/// Whether a call of the day's into the interface waits for the next frame.
fn deferring() -> bool {
    on_main() && drawing_on() && main_side(|s| s.in_day && !s.holds)
}

#[called_by_game]
unsafe extern "C" fn map_text(idler: usize, province: u32, kind: usize, text: usize, number: usize, last: usize) -> usize {
    if deferring() {
        let m = crate::hooked_memory();
        if let (Ok(kind_text), Ok(text_text)) = (game_string::read(&m, kind), game_string::read(&m, text)) {
            // The strings are the call's to destroy.
            let destroy: unsafe extern "C" fn(usize) = unsafe { std::mem::transmute(m.at(game_string::DESTROY_AT.rva())) };
            unsafe {
                destroy(kind);
                destroy(text);
            }
            DEFERRED.lock().unwrap_or_else(|e| e.into_inner()).push(Deferred::MapText { idler, province, kind: kind_text, text: text_text, number, last });
            DEFERRED_COUNT.fetch_add(1, Ordering::Relaxed);
            return 0;
        }
    }
    unsafe { ORIGINAL_MAP_TEXT.get()(idler, province, kind, text, number, last) }
}

#[called_by_game]
unsafe extern "C" fn daily_refresh(interface: usize) -> usize {
    if deferring() {
        DEFERRED.lock().unwrap_or_else(|e| e.into_inner()).push(Deferred::DailyRefresh { interface });
        DEFERRED_COUNT.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    unsafe { ORIGINAL_DAILY_REFRESH.get()(interface) }
}

/// The calls deferred, done as the game would have done them, by the main
/// thread holding the lock; those whose idler or interface has gone since
/// are left out.
fn do_deferred(m: &Memory) {
    let deferred = std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|e| e.into_inner()));
    if deferred.is_empty() {
        return;
    }
    let idler = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
    let interface = m.ptr(m.at(WORLD)).and_then(|world| m.ptr(world + WORLD_INTERFACE)).unwrap_or(0);
    for d in deferred {
        match d {
            Deferred::MapText { idler: of, province, kind, text, number, last } if of == idler => unsafe {
                // Made as the game makes them, for the call to destroy.
                let (mut kind_s, mut text_s) = (game_string::empty(), game_string::empty());
                let (kind_at, text_at) = (kind_s.as_mut_ptr() as usize, text_s.as_mut_ptr() as usize);
                game_string::append(kind_at, &kind);
                game_string::append(text_at, &text);
                ORIGINAL_MAP_TEXT.get()(of, province, kind_at, text_at, number, last);
            },
            Deferred::DailyRefresh { interface: of } if of == interface => unsafe {
                ORIGINAL_DAILY_REFRESH.get()(of);
            },
            _ => {}
        }
    }
}

/// The renderer's BeginScene (`0x16d4e00`, its table's `[0x1fd1290]`): the
/// one renderer of the game's, which both threads draw through, forgets the
/// vertex declaration it keeps (`+0x108`) as a scene begins, and the drawing
/// reads it as it draws a mesh's part (`0x16d4fa0`). A scene the main thread
/// begins in a day (drawing into a texture) is a crossing, whatever led
/// there; the drawing thread's own scenes are its frames.
const SCENE_BEGINS: usize = 0x16d4e00;
const RENDERER_DECLARATION: usize = 0x108;

fn scene_begins(call: &mut Call) {
    if on_main() {
        crossing(call.observed().unwrap_or(0));
    } else if drawing_on() && !is_drawing_thread() && !OTHER_SCENE_LOGGED.swap(true, Ordering::Relaxed) {
        log!("drawing: thread {} began a scene of the renderer, which the drawing thread may be drawing with", crate::thread_id());
    }
}

static OTHER_SCENE_LOGGED: AtomicBool = AtomicBool::new(false);

/// A mesh object's destructor (`pdxmeshobject.cpp`, vtable `0x1d92408`): the
/// object and whether to free it. The drawing draws mesh objects; one let go
/// while it draws faults it (docs/research/drawing-beside-the-day.md, "The
/// day's crossings read again"), so one let go in the day is a crossing,
/// whatever led there.
const MESH_LET_GO: usize = 0x153e560;

fn mesh_let_go(call: &mut Call) {
    if !drawing_on() {
        return;
    }
    let held = if on_main() {
        crossing(call.observed().unwrap_or(0));
        main_side(|s| s.holds)
    } else {
        false
    };
    // Let go with the lock not held, the drawing thread may draw it: which
    // thread, and from where, the first few times.
    if !held && UNHELD_LOGGED.fetch_add(1, Ordering::Relaxed) < MOST_UNHELD_LOGGED {
        let mut frames = [std::ptr::null_mut(); 16];
        let n = unsafe { windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace(1, 16, frames.as_mut_ptr(), std::ptr::null_mut()) } as usize;
        let base = call.memory().at(0usize);
        let stack: Vec<String> = frames[..n]
            .iter()
            .map(|&f| f as usize)
            .map(|f| if f >= base && f < base + 0x3000000 { format!("{:#x}", f - base) } else { format!("!{f:#x}") })
            .collect();
        let thread = if is_drawing_thread() {
            "the drawing thread"
        } else if on_main() {
            "the main thread"
        } else {
            "another thread"
        };
        log!("drawing: {thread} let a mesh object go without the lock: {}", stack.join(" < "));
    }
}

/// How many mesh objects let go without the lock were logged, and the most.
static UNHELD_LOGGED: AtomicU32 = AtomicU32::new(0);
const MOST_UNHELD_LOGGED: u32 = 20;

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

/// The idler is about to be switched, by the main thread holding the lock. A
/// frame of the drawing thread's may be in Present, without the lock, and is
/// to end with the idler it began with: the main thread lets the lock go
/// until it has.
#[called_by_game]
extern "C" fn idler_switches() {
    PREPARED.store(0, Ordering::Release);
    if !IN_FRAME.load(Ordering::Acquire) {
        return;
    }
    main_side(main_give);
    while IN_FRAME.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    main_side(|s| main_take(s, AT_FRAME));
}

fn place_reset(call: &mut Call) {
    let m = call.memory();
    if call.arg(1) as u8 != 0 && m.ptr(call.arg(0) + PLACE_MODEL).is_ok_and(|model| model != 0) {
        crossing(call.observed().unwrap_or(0));
    }
}

fn unit_destroyed(call: &mut Call) {
    let m = call.memory();
    if m.ptr(call.arg(0) + UNIT_PLACE).and_then(|place| if place == 0 { Ok(0) } else { m.ptr(place + PLACE_MODEL) }).is_ok_and(|model| model != 0) {
        crossing(call.observed().unwrap_or(0));
    }
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
/// A crossing changed the interface's state since the last frame prepared:
/// what it began (a unit's model let go, still on the map's lists) the next
/// update finishes, and the game never draws between the two, so neither
/// does the drawing thread. A crossing in an update comes before the frame
/// is prepared, which clears it; one in a day or at the safe frame after the
/// update, after it.
static CHANGED: AtomicBool = AtomicBool::new(false);
/// How many frames the main thread prepared, and the last of them drawn. The
/// game's frames are paced by Present, which waits for the display; the main
/// thread no longer presents, so a frame that runs no day (paused, or between
/// the days of a slow speed) waits for the last one to be drawn instead, as
/// it waited for the display, and the main thread does not spin.
static PREPARED_SEQ: AtomicU64 = AtomicU64::new(0);
static DRAWN_SEQ: AtomicU64 = AtomicU64::new(0);
/// A day ran since the last application frame began.
static DAY_RAN: AtomicBool = AtomicBool::new(false);
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
static CROSSINGS: AtomicU64 = AtomicU64::new(0);
/// For the log, by crossing: the function crossed into, how many times, and
/// how long the frames it held back were held back for, in microseconds (from
/// the first crossing after an update to the next update).
static BY_CROSSING: Mutex<Vec<(usize, u64, u64)>> = Mutex::new(Vec::new());
/// The crossing that held the frames back since the last update, and when.
static HELD_BACK_BY: Mutex<Option<(usize, Instant)>> = Mutex::new(None);
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

fn main_take(s: &mut MainSide, at: usize) {
    if !s.holds {
        let waited = LOCK.take();
        MAIN_WAITED.fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        WAITED_AT[at].fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
        s.holds = true;
    }
}

fn main_give(s: &mut MainSide) {
    if s.holds {
        s.holds = false;
        LOCK.give();
    }
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
    if !DAY_RAN.swap(false, Ordering::AcqRel) {
        pace();
    }
    main_side(|s| {
        let took = !s.holds;
        main_take(s, AT_FRAME);
        s.frames.push(took);
    });
    do_deferred(&crate::hooked_memory());
}

fn app_frame_after(call: &mut Call) {
    if !on_main() {
        return;
    }
    let m = call.memory();
    let app = call.arg(0);
    let quits = [APP_QUITS, APP_PUMP_QUITS].iter().any(|&at| m.u8(app + at).is_ok_and(|b| b != 0));
    main_side(|s| {
        if s.frames.pop() == Some(true) {
            s.crossing = false;
            main_give(s);
        }
    });
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
    main_side(|s| main_take(s, AT_FRAME));
    if IN_FRAME.load(Ordering::Acquire) {
        main_side(main_give);
        while IN_FRAME.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        main_side(|s| main_take(s, AT_FRAME));
    }
    log!("drawing: the game quits; the drawing thread draws no more");
}

/// Waits for the drawing thread to draw the last frame prepared, unless it
/// cannot (a crossing changed what it draws) or the main thread holds the
/// lock (a frame inside another).
fn pace() {
    let wanted = PREPARED_SEQ.load(Ordering::Acquire);
    if DRAWN_SEQ.load(Ordering::Acquire) >= wanted || !drawing_on() || CHANGED.load(Ordering::Acquire) || main_side(|s| s.holds || s.in_day) {
        return;
    }
    let began = Instant::now();
    while DRAWN_SEQ.load(Ordering::Acquire) < wanted && began.elapsed() < PACE_AT_MOST && drawing_on() {
        std::thread::sleep(Duration::from_micros(250));
    }
    PACED.fetch_add(began.elapsed().as_micros() as u64, Ordering::Relaxed);
}

/// The daily update begins (`game::events`): it lets the lock go.
pub fn day_begins() {
    if !on_main() || !drawing_on() {
        return;
    }
    DAY_RAN.store(true, Ordering::Release);
    main_side(|s| {
        s.in_day = true;
        s.day_gave = s.holds;
        main_give(s);
    });
}

/// The daily update has returned (`game::events`): the frame it ran in has
/// the lock again, and does what the day deferred. Done at the next frame's
/// beginning instead, the interface's daily refresh, a crossing (the
/// minimap), held the drawing back through the whole of the next day, for
/// that frame's preparation came after it: at speed 5 the drawing stood
/// still for most of the time. Here only the rest of this frame's update
/// comes before the preparation.
pub fn day_ends() {
    if !on_main() {
        return;
    }
    let holds = main_side(|s| {
        if !s.in_day {
            return false;
        }
        s.in_day = false;
        s.crossing = false;
        if s.day_gave {
            main_take(s, AT_DAY_END);
            s.day_gave = false;
        }
        s.holds
    });
    if holds {
        do_deferred(&crate::hooked_memory());
    }
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

/// The day reaches the interface or the graphics, in the function at
/// `into`: the lock first, kept until the next check for a frame inside the
/// day.
pub fn crossing(into: usize) {
    if !on_main() {
        return;
    }
    main_side(|s| {
        // Wherever it comes: one in an update is followed by the frame's
        // drawing, which forgets it; one after the frame was prepared (in a
        // day, or at the safe frame after the update) changes what the
        // prepared frame draws, so the drawing thread waits for the next
        // update.
        if !CHANGED.swap(true, Ordering::AcqRel) {
            *HELD_BACK_BY.lock().unwrap_or_else(|e| e.into_inner()) = Some((into, Instant::now()));
        }
        count_crossing(into, 1, 0);
        if s.in_day && !s.holds {
            CROSSINGS.fetch_add(1, Ordering::Relaxed);
            main_take(s, AT_CROSSING);
            s.crossing = true;
        }
    });
}

fn count_crossing(into: usize, times: u64, held: u64) {
    let mut by = BY_CROSSING.lock().unwrap_or_else(|e| e.into_inner());
    match by.iter_mut().find(|c| c.0 == into) {
        Some(c) => {
            c.1 += times;
            c.2 += held;
        }
        None => by.push((into, times, held)),
    }
}

/// The crossings since the last log, the three that held the frames back
/// longest first: the RVA crossed into, the times, the milliseconds held back.
fn crossings_logged(base: usize) -> String {
    let mut by = std::mem::take(&mut *BY_CROSSING.lock().unwrap_or_else(|e| e.into_inner()));
    by.sort_by_key(|c| std::cmp::Reverse(c.2));
    by.iter()
        .take(3)
        .map(|&(into, times, held)| format!("{:#x} {times}x {:.0} ms", into.wrapping_sub(base), held as f64 / 1000.0))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The in-game frame calls its drawing, on the main thread, which holds the
/// lock.
#[called_by_game]
extern "C" fn frame_draws(idler: usize) {
    let m = crate::hooked_memory();
    // A frame the main thread draws itself leaves the last one prepared
    // behind: its scenes are crossings, which hold it back.
    if !drawing_on() || must_draw_here(&m) {
        draw_here(&m, idler);
        return;
    }
    if let Err(e) = first_steps(&m, idler) {
        log!("drawing: the drawing's first steps could not read the game: {e}");
        return;
    }
    // This frame's update, and its first steps, which may draw the map
    // mode's colors in a scene of their own, have finished whatever the
    // crossings before began.
    CHANGED.store(false, Ordering::Release);
    if let Some((into, at)) = HELD_BACK_BY.lock().unwrap_or_else(|e| e.into_inner()).take() {
        count_crossing(into, 0, at.elapsed().as_micros() as u64);
    }
    PREPARED.store(idler, Ordering::Release);
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
        let (map, camera) = (m.ptr(idler + IDLER_MAP)?, m.ptr(idler + IDLER_CAMERA)?);
        unsafe { direct3(m, AMBIENT)(idler + IDLER_SOUND, map, camera) };
    }
    if m.u8(idler + IDLER_COLORS_DUE)? != 0 || m.u8(idler + IDLER_COLORS_ASKED)? != 0 {
        let mut now = 0i64;
        let at = unsafe { direct1(m, CLOCK)(&mut now as *mut i64 as usize) };
        let now = if at != 0 { m.u64(at)? as i64 } else { now };
        if now >= m.u64(idler + IDLER_COLORS_AT)? as i64 {
            let map = m.ptr(m.ptr(m.at(APP))? + APP_MAP)?;
            let (mode, arg) = (m.u32(idler + IDLER_MAP_MODE)? as usize, m.u32(idler + IDLER_MAP_MODE_ARG)? as usize);
            unsafe { direct3(m, COLORS)(map, mode, arg) };
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

/// The drawing thread is in Direct3D's Present, which may wait for the
/// display: the main thread does not draw a frame of its own meanwhile.
static PRESENTING: AtomicBool = AtomicBool::new(false);

/// The drawing thread's frame goes into Direct3D's Present (the framework's
/// Present observer, `framework_ui::hook`): it lets the lock go for it, as
/// the frame is drawn and Present touches only the device, which serves
/// several threads.
pub fn present_begins() {
    PRESENTING.store(true, Ordering::Release);
    LOCK.give();
}

/// Present has returned: the drawing thread takes the lock again for what
/// the drawing does after it (the idler's `+0x1608`, its clock).
pub fn present_ends() {
    PRESENTING.store(false, Ordering::Release);
    let waited = LOCK.take();
    DRAWING_WAITED.fetch_add(waited.as_micros() as u64, Ordering::Relaxed);
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
        let idler = PREPARED.load(Ordering::Acquire);
        let current = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
        let seq = PREPARED_SEQ.load(Ordering::Acquire);
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
        if idler != 0 && current == idler && !CHANGED.load(Ordering::Acquire) && !must_draw_here(&m) {
            DRAWING_LENGTH.store(length.to_bits(), Ordering::Relaxed);
            IN_FRAME.store(true, Ordering::Release);
            unsafe { direct1(&m, DRAWING)(idler) };
            IN_FRAME.store(false, Ordering::Release);
            DRAWN.fetch_add(1, Ordering::Relaxed);
            DRAWN_SEQ.store(seq, Ordering::Release);
        }
        LOCK.give();
        if logged.elapsed() >= Duration::from_secs(10) {
            let drawn = DRAWN.load(Ordering::Relaxed);
            log!(
                "drawing: {:.1} frames a second, {} drawn by the main thread; the main thread waited {:.0} ms for the lock ({:.0} at frames, {:.0} at crossings, {:.0} at days' ends) and {:.0} ms for frames drawn, the drawing thread {:.0} ms; {} crossing(s), {} deferred, the frames held back longest by {}",
                (drawn - last_drawn) as f64 / logged.elapsed().as_secs_f64(),
                HERE.swap(0, Ordering::Relaxed),
                MAIN_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                WAITED_AT[AT_FRAME].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                WAITED_AT[AT_CROSSING].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                WAITED_AT[AT_DAY_END].swap(0, Ordering::Relaxed) as f64 / 1000.0,
                PACED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                DRAWING_WAITED.swap(0, Ordering::Relaxed) as f64 / 1000.0,
                CROSSINGS.swap(0, Ordering::Relaxed),
                DEFERRED_COUNT.swap(0, Ordering::Relaxed),
                crossings_logged(m.at(0usize))
            );
            last_drawn = drawn;
            logged = Instant::now();
        }
    }
}
