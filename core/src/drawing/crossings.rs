//! The crossings (GLOSSARY): where the main thread changes what the drawing
//! thread reads and cannot wait for the frame's preparation (what can is
//! [`super::deferred`]). Each is caught where the game's code enters it, some
//! at the bottom whatever path led there (a scene of the renderer's begun, a
//! mesh object let go that is not deferred): in a day the main thread takes the lock there, and the
//! frame prepared is marked changed, so that the drawing thread draws nothing
//! until the next update finishes what the crossing began. Also the count,
//! for the log, of the crossings and of how long each held the frames back.
use super::{drawing_on, is_drawing_thread, main_side, on_main, MainLock};
use crate::hooks::Call;
use crate::points::Point;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub(super) fn points() -> Vec<Point> {
    [
        Point::observe(PLACE_RESET, "48 8b c4 48 89 58 10 48", "a unit's place worked out again", 2, Some(place_reset), None),
        // test dl, dl; je; call; mov rcx, [rbx + 0x70]; test rcx, rcx; je;
        // mov rax, [rcx]; lea edx, [rdi + 1]; call [rax]; mov [rbx + 0x70], rdi
        Point::keep(0x14488c3, "84 d2 74 24 e8 64 f3 ff ff 48 8b 4b 70 48 85 c9 74 0c 48 8b 01 8d 57 01 ff 10 48 89 7b 70", "a unit's place's model let go")
            .uses(&[PLACE_MODEL]),
        Point::observe(SCENE_BEGINS, "48 89 5c 24 08 57 48 83 ec 20 48 8b f9", "the renderer begins a scene", 1, Some(scene_begins), None),
        // mov qword ptr [rdi + 0x108], 0: the vertex declaration it keeps
        // forgotten.
        Point::keep(0x16d4e2f, "48 c7 87 08 01 00 00 00 00 00 00", "a scene forgets the renderer's vertex declaration").uses(&[RENDERER_DECLARATION]),
        Point::observe(UNIT_DESTROYED, "48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57", "a unit's destructor", 1, Some(unit_destroyed), None),
        // mov rbx, [rcx + 0x1a8]; ...; mov rcx, [rbx + 0x70]; test rcx, rcx; je
        Point::keep(0xd3b147, "48 8b 99 a8 01 00 00 45 33 ed 48 85 db 74 35 48 8b 4b 70 48 85 c9 74 0d", "a unit's place's model let go with the unit")
            .uses(&[UNIT_PLACE, PLACE_MODEL]),
    ]
    .into_iter()
    .chain(CROSSINGS_AT.iter().map(|&(rva, bytes, what, argc)| Point::observe(rva, bytes, what, argc, Some(crossed), None)))
    .collect()
}

