//! The deferred calls (docs/plans/drawing-thread.md, C2): what the main
//! thread asks of the interface and the graphics and waits for nothing of,
//! kept and done by it as the next frame is prepared ([`done`]), while it
//! holds the lock and the frame's update has finished: a text floating on the
//! map, the interface's daily refresh, the map told of a province's new
//! owner, the minimap drawn again, a mesh object let go; and, from the
//! day's and the commands' code, the map mode set and a port's model. Until then the drawing thread draws on, as
//! nothing changed. Done anywhere else, each would be a crossing, which holds
//! the frame prepared back to the next update: from the commands' step before
//! a day, through the whole day.
use super::{drawing_on, on_main, APP, APP_IDLER, APP_MAP};
use crate::called_by_game;
use crate::game_string;
use crate::memory::Memory;
use crate::objects::campaign::globals::WORLD;
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::replace(
            MESH_LET_GO,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18",
            "a mesh object's destructor",
            mesh_let_go as MeshLetGo,
            &ORIGINAL_MESH_LET_GO,
        ),
        Point::replace(MAP_TEXT, "48 89 5c 24 10 4c 89 4c 24 20", "a text floating on the map", map_text as MapText, &ORIGINAL_MAP_TEXT),
        // mov rcx, rsi; call string destructor: the text's type destroyed.
        Point::keep(0x6e9f68, "48 8b ce e8 f0 b6 9a ff 90 48 8b 8d a8 00 00 00", "a floating text's strings destroyed"),
        Point::replace(DAILY_REFRESH, "4c 8b dc 49 89 5b 18 55 56 57", "the interface's daily refresh", daily_refresh as DailyRefresh, &ORIGINAL_DAILY_REFRESH),
        Point::jump(MINIMAP, "48 8b c4 48 89 58 10", "the minimap drawn again", minimap_stub as *const () as usize, &MINIMAP_RESUME),
        Point::jump(MAP_MODE_SET, "48 89 5c 24 08 48 89 6c 24 10", "the map mode set", map_mode_stub as *const () as usize, &MAP_MODE_RESUME),
        Point::jump(PORT_MODEL, "48 8b c4 4c 89 40 18", "a port's model", port_model_stub as *const () as usize, &PORT_MODEL_RESUME),
        // call MAP_MODE_SET; call PORT_MODEL: the deferred callers, known by
        // where they return to.
        Point::keep(MAP_MODE_FROM_CAPITAL - 5, "e8 5c 7e e0 ff", "a moved capital's map mode set"),
        Point::keep(PORT_MODEL_FROM_TRADE - 5, "e8 9a eb ea 00", "a center of trade's port model"),
        Point::jump(OWNER_TOLD, "4c 8b c9 83 fa ff", "the map told of a province's new owner", owner_told_stub as *const () as usize, &OWNER_TOLD_RESUME),
        // mov rcx, [r13 + 0x1e00]; test rcx, rcx; je; mov rax, [rcx];
        // call [rax + 0x2a8]: the day's call of the interface's daily refresh.
        Point::keep(0x75c5e7, "49 8b 8d 00 1e 00 00 48 85 c9 74 09 48 8b 01 ff 90 a8 02 00 00", "the day's daily refresh of the interface")
            .uses(&[WORLD_INTERFACE, 0x2a8]),
    ]
}

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
/// The graphical map told that a province changed owner (`0xa00598`,
/// `0xa0e734`): its map entry's flag `+0x13f8` gets bit 3 (`|= 8`), or every
/// entry's for -1, which the map's drawing reads. It answers nothing. Its
/// callers keep `r10` and `r11` across the call (`0xa0e748`, `0xa0e755`),
/// which the function does not touch, so it is entered through a stub that
/// keeps every register, and goes on at its third instruction.
const OWNER_TOLD: usize = 0x10af990;
const OWNER_TOLD_GOES_ON: usize = 0x10af996;
crate::resume!(OWNER_TOLD_RESUME: OWNER_TOLD_GOES_ON_AT = OWNER_TOLD_GOES_ON);
/// The minimap drawn again (`0x10b6c50`: the minimap and a flag): its texture
/// made from `map/minimap_base.bmp` once and the countries' colors, which
/// the drawing draws. It answers nothing its callers read, and they call it
/// directly, so that it is entered through a stub that keeps every register,
/// and goes on at its third instruction. The in-game frame calls it before
/// its commands when the idler's `+0x33b` asks for it (`0x8143b5`–`0x8143eb`),
/// so that it held the frame back through the day that followed; the
/// interface's daily refresh calls it too (`0x833b7d`).
const MINIMAP: usize = 0x10b6c50;
/// The in-game idler's map mode set (`0x821ff0`, its slot `+0xc8`: the idler,
/// the map mode, two flags): the names made again, the colors asked for, the
/// windows that show the map mode told. Nearly a hundred places call it
/// directly, the interface's buttons among them, which read the map mode back;
/// a moved capital calls it from the provinces' code (`0xa1a18f`, which reads
/// nothing after it), in a day or in the commands' step, and only that call
/// is deferred, to the frame's preparation, known by where it returns to. It
/// is entered through a stub that keeps every register, and goes on at its
/// third instruction.
const MAP_MODE_SET: usize = crate::map::REFRESH.rva();
const MAP_MODE_GOES_ON: usize = 0x821ffa;
const MAP_MODE_FROM_CAPITAL: usize = 0xa1a194;
crate::resume!(MAP_MODE_RESUME: MAP_MODE_GOES_ON_AT = MAP_MODE_GOES_ON);
/// A port's model on the map (`0x13238b0`: the province's entry of the
/// graphical map's, the province, the graphical map, a flag), made, moved or
/// let go. The map's markers' update calls it (`0x1321880`); the center of
/// trade's update does (`0x474d11`, which reads nothing after it), in a day
/// or in the commands' step, and only that call is deferred, known by where
/// it returns to. Entered through a stub that keeps every register, going on
/// at its third instruction.
const PORT_MODEL: usize = 0x13238b0;
const PORT_MODEL_GOES_ON: usize = 0x13238b7;
const PORT_MODEL_FROM_TRADE: usize = 0x474d16;
crate::resume!(PORT_MODEL_RESUME: PORT_MODEL_GOES_ON_AT = PORT_MODEL_GOES_ON);
const MINIMAP_GOES_ON: usize = 0x10b6c57;
crate::resume!(MINIMAP_RESUME: MINIMAP_GOES_ON_AT = MINIMAP_GOES_ON);
/// A mesh object's scalar deleting destructor (`pdxmeshobject.cpp`, the slot
/// 0 of its vtable `0x1d92408`, called through it only): the object and
/// whether to free it (bit 0). It takes the object off its mesh's list of
/// objects (`[+8]+0x140`), lets its parts go (`+0x58`, `+0x90`, `+0xa0`) and
/// frees its 0xf0 bytes; it answers the object. The map's markers keep the
/// objects of the units and the ports they draw on lists of theirs, which the
/// next update takes them off; the drawing reads them there until then. Freed
/// as the frame is prepared instead, the object outlives every frame that
/// draws it (docs/research/drawing-beside-the-day.md, "Frames late at speed
/// 5").
const MESH_LET_GO: usize = 0x153e560;
type MeshLetGo = unsafe extern "C" fn(usize, u32) -> usize;
static ORIGINAL_MESH_LET_GO: Function<MeshLetGo> = Function::new();

