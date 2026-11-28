//! The crossings (GLOSSARY): where the main thread changes what the drawing
//! thread reads and cannot wait for the frame's preparation (what can is
//! [`super::deferred`]). Each is caught where the game's code enters it, some
//! at the bottom whatever path led there (a scene of the renderer's begun, a
//! mesh object let go that is not deferred): in a day the main thread takes the lock there, and the
//! frame prepared is marked changed, so that the drawing thread draws nothing
//! until the next update finishes what the crossing began. Also the count,
//! for the log, of the crossings and of how long each held the frames back.
use super::{drawing_on, is_drawing_thread, main_side, on_main, MainLock};
use crate::base::called_by_game;
use crate::native::hooks::Call;
use crate::native::points::Point;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Tracks crossings that invalidate the prepared frame until the next update
/// and aggregates crossing counts and hold-back durations for the log.
struct CrossingsTracker {
    changed: AtomicBool,
    held_back_by: Mutex<Option<(usize, Instant)>>,
    taken: AtomicU64,
    by_crossing: Mutex<Vec<(usize, u64, u64)>>,
}

impl CrossingsTracker {
    const fn new() -> Self {
        Self { changed: AtomicBool::new(false), held_back_by: Mutex::new(None), taken: AtomicU64::new(0), by_crossing: Mutex::new(Vec::new()) }
    }