/// The crossings the day reaches, at their entries (docs/research/drawing-beside-the-day.md,
/// "Where the day reaches the interface and the graphics", "The crossings a
/// late day reaches"): the RVA, its first bytes, what it is, the arguments it
/// reads.
const CROSSINGS_AT: [(usize, &str, &str, u32); 8] = [
    (0x81e280, "40 55 53 56 57 41 54", "the interface's monthly refresh", 1),
    (0x8c1430, "4c 89 44 24 18", "a message's popup", 3),
    (0x10a03f0, "48 89 5c 24 18", "a window made", 5),
    (0x1347a50, "48 89 5c 24 10", "the map's markers of a province", 3),
    (0xded0f0, "48 89 5c 24 18", "the new age's window", 1),
    (0xde6300, "48 89 5c 24 10", "a combat's result drawn into a texture", 2),
    (0xe428b0, "48 89 5c 24 08", "a country's window to the player", 4),
    (0x123f800, "48 89 5c 24 10", "an item of the message list", 4),
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

/// A mesh object let go where it cannot wait for the frame's preparation
/// (docs/research/drawing-beside-the-day.md, "The day's crossings read
/// again"; [`super::deferred`] keeps the rest): one let go on another thread,
/// or destroyed without being freed. `into` is the destructor's address.
pub(super) fn mesh_let_go(into: usize) {
    if !drawing_on() {
        return;
    }
    let held = if on_main() {
        crossing(into);
        main_side(|s| s.holds())
    } else {
        false
    };
    // Let go with the lock not held, the drawing thread may draw it: which
    // thread, and from where, the first few times.
    if !held && UNHELD_LOGGED.fetch_add(1, Ordering::Relaxed) < MOST_UNHELD_LOGGED {
        let mut frames = [std::ptr::null_mut(); 16];
        let n = unsafe { windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace(1, 16, frames.as_mut_ptr(), std::ptr::null_mut()) } as usize;
        let base = crate::hooked_memory().at(0usize);
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

/// A crossing changed what the frame prepared draws since it was prepared:
/// what it began (a unit's model let go, still on the map's lists) the next
/// update finishes, and the game never draws between the two, so neither
/// does the drawing thread. A crossing in an update comes before the frame
/// is prepared, which forgets it; one in a day or at the safe frame after the
/// update, after it.
static CHANGED: AtomicBool = AtomicBool::new(false);
/// The crossing that marked the frame changed, and when.
static HELD_BACK_BY: Mutex<Option<(usize, Instant)>> = Mutex::new(None);
/// For the log: the crossings that took the lock in a day, and by crossing,
/// the function crossed into, how many times, and how long the frames it
/// held back were held back for, in microseconds (from the first crossing
/// after an update to the next update).
static TAKEN: AtomicU64 = AtomicU64::new(0);
static BY_CROSSING: Mutex<Vec<(usize, u64, u64)>> = Mutex::new(Vec::new());

/// The day reaches the interface or the graphics, in the function at
/// `into`: the lock first, kept until the next check for a frame inside the
/// day.
pub(super) fn crossing(into: usize) {
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
        count(into, 1, 0);
        if s.crossing(&MainLock) {
            TAKEN.fetch_add(1, Ordering::Relaxed);
        }
    });
}

/// The crossing that holds the frame prepared back, if one does (0 when it
/// is not known yet).
pub(super) fn holding_back() -> Option<usize> {
    CHANGED.load(Ordering::Acquire).then(|| HELD_BACK_BY.lock().unwrap_or_else(|e| e.into_inner()).map_or(0, |(into, _)| into))
}

/// A frame is prepared, by the main thread holding the lock: its update has
/// finished whatever the crossings before began.
pub(super) fn frame_prepared() {
    CHANGED.store(false, Ordering::Release);
    if let Some((into, at)) = HELD_BACK_BY.lock().unwrap_or_else(|e| e.into_inner()).take() {
        count(into, 0, at.elapsed().as_micros() as u64);
    }
}

fn count(into: usize, times: u64, held: u64) {
    let mut by = BY_CROSSING.lock().unwrap_or_else(|e| e.into_inner());
    match by.iter_mut().find(|c| c.0 == into) {
        Some(c) => {
            c.1 += times;
            c.2 += held;
        }
        None => by.push((into, times, held)),
    }
}

/// The crossings since the last log: how many took the lock in a day, and
/// the three that held the frames back longest, the RVA crossed into, the
/// times, the milliseconds held back.
pub(super) fn logged(base: usize) -> (u64, String) {
    let mut by = std::mem::take(&mut *BY_CROSSING.lock().unwrap_or_else(|e| e.into_inner()));
    by.sort_by_key(|c| std::cmp::Reverse(c.2));
    let longest = by
        .iter()
        .take(3)
        .map(|&(into, times, held)| format!("{:#x} {times}x {:.0} ms", into.wrapping_sub(base), held as f64 / 1000.0))
        .collect::<Vec<_>>()
        .join(", ");
    (TAKEN.swap(0, Ordering::Relaxed), longest)
}