/// What the day asked of the interface and waits for nothing of: done at the
/// next application frame, before its update (docs/plans/drawing-thread.md,
/// C2). Until then the drawing thread draws on, as nothing changed.
enum Deferred {
    MapText { idler: usize, province: u32, kind: Vec<u8>, text: Vec<u8>, number: usize, last: usize },
    DailyRefresh { interface: usize },
    OwnerTold { map: usize, province: i32 },
    Minimap { idler: usize, minimap: usize, flag: u32 },
    MapMode { idler: usize, mode: u32, first: u32, second: u32 },
    PortModel { entry: usize, province: usize, map: usize, flag: u32 },
    MeshLetGo { mesh: usize, free: u32 },
}

static DEFERRED: Mutex<Vec<Deferred>> = Mutex::new(Vec::new());
/// The calls deferred since the last log.
static COUNT: AtomicU64 = AtomicU64::new(0);

fn defer(call: Deferred) {
    DEFERRED.lock().unwrap_or_else(|e| e.into_inner()).push(call);
    COUNT.fetch_add(1, Ordering::Relaxed);
}

/// How many calls were deferred since the last log.
pub(super) fn logged() -> u64 {
    COUNT.swap(0, Ordering::Relaxed)
}

/// The deferred calls are being done: what they call is not deferred again.
static DOING: AtomicBool = AtomicBool::new(false);