    fn record_crossing(&self, into: usize, now: Instant, took_lock: bool) {
        if !self.changed.swap(true, Ordering::AcqRel) {
            *self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()) = Some((into, now));
        }
        self.count(into, 1, 0);
        if took_lock {
            self.taken.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Runs a main-thread crossing through [`super::turns::MainSide`],
    /// promoting transient lock-order ownership and recording hold-back only
    /// while threaded drawing is active.
    fn record_main_crossing(
        &self,
        drawing_on: bool,
        on_main: bool,
        into: usize,
        now: Instant,
        main: &mut super::turns::MainSide,
        lock: &impl super::turns::Lock,
    ) -> bool {
        if !on_main || !drawing_on {
            return false;
        }
        let took = main.crossing(lock);
        self.record_crossing(into, now, took);
        true
    }

    /// Runs a main-thread renderer call: skips work when `main` already holds
    /// the presentation lock, and otherwise records a crossing.
    fn renderer_call(
        &self,
        drawing_on: bool,
        on_main: bool,
        into: usize,
        now: Instant,
        main: &mut super::turns::MainSide,
        lock: &impl super::turns::Lock,
    ) -> bool {
        if !on_main || !drawing_on || main.holds() {
            return false;
        }
        self.record_main_crossing(drawing_on, on_main, into, now, main, lock)
    }

    fn record_lock_order(&self, into: usize, took_lock: bool) {
        self.count(into, 1, 0);
        if took_lock {
            self.taken.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn holding_back(&self) -> Option<usize> {
        self.changed.load(Ordering::Acquire).then(|| self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()).map_or(0, |(into, _)| into))
    }

    fn frame_prepared(&self, now: Instant) {
        self.changed.store(false, Ordering::Release);
        if let Some((into, at)) = self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.count(into, 0, now.saturating_duration_since(at).as_micros() as u64);
        }
    }

    fn clear(&self) {
        self.changed.store(false, Ordering::Release);
        self.held_back_by.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    fn count(&self, into: usize, times: u64, held: u64) {
        let mut by = self.by_crossing.lock().unwrap_or_else(|e| e.into_inner());
        match by.iter_mut().find(|c| c.0 == into) {
            Some(c) => {
                c.1 += times;
                c.2 += held;
            }
            None => by.push((into, times, held)),
        }
    }

    fn logged(&self, base: usize) -> (u64, String) {
        let mut by = std::mem::take(&mut *self.by_crossing.lock().unwrap_or_else(|e| e.into_inner()));
        by.sort_by_key(|c| std::cmp::Reverse(c.2));
        let longest = by
            .iter()
            .take(3)
            .map(|&(into, times, held)| format!("{:#x} {times}x {:.0} ms", into.wrapping_sub(base), held as f64 / 1000.0))
            .collect::<Vec<_>>()
            .join(", ");
        (self.taken.swap(0, Ordering::Relaxed), longest)
    }
}

/// Lock-free fixed-capacity set of game locks taken by the drawing thread,
/// consulted without taking a Rust mutex inside `EnterCriticalSection` or
/// `AcquireSRWLockExclusive`.
struct LockOrder<const N: usize> {
    locks: [AtomicUsize; N],
    known: AtomicUsize,
    overflow_logged: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LockAction {
    None,
    WarnOverflow,
    TakePresentationFirst,
}

impl<const N: usize> LockOrder<N> {
    const fn new() -> Self {
        Self { locks: [const { AtomicUsize::new(0) }; N], known: AtomicUsize::new(0), overflow_logged: AtomicBool::new(false) }
    }

    fn clear(&self) {
        self.known.store(0, Ordering::Release);
        self.overflow_logged.store(false, Ordering::Relaxed);
    }

    fn observe(&self, lock: usize, on_drawing: bool, on_main_unheld: bool) -> LockAction {
        if !on_drawing && !on_main_unheld {
            return LockAction::None;
        }
        let count = self.known.load(Ordering::Acquire).min(N);
        let known = self.locks[..count].iter().any(|l| l.load(Ordering::Relaxed) == lock);
        if on_drawing {
            if !known && count < N {
                self.locks[count].store(lock, Ordering::Relaxed);
                self.known.store(count + 1, Ordering::Release);
            } else if !known && !self.overflow_logged.swap(true, Ordering::Relaxed) {
                return LockAction::WarnOverflow;
            }
            LockAction::None
        } else if known {
            LockAction::TakePresentationFirst
        } else {
            LockAction::None
        }
    }
}

/// The MSVC CRT `_Lockit(0)` locale/facet critical section (`0x233cdd0`),
/// entered exclusively via `0x1a5f504` (`0x1a0c750`, `_Lockit` ctor) and left
/// via `0x1a5f520` (`0x1a0c7c8`, `_Lockit` dtor). All 175 callers in `eu4.exe`
/// touch only CRT locale/facet internals and heap allocation — none ever enters
/// EU4 graphics, the renderer table, or any crossing. Because it is a strict
/// leaf lock that can never nest a presentation-lock acquisition, it is exempt
/// from `LOCK_ORDER` (docs/game/interface/stream-locks.md).
const STREAM_LOCALE_LOCK: usize = 0x233cdd0;

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
        Point::keep(0x1a5f504, "48 83 ec 28 e8 43 db 00 00 48 8d 0d bc d8 8d 00 48 83 c4 28 48 ff 25 31 72 10 00", "lock-zero EnterCriticalSection")
            .uses(&[STREAM_LOCALE_LOCK]),
        Point::keep(0x1a5f520, "48 8d 0d a9 d8 8d 00 48 ff 25 2a 72 10 00", "lock-zero LeaveCriticalSection").uses(&[STREAM_LOCALE_LOCK]),
    ]
    .into_iter()
    .chain(CROSSINGS_AT.iter().map(|&(rva, bytes, what, argc)| Point::observe(rva, bytes, what, argc, Some(crossed), None)))
    .chain(RENDERER_TABLE.iter().enumerate().filter(|&(_, &function)| function != 0).map(|(i, &function)| {
        let method = Point::method(RENDERER_TABLE_AT, 8 * i, function, "a function of the graphics back end's table");
        method.redirected(renderer_entries as *const () as usize + RENDERER_ENTRY_LEN * i, &RENDERER_ORIGINALS[i])
    }))
    .chain([
        Point::import(ENTER_SECTION, "KERNEL32.dll", "EnterCriticalSection", "the game enters a critical section")
            .redirected(section_entered as *const () as usize, &ORIGINAL_ENTER_SECTION),
        Point::import(ACQUIRE_SRW, "KERNEL32.dll", "AcquireSRWLockExclusive", "the game takes a slim lock")
            .redirected(srw_taken as *const () as usize, &ORIGINAL_ACQUIRE_SRW),
    ])
    .collect()
}

/// The crossings the day reaches, at their entries (docs/game/interface/frame-lifecycle.md,
/// "Where the day reaches the interface and the graphics", "The crossings a
/// late day reaches", "An event's window, met half made"): the RVA, its first
/// bytes, what it is, the arguments it reads.
const CROSSINGS_AT: [(usize, &str, &str, u32); 8] = [
    (0x8c1430, "4c 89 44 24 18", "a message's popup", 3),
    (0x10a03f0, "48 89 5c 24 18", "a window made", 5),
    (0x1347a50, "48 89 5c 24 10", "the map's markers of a province", 3),
    (0xded0f0, "48 89 5c 24 18", "the new age's window", 1),
    (0xde6300, "48 89 5c 24 10", "a combat's result drawn into a texture", 2),
    (0xe428b0, "48 89 5c 24 08", "a country's window to the player", 4),
    (0x123f800, "48 89 5c 24 10", "an item of the message list", 4),
    (0x828c00, "48 89 5c 24 08", "an event's window shown to the player", 4),
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
/// there; the drawing thread's own scenes are its frames. One the main
/// thread begins with the lock held is not ([`renderer_called`]).
pub(crate) const SCENE_BEGINS: usize = 0x16d4e00;
const RENDERER_DECLARATION: usize = 0x108;

fn scene_begins(call: &mut Call) {
    if on_main() {
        renderer_crossing(call.observed().unwrap_or(0));
    } else if drawing_on() && !is_drawing_thread() && !OTHER_SCENE_LOGGED.swap(true, Ordering::Relaxed) {
        log!("drawing: thread {} began a scene of the renderer, which the drawing thread may be drawing with", crate::base::thread_id());
    }
}

/// The graphics back end's table of functions (`0x1fd10f8`, 81 of them;
/// docs/game/program/architecture.md, "The graphics back end"): the one way the
/// game reaches Direct3D, through the one renderer both threads draw with. No
/// code outside the back end calls Direct3D or D3DX, and the functions it
/// holds are called directly only where they do nothing (those that answer
/// a constant) (docs/game/interface/frame-lifecycle.md,
/// "The graphics back end, the one way in"). So the main thread in a day
/// reaching any of them is a crossing, whatever path led there: a text drawn
/// into a texture of its own read and set the render target before its
/// scene began, and the drawing thread drew part of its frame into the text
/// ("A text drawn into its texture in a day"). Each slot is sent to an entry
/// of [`renderer_entries`], which tells [`renderer_called`] and goes on to
/// the slot's function.
const RENDERER_TABLE_AT: usize = 0x1fd10f8;
/// The table's functions by slot; 0 for the five slots whose functions read
/// and write nothing, left as they are: they answer a constant (true, false,
/// 0, nothing) or the address of their argument's `+0x20`, shared with code
/// elsewhere and named there, and are called far more than the others (the
/// main thread called the one that answers 0 some 250,000 times a second).
const RENDERER_TABLE: [usize; 81] = [
    0x16d75c0, 0x16d8b50, 0x16d7250, 0x16d73d0, 0x16d5d90, 0x16d5d30, 0x16d5c30, 0, 0x16d5220, 0x16d5a20, 0x16d51f0, 0x16d7180, 0x16da460, 0x16d6980,
    0x16d4f00, 0x16d4e70, 0x16d6990, 0x16d6fc0, 0x16d65d0, 0x16d6640, 0x16d6890, 0x16d5e00, 0x16d5f20, 0x16d79b0, 0x16d76d0, 0x16d7760, 0x16d7930, 0x16d22e0,
    0x16d73e0, 0x16d70d0, 0x16d71b0, 0x16d8560, 0x16d50e0, 0, 0x16d4e90, 0x16d5f10, 0x16d7110, 0, 0x16dab80, 0x16d8e60, 0x16d8e50, 0x16d8990, 0, 0x16d8080, 0,
    0x16d7dd0, 0x16d4c50, 0x16d8eb0, 0x16d7030, 0x16d4da0, 0x16d8820, 0x16d4e00, 0x16d4e50, 0x16d8ea0, 0x16d8d30, 0, 0x16d8d70, 0, 0, 0x16d4c10, 0x16d8ed0,
    0x16d8e20, 0x16d96f0, 0x16d4b20, 0x16d4a40, 0x16d4db0, 0x16d4bb0, 0x16d8d20, 0x16d66f0, 0x16d7de0, 0x16d7f30, 0x16d80f0, 0x16d4fa0, 0x16d9750, 0x16d7770,
    0x16d79c0, 0x16d88c0, 0x16d8ca0, 0x16d4c80, 0x16d82b0, 0x16d7e70,
];
static RENDERER_ORIGINALS: [AtomicUsize; 81] = [const { AtomicUsize::new(0) }; 81];
/// The bytes of each entry of [`renderer_entries`].
const RENDERER_ENTRY_LEN: usize = 16;

/// The `slot`th function of the table called by the main thread: a crossing
/// unless the main thread holds the lock. What the renderer is shared for is
/// its state between a frame's calls (the render target, the vertex
/// declaration): with the lock held, the drawing thread is in no frame, and
/// a text drawn into its texture, a texture made or let go, is whole before
/// the lock goes, its owner holding the new one. So it is no change the next
/// update has to finish, and the frame prepared is not held back for it. The
/// in-game frame, after its drawing, draws the war overview's buttons into
/// their textures again at every frame while the player is at war
/// (docs/game/interface/frame-lifecycle.md, "The war overview after the
/// drawing"): held back each time, the frame was never drawn.
#[called_by_game]
extern "C" fn renderer_called(slot: usize) {
    if !drawing_on() || super::main_holds() {
        return;
    }
    renderer_crossing(crate::native::hooked_memory().at(RENDERER_TABLE[slot]));
}

fn renderer_crossing(into: usize) {
    if !on_main() || !drawing_on() || super::main_holds() {
        return;
    }
    main_side(|s| {
        if TRACKER.renderer_call(true, true, into, Instant::now(), s, &MainLock) {
            crate::native::probes::frame_invalidated();
        }
    });
}

macro_rules! renderer_entries {
    ($($slot:literal)*) => {
        // mov eax, slot (5 bytes); jmp rel32 (5 bytes); int3 to 16 bytes.
        concat!($("mov eax, ", stringify!($slot), "\n", ".byte 0xe9\n", ".long 3f - . - 4\n", ".fill 6, 1, 0xcc\n",)* "3:\n")
    };
}

/// One entry for each slot of the table, [`RENDERER_ENTRY_LEN`] bytes apart:
/// the slot's number in `eax`, in which a call through a pointer passes
/// nothing, then what every entry does: on the main thread (the thread's id
/// at `gs:[0x48]`), tell [`renderer_called`], keeping every register an
/// argument may be in; then go on to the slot's function as if the game had
/// called it.
#[unsafe(naked)]
extern "C" fn renderer_entries() {
    std::arch::naked_asm!(
        renderer_entries!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39
            40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80),
        "mov r11d, dword ptr gs:[0x48]",
        "cmp r11d, dword ptr [rip + {main}]",
        "jne 4f",
        call_rust!("mov ecx, eax", call renderer_called),
        "4:",
        "lea r11, [rip + {originals}]",
        "jmp qword ptr [r11 + rax * 8]",
        main = sym crate::base::MAIN_THREAD,
        renderer_called = sym renderer_called,
        originals = sym RENDERER_ORIGINALS,
    );
}

/// The game's own locks, which it takes through its imports: a critical
/// section (`EnterCriticalSection`, `0x1b66750`) and a slim lock
/// (`AcquireSRWLockExclusive`, `0x1b66538`). The drawing thread takes some of
/// them while it draws, holding the presentation lock: the graphics' resources
/// (`0x16c40b0`, under `[0x242b818]`), as the map's drawing looks a texture
/// up (`0x13f968f`). The main thread takes the same lock and calls the
/// renderer's table inside it, a crossing: waiting there for the presentation
/// lock, it held what the drawing thread waited for, and the game hung
/// (docs/game/interface/frame-lifecycle.md, "The game's locks, in one
/// order"). So the locks are taken in one order, the presentation lock
/// first: the drawing thread learns each lock it takes, and the main thread,
/// in a day, takes the presentation lock before one of those. Only the lock:
/// taking one changes nothing the frame draws, which a crossing inside it
/// marks.
const ENTER_SECTION: usize = 0x1b66750;
const ACQUIRE_SRW: usize = 0x1b66538;
static ORIGINAL_ENTER_SECTION: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_ACQUIRE_SRW: AtomicUsize = AtomicUsize::new(0);
const LOCK_ORDER_CAPACITY: usize = 256;
static LOCK_ORDER: LockOrder<LOCK_ORDER_CAPACITY> = LockOrder::new();

/// Clears learned drawing-thread lock addresses once the drawing thread has
/// stopped drawing the previous idler, so transient heap locks from prior
/// sessions never exhaust [`LOCK_ORDER`].
pub(super) fn reset_lock_order() {
    LOCK_ORDER.clear();
}

#[called_by_game]
unsafe extern "system" fn section_entered(section: usize) {
    lock_taken(section, ENTER_SECTION);
    let enter: unsafe extern "system" fn(usize) = unsafe { std::mem::transmute(ORIGINAL_ENTER_SECTION.load(Ordering::Acquire)) };
    unsafe { enter(section) }
}

#[called_by_game]
unsafe extern "system" fn srw_taken(lock: usize) {
    lock_taken(lock, ACQUIRE_SRW);
    let acquire: unsafe extern "system" fn(usize) = unsafe { std::mem::transmute(ORIGINAL_ACQUIRE_SRW.load(Ordering::Acquire)) };
    unsafe { acquire(lock) }
}

fn lock_taken(lock: usize, by: usize) {
    if !drawing_on() || lock == crate::native::hooked_memory().at(STREAM_LOCALE_LOCK) {
        return;
    }
    let tid = crate::base::thread_id();
    let on_drawing = tid == super::DRAWING_THREAD.load(Ordering::Relaxed);
    let on_main_unheld = !on_drawing && tid == crate::base::main_thread() && !super::main_holds();
    if !on_drawing && !on_main_unheld {
        return;
    }
    match LOCK_ORDER.observe(lock, on_drawing, on_main_unheld) {
        LockAction::WarnOverflow => {
            log!("drawing: the drawing thread took more than {LOCK_ORDER_CAPACITY} of the game's locks; the rest are not kept in order");
        }
        LockAction::TakePresentationFirst => {
            main_side(|s| {
                let took = s.crossing(&MainLock);
                TRACKER.record_lock_order(crate::native::hooked_memory().at(by), took);
            });
        }
        LockAction::None => {}
    }
}

static OTHER_SCENE_LOGGED: AtomicBool = AtomicBool::new(false);

/// A mesh object let go where it cannot wait for the frame's preparation
/// (docs/game/interface/frame-lifecycle.md, "The day's crossings read
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
        let base = crate::native::hooked_memory().at(0usize);
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

static TRACKER: CrossingsTracker = CrossingsTracker::new();

/// The day reaches the interface or the graphics, in the function at
/// `into`: the lock first, kept until the next check for a frame inside the
/// day.
pub(super) fn crossing(into: usize) {
    if !on_main() || !drawing_on() {
        return;
    }
    main_side(|s| {
        if TRACKER.record_main_crossing(true, true, into, Instant::now(), s, &MainLock) {
            crate::native::probes::frame_invalidated();
        }
    });
}

/// The crossing that holds the frame prepared back, if one does (0 when it
/// is not known yet).
pub(super) fn holding_back() -> Option<usize> {
    TRACKER.holding_back()
}

/// A frame is prepared, by the main thread holding the lock: its update has
/// finished whatever the crossings before began.
pub(super) fn frame_prepared() {
    TRACKER.frame_prepared(Instant::now());
}

pub(super) fn clear() {
    TRACKER.clear();
}

/// The crossings since the last log: how many took the lock in a day, and
/// the three that held the frames back longest, the RVA crossed into, the
/// times, the milliseconds held back.
pub(super) fn logged(base: usize) -> (u64, String) {
    TRACKER.logged(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[derive(Default)]
    struct TestLock(Mutex<Vec<&'static str>>);

    impl super::super::turns::Lock for TestLock {
        fn take(&self, _: super::super::turns::At) -> Duration {
            self.0.lock().unwrap().push("take");
            Duration::ZERO
        }
        fn give(&self) {
            self.0.lock().unwrap().push("give");
        }
    }

    #[test]
    fn lock_order_learns_drawing_locks_warns_once_on_overflow_and_orders_unheld_main_thread() {
        let order: LockOrder<2> = LockOrder::new();
        assert_eq!(order.observe(0xaa, false, true), LockAction::None);
        assert_eq!(order.observe(0xaa, true, false), LockAction::None);
        assert_eq!(order.observe(0xbb, true, false), LockAction::None);
        assert_eq!(order.observe(0xcc, true, false), LockAction::WarnOverflow);
        assert_eq!(order.observe(0xdd, true, false), LockAction::None, "overflow warns only once");
        assert_eq!(order.observe(0xaa, false, true), LockAction::TakePresentationFirst);
        assert_eq!(order.observe(0xaa, false, false), LockAction::None, "main already holds presentation lock");
        assert_eq!(order.observe(0xcc, false, false), LockAction::None, "worker threads bypass lock-order scan");
        assert_eq!(order.observe(0xcc, false, true), LockAction::None, "unknown lock untouched");
    }

    #[test]
    fn renderer_call_skips_when_drawing_off_or_main_holds_and_crosses_when_unheld() {
        let lock = TestLock::default();
        let mut main = super::super::turns::MainSide::new();
        let tracker = CrossingsTracker::new();
        let now = Instant::now();

        main.day_begins(&lock);
        assert!(!tracker.renderer_call(false, true, 0x1000, now, &mut main, &lock));
        assert!(!main.holds());
        assert_eq!(tracker.holding_back(), None);

        assert!(tracker.renderer_call(true, true, 0x2000, now, &mut main, &lock));
        assert!(main.holds());
        assert_eq!(tracker.holding_back(), Some(0x2000));

        assert!(!tracker.renderer_call(true, true, 0x3000, now, &mut main, &lock));

        main.frame_check(&lock);
        assert!(!main.holds());
    }

    #[test]
    fn tracker_records_hold_back_duration_and_clears_on_prepare() {
        let tracker = CrossingsTracker::new();
        let start = Instant::now();
        tracker.record_crossing(0x1400_1234, start, true);
        assert_eq!(tracker.holding_back(), Some(0x1400_1234));
        tracker.frame_prepared(start + Duration::from_millis(12));
        assert_eq!(tracker.holding_back(), None);
        let (taken, summary) = tracker.logged(0x1400_0000);
        assert_eq!(taken, 1);
        assert!(summary.contains("0x1234 1x 12 ms"), "{summary}");
    }
}