/// Whether a call of the main thread's waits for the next frame's preparation.
fn deferring() -> bool {
    on_main() && drawing_on() && !DOING.load(Ordering::Relaxed)
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
            defer(Deferred::MapText { idler, province, kind: kind_text, text: text_text, number, last });
            return 0;
        }
    }
    unsafe { ORIGINAL_MAP_TEXT.get()(idler, province, kind, text, number, last) }
}

#[called_by_game]
unsafe extern "C" fn daily_refresh(interface: usize) -> usize {
    if deferring() {
        defer(Deferred::DailyRefresh { interface });
        return 0;
    }
    unsafe { ORIGINAL_DAILY_REFRESH.get()(interface) }
}

/// Whether the map's owner mark waits for the next frame.
#[called_by_game]
extern "C" fn owner_told(map: usize, province: i32) -> bool {
    if !deferring() {
        return false;
    }
    defer(Deferred::OwnerTold { map, province });
    true
}

#[unsafe(naked)]
extern "C" fn owner_told_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call owner_told, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "mov r9, rcx",
        "cmp edx, -1",
        "jmp qword ptr [rip + {goes_on}]",
        "2:",
        "ret",
        owner_told = sym owner_told,
        goes_on = sym OWNER_TOLD_GOES_ON_AT,
    );
}

/// Whether the minimap's drawing waits for the next frame; once for each
/// minimap and flag in a frame.
#[called_by_game]
extern "C" fn minimap(minimap: usize, flag: u32) -> bool {
    if !deferring() {
        super::crossings::crossing(crate::hooked_memory().at(MINIMAP));
        return false;
    }
    let m = crate::hooked_memory();
    let idler = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
    let flag = flag & 0xff;
    let due = Deferred::Minimap { idler, minimap, flag };
    let already = DEFERRED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|d| matches!(d, Deferred::Minimap { minimap: m, flag: f, .. } if *m == minimap && *f == flag));
    if !already {
        defer(due);
    }
    true
}

/// Whether a call that returns to `from` waits for the next frame's
/// preparation: from the one caller of the function's that is deferred.
fn deferring_from(from: usize, deferred_caller: usize) -> bool {
    deferring() && from == crate::hooked_memory().at(deferred_caller)
}

/// Whether the map mode's set waits for the next frame: from a moved capital.
#[called_by_game]
extern "C" fn map_mode_set(idler: usize, mode: u32, first: u32, second: u32, from: usize) -> bool {
    if !deferring_from(from, MAP_MODE_FROM_CAPITAL) {
        super::crossings::crossing(crate::hooked_memory().at(MAP_MODE_SET));
        return false;
    }
    defer(Deferred::MapMode { idler, mode, first: first & 0xff, second: second & 0xff });
    true
}

#[unsafe(naked)]
extern "C" fn map_mode_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rax, qword ptr [rbp + 8]", "mov qword ptr [rsp + 0x20], rax", call map_mode_set, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "mov qword ptr [rsp + 8], rbx",
        "mov qword ptr [rsp + 0x10], rbp",
        "jmp qword ptr [rip + {goes_on}]",
        "2:",
        "ret",
        map_mode_set = sym map_mode_set,
        goes_on = sym MAP_MODE_GOES_ON_AT,
    );
}

/// Whether a port's model waits for the next frame: from the center of
/// trade's update.
#[called_by_game]
extern "C" fn port_model(entry: usize, province: usize, map: usize, flag: u32, from: usize) -> bool {
    if !deferring_from(from, PORT_MODEL_FROM_TRADE) {
        super::crossings::crossing(crate::hooked_memory().at(PORT_MODEL));
        return false;
    }
    defer(Deferred::PortModel { entry, province, map, flag: flag & 0xff });
    true
}

#[unsafe(naked)]
extern "C" fn port_model_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rax, qword ptr [rbp + 8]", "mov qword ptr [rsp + 0x20], rax", call port_model, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "mov rax, rsp",
        "mov qword ptr [rax + 0x18], r8",
        "jmp qword ptr [rip + {goes_on}]",
        "2:",
        "ret",
        port_model = sym port_model,
        goes_on = sym PORT_MODEL_GOES_ON_AT,
    );
}

#[unsafe(naked)]
extern "C" fn minimap_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call minimap, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "mov rax, rsp",
        "mov qword ptr [rax + 0x10], rbx",
        "jmp qword ptr [rip + {goes_on}]",
        "2:",
        "ret",
        minimap = sym minimap,
        goes_on = sym MINIMAP_GOES_ON_AT,
    );
}

/// A mesh object freed waits for the next frame's preparation, also when a
/// deferred call frees it as the deferred calls are done: they are done after
/// the update that would take it off the map's markers' lists, so that only
/// the next update does, and the frame prepared meanwhile still draws it.
#[called_by_game]
unsafe extern "C" fn mesh_let_go(mesh: usize, free: u32) -> usize {
    if free & 1 != 0 && on_main() && drawing_on() {
        defer(Deferred::MeshLetGo { mesh, free });
        return mesh;
    }
    let m = crate::hooked_memory();
    super::crossings::mesh_let_go(m.at(MESH_LET_GO));
    unsafe { ORIGINAL_MESH_LET_GO.get()(mesh, free) }
}

/// The calls deferred, done as the game would have done them, by the main
/// thread holding the lock: as a frame is prepared, after its update, and
/// before the idler is switched or the game quits. Those whose idler,
/// interface or map has gone since are left out; every mesh object is let go.
pub(super) fn done(m: &Memory) {
    let deferred = std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|e| e.into_inner()));
    if deferred.is_empty() {
        return;
    }
    DOING.store(true, Ordering::Relaxed);
    let idler = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
    let interface = m.ptr(m.at(WORLD)).and_then(|world| m.ptr(world + WORLD_INTERFACE)).unwrap_or(0);
    let map = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_MAP)).unwrap_or(0);
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
            Deferred::OwnerTold { map: of, province } if of == map => unsafe {
                let told: unsafe extern "C" fn(usize, i32) = std::mem::transmute(m.at(OWNER_TOLD));
                told(of, province);
            },
            Deferred::DailyRefresh { interface: of } if of == interface => unsafe {
                ORIGINAL_DAILY_REFRESH.get()(of);
            },
            Deferred::Minimap { idler: of, minimap, flag } if of == idler => unsafe {
                let draw: unsafe extern "C" fn(usize, u32) = std::mem::transmute(m.at(MINIMAP));
                draw(minimap, flag);
            },
            Deferred::MapMode { idler: of, mode, first, second } if of == idler => unsafe {
                let set: unsafe extern "C" fn(usize, u32, u32, u32) = std::mem::transmute(m.at(MAP_MODE_SET));
                set(of, mode, first, second);
            },
            Deferred::PortModel { entry, province, map: of, flag } if of == map => unsafe {
                let model: unsafe extern "C" fn(usize, usize, usize, u32) = std::mem::transmute(m.at(PORT_MODEL));
                model(entry, province, of, flag);
            },
            Deferred::MeshLetGo { mesh, free } => unsafe {
                ORIGINAL_MESH_LET_GO.get()(mesh, free);
            },
            _ => {}
        }
    }
    DOING.store(false, Ordering::Relaxed);
}
