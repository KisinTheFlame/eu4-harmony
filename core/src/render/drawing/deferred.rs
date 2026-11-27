//! The deferred calls : what the main
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
use crate::base::called_by_game;
use crate::native::game_string;
use crate::native::memory::Memory;
use crate::native::objects::campaign::globals::WORLD;
use crate::native::points::{Function, Point};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Mutex, RwLock,
};

/// Bounded queue and reentry guard for main-thread calls deferred to frame
/// preparation.
struct DeferredQueue<T> {
    items: Mutex<Vec<T>>,
    count: AtomicU64,
    doing: AtomicBool,
}

struct DoingGuard<'a, T>(&'a DeferredQueue<T>);

impl<T> Drop for DoingGuard<'_, T> {
    fn drop(&mut self) {
        self.0.doing.store(false, Ordering::Release);
    }
}

/// Deferral rule for a hookable call while threaded drawing runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeferPolicy {
    /// Defer during the day; while replaying `done()`, run inline.
    DeferOnly,
    /// Defer when the caller matches the expected day-side call site;
    /// otherwise record a crossing during the day. While replaying `done()`,
    /// run inline.
    DeferOrCross { caller_matches: bool },
    /// Always defer on the main thread while threaded drawing runs, even
    /// during replay (for mesh frees whose objects are still drawn until the
    /// next frame update).
    AlwaysDefer,
}

/// Decision for a hookable call that may be deferred until frame preparation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeferredAction {
    Defer,
    RunNow,
    RunWithCrossing,
}

impl<T> DeferredQueue<T> {
    const fn new() -> Self {
        Self { items: Mutex::new(Vec::new()), count: AtomicU64::new(0), doing: AtomicBool::new(false) }
    }

    fn is_doing(&self) -> bool {
        self.doing.load(Ordering::Acquire)
    }

    fn decide(&self, on_main: bool, drawing_on: bool, policy: DeferPolicy) -> DeferredAction {
        if !on_main || !drawing_on {
            return DeferredAction::RunNow;
        }
        if self.is_doing() {
            return match policy {
                DeferPolicy::AlwaysDefer => DeferredAction::Defer,
                DeferPolicy::DeferOnly | DeferPolicy::DeferOrCross { .. } => DeferredAction::RunNow,
            };
        }
        match policy {
            DeferPolicy::DeferOnly | DeferPolicy::AlwaysDefer | DeferPolicy::DeferOrCross { caller_matches: true } => DeferredAction::Defer,
            DeferPolicy::DeferOrCross { caller_matches: false } => DeferredAction::RunWithCrossing,
        }
    }

    fn push(&self, item: T) {
        self.items.lock().unwrap_or_else(|e| e.into_inner()).push(item);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    fn push_if_absent(&self, item: T, mut same: impl FnMut(&T) -> bool) {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        if !items.iter().any(&mut same) {
            items.push(item);
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn take_for_execution(&self) -> Option<(Vec<T>, DoingGuard<'_, T>)> {
        let taken = std::mem::take(&mut *self.items.lock().unwrap_or_else(|e| e.into_inner()));
        if taken.is_empty() {
            return None;
        }
        self.doing.store(true, Ordering::Release);
        Some((taken, DoingGuard(self)))
    }

    fn logged(&self) -> u64 {
        self.count.swap(0, Ordering::Relaxed)
    }
}

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
        Point::keep(PORT_MODEL_FROM_MARKERS_A - 5, "e8 49 1e 00 00", "a port's model in the markers update, first"),
        Point::keep(PORT_MODEL_FROM_MARKERS_B - 5, "e8 13 1e 00 00", "a port's model in the markers update, second"),
        Point::keep(PORT_MODEL_FROM_MARKERS_C - 5, "e8 03 1d 00 00", "a port's model in the markers update, third"),
        Point::keep(0x10b7e3a, "4c 8b 87 60 02 00 00", "a minimap's texture").uses(&[MINIMAP_TEXTURE]),
        Point::jump(
            MINIMAP_UPLOAD_CALL,
            "45 8b c5 ff 15 02 95 f1 00",
            "the minimap's texture uploaded",
            minimap_upload_stub as *const () as usize,
            &MINIMAP_UPLOAD_RESUME,
        )
        .uses(&[TEXTURE_UPLOAD_SLOT])
        .recorded(&MINIMAP_UPLOAD_AT),
        Point::keep(MINIMAP_UPLOAD_DONE, "48 8d 97 68 02 00 00", "the minimap's texture upload done"),
        Point::keep(0x10a8c91, "48 8b 85 98 16 00 00", "the map mode color texture").uses(&[MAP_COLORS_TEXTURE]),
        Point::jump(
            MAP_COLORS_UPLOAD_CALL,
            "49 8b ce ff 15 3b 86 f2 00",
            "the map mode color texture uploaded",
            map_colors_upload_stub as *const () as usize,
            &MAP_COLORS_UPLOAD_RESUME,
        )
        .uses(&[TEXTURE_UPLOAD_SLOT])
        .recorded(&MAP_COLORS_UPLOAD_AT),
        Point::keep(MAP_COLORS_UPLOAD_DONE, "39 5f 7c 7e 1d", "the map mode color texture upload done"),
        Point::replace(
            PROVINCE_MAP_UPLOAD_FN,
            "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 70",
            "a province map texture uploaded",
            province_map_upload as ProvinceMapUploadFn,
            &ORIGINAL_PROVINCE_MAP_UPLOAD,
        ),
        Point::keep(TEXTURE_UPLOAD_FN, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 60 48 8b 84 24 98 00 00", "the graphics back end's texture upload"),
        Point::replace(
            TEXTURE_DESC_FN,
            "48 85 c9 0f 84 38 01 00 00 53 48 83 ec 40 48 8b 09",
            "a texture's dimensions and format",
            get_texture_desc as TextureDescFn,
            &ORIGINAL_TEXTURE_DESC,
        ),
        Point::replace(
            TEXTURE_RELEASE_FN,
            "48 85 c9 74 63 53 48 83 ec 20 48 8b d9 48 89 7c 24 30",
            "a texture's Direct3D interfaces released",
            release_texture as TextureReleaseFn,
            &ORIGINAL_TEXTURE_RELEASE,
        ),
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
resume!(OWNER_TOLD_RESUME: OWNER_TOLD_GOES_ON_AT = OWNER_TOLD_GOES_ON);
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
const MAP_MODE_SET: usize = crate::native::objects::map_rva::REFRESH.rva();
const MAP_MODE_GOES_ON: usize = 0x821ffa;
const MAP_MODE_FROM_CAPITAL: usize = 0xa1a194;
resume!(MAP_MODE_RESUME: MAP_MODE_GOES_ON_AT = MAP_MODE_GOES_ON);
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
/// Calls to `PORT_MODEL` inside the map's markers update (`0x1321880`), which
/// runs on the main thread during the frame update while already holding the
/// presentation lock right before `frame_draws`. They are part of the update
/// itself, not a crossing.
const PORT_MODEL_FROM_MARKERS_A: usize = 0x1321a67;
const PORT_MODEL_FROM_MARKERS_B: usize = 0x1321a9d;
const PORT_MODEL_FROM_MARKERS_C: usize = 0x1321bad;
resume!(PORT_MODEL_RESUME: PORT_MODEL_GOES_ON_AT = PORT_MODEL_GOES_ON);
const MINIMAP_GOES_ON: usize = 0x10b6c57;
resume!(MINIMAP_RESUME: MINIMAP_GOES_ON_AT = MINIMAP_GOES_ON);
/// Direct3D 9 texture uploads via `[0x1fd1378]` (`0x16d7e70`, 4-byte RGBA
/// upload through `D3DXLoadSurfaceFromMemory`). Running these uploads on the
/// main thread while the drawing thread is inside `Present` blocks on
/// Direct3D 9's internal device critical section; capturing the CPU pixel
/// buffers and flushing them on the drawing thread before drawing removes that
/// lock stall without racing on world state:
/// - `0x10b7e6d` in `0x10b6c50` (`eu3minimap.cpp`): minimap texture (`[minimap + 0x260]`).
/// - `0x10a8d34` in `0x10a8c30` (`pdxmaptexturegeneration.cpp`): map-mode color texture (`[pdxmap + 0x1698]`).
/// - `0x12ef4e0` (`pdxmaptexturegeneration.cpp`): province color & stripe textures (`*slot_ptr`), replaced at entry when `*slot_ptr` is already created.
const MINIMAP_TEXTURE: usize = 0x260;
const MINIMAP_UPLOAD_CALL: usize = 0x10b7e6d;
const MINIMAP_UPLOAD_DONE: usize = 0x10b7ea0;
const MAP_COLORS_TEXTURE: usize = 0x1698;
const MAP_COLORS_UPLOAD_CALL: usize = 0x10a8d34;
const MAP_COLORS_UPLOAD_DONE: usize = 0x10a8d3d;
const PROVINCE_MAP_UPLOAD_FN: usize = 0x12ef4e0;
type ProvinceMapUploadFn = unsafe extern "C" fn(usize, usize, usize, u32, u32);
static ORIGINAL_PROVINCE_MAP_UPLOAD: Function<ProvinceMapUploadFn> = Function::new();
const TEXTURE_UPLOAD_SLOT: usize = 0x1fd1378;
const TEXTURE_UPLOAD_FN: usize = 0x16d7e70;
resume!(MINIMAP_UPLOAD_RESUME: MINIMAP_UPLOAD_DONE_AT = MINIMAP_UPLOAD_DONE);
static MINIMAP_UPLOAD_AT: AtomicUsize = AtomicUsize::new(0);
resume!(MAP_COLORS_UPLOAD_RESUME: MAP_COLORS_UPLOAD_DONE_AT = MAP_COLORS_UPLOAD_DONE);
static MAP_COLORS_UPLOAD_AT: AtomicUsize = AtomicUsize::new(0);

/// `0x16d7f30(wrapper, out[4])` calls `IDirect3DTexture9::GetLevelDesc(0)` to
/// read the immutable `(width, height, format, bpp)` of a texture, blocking on
/// Direct3D 9's device critical section whenever the drawing thread is inside
/// `Present`. Caching by `(wrapper, *wrapper)` and evicting in `0x16d8080`
/// (the single back-end function that releases `*wrapper`) plus device reset
/// removes those repeated lock stalls cleanly.
const TEXTURE_DESC_FN: usize = 0x16d7f30;
const TEXTURE_RELEASE_FN: usize = 0x16d8080;
type TextureDescFn = unsafe extern "C" fn(usize, *mut u32);
type TextureReleaseFn = unsafe extern "C" fn(usize);
static ORIGINAL_TEXTURE_DESC: Function<TextureDescFn> = Function::new();
static ORIGINAL_TEXTURE_RELEASE: Function<TextureReleaseFn> = Function::new();
/// A mesh object's scalar deleting destructor (`pdxmeshobject.cpp`, the slot
/// 0 of its vtable `0x1d92408`, called through it only): the object and
/// whether to free it (bit 0). It takes the object off its mesh's list of
/// objects (`[+8]+0x140`), lets its parts go (`+0x58`, `+0x90`, `+0xa0`) and
/// frees its 0xf0 bytes; it answers the object. The map's markers keep the
/// objects of the units and the ports they draw on lists of theirs, which the
/// next update takes them off; the drawing reads them there until then. Freed
/// as the frame is prepared instead, the object outlives every frame that
/// draws it (docs/game/interface/frame-lifecycle.md, "Frames late at speed
/// 5").
const MESH_LET_GO: usize = 0x153e560;
type MeshLetGo = unsafe extern "C" fn(usize, u32) -> usize;
static ORIGINAL_MESH_LET_GO: Function<MeshLetGo> = Function::new();

/// What the day asked of the interface and waits for nothing of: done at the
/// next application frame, before its update (docs/plans/README.md#drawing,
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

static DEFERRED: DeferredQueue<Deferred> = DeferredQueue::new();

fn defer(call: Deferred) {
    DEFERRED.push(call);
}

/// How many calls were deferred since the last log.
pub(super) fn logged() -> u64 {
    DEFERRED.logged()
}

/// Whether a call of the main thread's waits for the next frame's preparation.
fn deferring() -> bool {
    DEFERRED.decide(on_main(), drawing_on(), DeferPolicy::DeferOnly) == DeferredAction::Defer
}

/// Decides a call that may either be deferred to frame preparation or cross
/// immediately when called from another site while threaded drawing runs.
fn defer_or_cross(caller_matches: bool, crossing_at: usize) -> bool {
    match DEFERRED.decide(on_main(), drawing_on(), DeferPolicy::DeferOrCross { caller_matches }) {
        DeferredAction::Defer => true,
        DeferredAction::RunWithCrossing => {
            super::crossings::crossing(crate::native::hooked_memory().at(crossing_at));
            false
        }
        DeferredAction::RunNow => false,
    }
}

#[called_by_game]
unsafe extern "C" fn map_text(idler: usize, province: u32, kind: usize, text: usize, number: usize, last: usize) -> usize {
    if deferring() {
        let m = crate::native::hooked_memory();
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
        call_rust!(call owner_told => bool),
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
    if !defer_or_cross(true, MINIMAP) {
        return false;
    }
    let m = crate::native::hooked_memory();
    let idler = m.ptr(m.at(APP)).and_then(|app| m.ptr(app + APP_IDLER)).unwrap_or(0);
    let flag = flag & 0xff;
    let due = Deferred::Minimap { idler, minimap, flag };
    DEFERRED.push_if_absent(due, |d| matches!(d, Deferred::Minimap { minimap: m, flag: f, .. } if *m == minimap && *f == flag));
    true
}

/// Whether the map mode's set waits for the next frame: from a moved capital.
#[called_by_game]
extern "C" fn map_mode_set(idler: usize, mode: u32, first: u32, second: u32, from: usize) -> bool {
    if !defer_or_cross(from == crate::native::hooked_memory().at(MAP_MODE_FROM_CAPITAL), MAP_MODE_SET) {
        return false;
    }
    defer(Deferred::MapMode { idler, mode, first: first & 0xff, second: second & 0xff });
    true
}

#[unsafe(naked)]
extern "C" fn map_mode_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rax, qword ptr [rbp + 8]", "mov qword ptr [rsp + 0x20], rax", call map_mode_set => bool),
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
/// trade's update. Calls from the map's markers update (`0x1321880`) run
/// inline without crossing since that update already owns the presentation
/// lock immediately before `frame_draws`.
#[called_by_game]
extern "C" fn port_model(entry: usize, province: usize, map: usize, flag: u32, from: usize) -> bool {
    let base = crate::native::hooked_memory().base;
    let rva = from.wrapping_sub(base);
    if matches!(rva, PORT_MODEL_FROM_MARKERS_A | PORT_MODEL_FROM_MARKERS_B | PORT_MODEL_FROM_MARKERS_C) {
        return false;
    }
    if !defer_or_cross(rva == PORT_MODEL_FROM_TRADE, PORT_MODEL) {
        return false;
    }
    defer(Deferred::PortModel { entry, province, map, flag: flag & 0xff });
    true
}

/// Latest CPU pixel buffers staged by the main thread for upload on the
/// drawing thread before its next frame, coalesced by owner `slot_ptr` (the
/// address holding the game's `GfxTexture*`), plus recycled pixel buffers so
/// steady-state frames do not allocate.
struct PendingTexture {
    slot_ptr: usize,
    renderer: usize,
    texture: usize,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

struct TextureUpload {
    slot_ptr: usize,
    renderer: usize,
    pixels: usize,
    width: u32,
    height: u32,
    texture: usize,
}

struct PendingTextures {
    staged: Vec<PendingTexture>,
    recycle: Vec<Vec<u8>>,
}

impl PendingTextures {
    const fn new() -> Self {
        Self { staged: Vec::new(), recycle: Vec::new() }
    }

    fn take_buffer(&mut self, len: usize) -> Vec<u8> {
        // Prefer the smallest recycled buffer that already fits `len`, else any recycled buffer.
        let best = self
            .recycle
            .iter()
            .enumerate()
            .filter(|(_, b)| b.capacity() >= len)
            .min_by_key(|(_, b)| b.capacity())
            .map(|(i, _)| i)
            .or_else(|| (!self.recycle.is_empty()).then_some(0));
        let mut buf = best.map_or_else(Vec::new, |i| self.recycle.swap_remove(i));
        buf.resize(len, 0);
        buf
    }

    fn return_buffer(&mut self, buf: Vec<u8>) {
        if self.recycle.len() < 8 {
            self.recycle.push(buf);
        }
    }

    fn stage(&mut self, m: &Memory, upload: TextureUpload) -> Result<bool, &'static str> {
        if !m.ptr(upload.slot_ptr).is_ok_and(|t| t == upload.texture) {
            return Err("slot pointer does not hold texture");
        }
        let len = (upload.width as usize) * (upload.height as usize) * 4;
        if let Some(idx) = self.staged.iter().position(|p| p.slot_ptr == upload.slot_ptr) {
            let mut buf = self.take_buffer(len);
            if m.read_into(upload.pixels, &mut buf).is_err() {
                self.return_buffer(buf);
                let old = self.staged.swap_remove(idx);
                self.return_buffer(old.pixels);
                return Ok(false);
            }
            let existing = &mut self.staged[idx];
            let old_buf = std::mem::replace(&mut existing.pixels, buf);
            existing.renderer = upload.renderer;
            existing.texture = upload.texture;
            existing.width = upload.width;
            existing.height = upload.height;
            self.return_buffer(old_buf);
            return Ok(true);
        }
        let mut buf = self.take_buffer(len);
        if m.read_into(upload.pixels, &mut buf).is_err() {
            self.return_buffer(buf);
            return Ok(false);
        }
        self.staged.push(PendingTexture {
            slot_ptr: upload.slot_ptr,
            renderer: upload.renderer,
            texture: upload.texture,
            width: upload.width,
            height: upload.height,
            pixels: buf,
        });
        Ok(true)
    }
}

static PENDING_TEXTURES: Mutex<PendingTextures> = Mutex::new(PendingTextures::new());
static SLOT_MISMATCH_LOGGED: AtomicBool = AtomicBool::new(false);

fn defer_texture_upload_inner(slot_ptr: usize, renderer: usize, pixels: usize, width: u32, height: u32, texture: usize) -> bool {
    if !on_main()
        || !drawing_on()
        || super::FRAMES.prepared_idler() == 0
        || slot_ptr == 0
        || renderer == 0
        || pixels == 0
        || texture == 0
        || width == 0
        || height == 0
        || width > 8192
        || height > 8192
    {
        return false;
    }
    let m = crate::native::hooked_memory();
    let staged = {
        let mut state = PENDING_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
        state.stage(&m, TextureUpload { slot_ptr, renderer, pixels, width, height, texture })
    };
    match staged {
        Ok(staged) => staged,
        Err(reason) => {
            crate::base::broken_invariant(&SLOT_MISMATCH_LOGGED, format_args!("drawing texture upload ({reason}): slot={slot_ptr:#x} texture={texture:#x}"));
            false
        }
    }
}

#[called_by_game]
extern "C" fn defer_texture_upload(slot_ptr: usize, renderer: usize, pixels: usize, width: u32, height: u32, texture: usize) -> bool {
    defer_texture_upload_inner(slot_ptr, renderer, pixels, width, height, texture)
}

#[called_by_game]
unsafe extern "C" fn province_map_upload(renderer: usize, slot_ptr: usize, pixels: usize, width: u32, height: u32) {
    if slot_ptr != 0 {
        let texture = unsafe { *(slot_ptr as *const usize) };
        if texture != 0 && defer_texture_upload_inner(slot_ptr, renderer, pixels, width, height, texture) {
            return;
        }
    }
    unsafe { ORIGINAL_PROVINCE_MAP_UPLOAD.get()(renderer, slot_ptr, pixels, width, height) };
}

#[unsafe(naked)]
extern "C" fn minimap_upload_stub() {
    std::arch::naked_asm!(
        call_rust!(
            "mov qword ptr [rsp + 0x28], r8",
            "mov dword ptr [rsp + 0x20], r9d",
            "mov r9d, r13d",
            "mov r8, rdx",
            "mov rdx, rcx",
            "lea rcx, [rdi + {slot_off}]",
            call defer_texture_upload => bool
        ),
        "jne 2f",
        "mov r8d, r13d",
        "mov rax, qword ptr [rip + {at}]",
        "add rax, {to_slot}",
        "call qword ptr [rax]",
        "2:",
        "jmp qword ptr [rip + {done}]",
        slot_off = const MINIMAP_TEXTURE,
        defer_texture_upload = sym defer_texture_upload,
        at = sym MINIMAP_UPLOAD_AT,
        to_slot = const TEXTURE_UPLOAD_SLOT - MINIMAP_UPLOAD_CALL,
        done = sym MINIMAP_UPLOAD_DONE_AT,
    );
}

#[unsafe(naked)]
extern "C" fn map_colors_upload_stub() {
    std::arch::naked_asm!(
        // `call_rust!` saves the game's `rbp` (`pdxmap`) at `[rbp]` before setting
        // up its own frame pointer, so dereference `[rbp]` for `slot_ptr`.
        call_rust!(
            "mov qword ptr [rsp + 0x28], rax",
            "mov dword ptr [rsp + 0x20], r9d",
            "mov r9d, r10d",
            "mov r8, r11",
            "mov rdx, r14",
            "mov rcx, qword ptr [rbp]",
            "add rcx, {slot_off}",
            call defer_texture_upload => bool
        ),
        "jne 2f",
        "mov rcx, r14",
        "mov rax, qword ptr [rip + {at}]",
        "add rax, {to_slot}",
        "call qword ptr [rax]",
        "2:",
        "jmp qword ptr [rip + {done}]",
        slot_off = const MAP_COLORS_TEXTURE,
        defer_texture_upload = sym defer_texture_upload,
        at = sym MAP_COLORS_UPLOAD_AT,
        to_slot = const TEXTURE_UPLOAD_SLOT - MAP_COLORS_UPLOAD_CALL,
        done = sym MAP_COLORS_UPLOAD_DONE_AT,
    );
}

/// Uploads all staged RGBA texture pixel buffers into Direct3D 9. Called on
/// the drawing thread under the presentation lock (outside `Present`), or on
/// the main thread immediately before inline fallback drawing once `Present`
/// has returned.
pub(super) fn flush_textures(m: &Memory) {
    let pending = {
        let mut state = PENDING_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
        if state.staged.is_empty() {
            return;
        }
        std::mem::take(&mut state.staged)
    };
    type UploadTexture = unsafe extern "C" fn(usize, *const u8, u32, u32, u32, usize);
    let upload: UploadTexture = unsafe { std::mem::transmute(m.at(TEXTURE_UPLOAD_FN)) };
    for item in &pending {
        if m.ptr(item.slot_ptr).is_ok_and(|t| t == item.texture) {
            unsafe { upload(item.renderer, item.pixels.as_ptr(), item.width, item.height, 4, item.texture) };
        }
    }
    let mut state = PENDING_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    for item in pending {
        state.return_buffer(item.pixels);
    }
}

/// Discards any staged texture uploads on idler switch, shutdown, or device reset.
pub(super) fn clear_textures() {
    let mut state = PENDING_TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    let old = std::mem::take(&mut state.staged);
    for item in old {
        state.return_buffer(item.pixels);
    }
}

/// Cached immutable level-0 descriptor returned by `0x16d7f30`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CachedTexDesc {
    width: u32,
    height: u32,
    format_and_bpp: Option<(u32, u32)>,
}

static TEX_DESC_CACHE: std::sync::LazyLock<RwLock<HashMap<(usize, usize), CachedTexDesc>>> = std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Clears cached texture descriptors (called before Direct3D device reset).
pub(super) fn clear_texture_desc_cache() {
    TEX_DESC_CACHE.write().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Evicts cached texture descriptors matching `wrapper` or `d3d_tex`, called
/// from `0x16d8080` and whenever framework code replaces `*wrapper` directly.
pub(super) fn evict_texture_desc(wrapper: usize, d3d_tex: usize) {
    if wrapper == 0 && d3d_tex == 0 {
        return;
    }
    let mut cache = TEX_DESC_CACHE.write().unwrap_or_else(|e| e.into_inner());
    if wrapper != 0 && d3d_tex != 0 {
        cache.remove(&(wrapper, d3d_tex));
    }
    cache.retain(|&(w, t), _| (wrapper == 0 || w != wrapper) && (d3d_tex == 0 || t != d3d_tex));
}

#[called_by_game]
unsafe extern "C" fn get_texture_desc(wrapper: usize, out: *mut u32) {
    if wrapper == 0 || out.is_null() {
        return;
    }
    let d3d_tex = unsafe { *(wrapper as *const usize) };
    if d3d_tex == 0 {
        return;
    }
    let key = (wrapper, d3d_tex);
    if let Some(desc) = TEX_DESC_CACHE.read().unwrap_or_else(|e| e.into_inner()).get(&key).copied() {
        unsafe {
            *out = desc.width;
            *out.add(1) = desc.height;
            if let Some((fmt, bpp)) = desc.format_and_bpp {
                *out.add(2) = fmt;
                *out.add(3) = bpp;
            }
        }
        return;
    }
    const UNSET: u32 = u32::MAX;
    let mut probe = [0u32, 0u32, UNSET, UNSET];
    unsafe { ORIGINAL_TEXTURE_DESC.get()(wrapper, probe.as_mut_ptr()) };
    let desc = CachedTexDesc { width: probe[0], height: probe[1], format_and_bpp: (probe[2] != UNSET).then_some((probe[2], probe[3])) };
    unsafe {
        *out = desc.width;
        *out.add(1) = desc.height;
        if let Some((fmt, bpp)) = desc.format_and_bpp {
            *out.add(2) = fmt;
            *out.add(3) = bpp;
        }
    }
    TEX_DESC_CACHE.write().unwrap_or_else(|e| e.into_inner()).insert(key, desc);
}

#[called_by_game]
unsafe extern "C" fn release_texture(wrapper: usize) {
    if wrapper != 0 {
        let d3d_tex = unsafe { *(wrapper as *const usize) };
        evict_texture_desc(wrapper, d3d_tex);
    }
    unsafe { ORIGINAL_TEXTURE_RELEASE.get()(wrapper) };
}

#[unsafe(naked)]
extern "C" fn port_model_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rax, qword ptr [rbp + 8]", "mov qword ptr [rsp + 0x20], rax", call port_model => bool),
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
        call_rust!(call minimap => bool),
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
    if free & 1 != 0 && DEFERRED.decide(on_main(), drawing_on(), DeferPolicy::AlwaysDefer) == DeferredAction::Defer {
        defer(Deferred::MeshLetGo { mesh, free });
        return mesh;
    }
    let m = crate::native::hooked_memory();
    super::crossings::mesh_let_go(m.at(MESH_LET_GO));
    unsafe { ORIGINAL_MESH_LET_GO.get()(mesh, free) }
}

/// The calls deferred, done as the game would have done them, by the main
/// thread holding the lock: as a frame is prepared, after its update, and
/// before the idler is switched or the game quits. Those whose idler,
/// interface or map has gone since are left out; every mesh object is let go.
pub(super) fn done(m: &Memory) {
    let Some((deferred, _doing)) = DEFERRED.take_for_execution() else {
        return;
    };
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_queue_deduplicates_and_resets_doing_even_on_unwind() {
        let queue: DeferredQueue<(usize, u32)> = DeferredQueue::new();
        queue.push_if_absent((10, 1), |x| *x == (10, 1));
        queue.push_if_absent((10, 1), |x| *x == (10, 1));
        queue.push_if_absent((10, 2), |x| *x == (10, 2));
        assert_eq!(queue.logged(), 2);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (items, _guard) = queue.take_for_execution().unwrap();
            assert_eq!(items.len(), 2);
            assert!(queue.is_doing());
            panic!("simulated unwind during deferred execution");
        }));
        assert!(result.is_err());
        assert!(!queue.is_doing(), "RAII guard must clear `doing` on unwind");
        assert!(queue.take_for_execution().is_none());
    }

    #[test]
    fn deferred_policy_runs_inline_during_replay_without_spurious_crossings() {
        let queue: DeferredQueue<u32> = DeferredQueue::new();
        assert_eq!(queue.decide(true, false, DeferPolicy::DeferOnly), DeferredAction::RunNow);
        assert_eq!(queue.decide(true, false, DeferPolicy::DeferOrCross { caller_matches: false }), DeferredAction::RunNow);

        assert_eq!(queue.decide(true, true, DeferPolicy::DeferOnly), DeferredAction::Defer);
        assert_eq!(queue.decide(true, true, DeferPolicy::DeferOrCross { caller_matches: true }), DeferredAction::Defer);
        assert_eq!(queue.decide(true, true, DeferPolicy::DeferOrCross { caller_matches: false }), DeferredAction::RunWithCrossing);
        assert_eq!(queue.decide(true, true, DeferPolicy::AlwaysDefer), DeferredAction::Defer);

        queue.push(1);
        let (_items, _guard) = queue.take_for_execution().unwrap();
        assert_eq!(queue.decide(true, true, DeferPolicy::DeferOnly), DeferredAction::RunNow);
        assert_eq!(queue.decide(true, true, DeferPolicy::DeferOrCross { caller_matches: true }), DeferredAction::RunNow);
        assert_eq!(queue.decide(true, true, DeferPolicy::DeferOrCross { caller_matches: false }), DeferredAction::RunNow);
        assert_eq!(queue.decide(true, true, DeferPolicy::AlwaysDefer), DeferredAction::Defer);
    }

    #[test]
    fn pending_textures_rejects_mismatched_slot_and_coalesces_matching_uploads() {
        let m = Memory { base: 0 };
        let mut pending = PendingTextures::new();
        let tex_a: usize = 0x1111_2222;
        let tex_b: usize = 0x3333_4444;
        let wrong_slot: usize = 0xdead_beef;
        let pixels_1: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        let pixels_2: [u8; 8] = [9, 10, 11, 12, 13, 14, 15, 16];

        // A wrong slot_ptr (such as reading call_rust!'s frame pointer instead of the
        // game's saved rbp) must be rejected instead of staging an upload that
        // flush_textures would silently drop.
        assert!(pending
            .stage(
                &m,
                TextureUpload {
                    slot_ptr: &wrong_slot as *const usize as usize,
                    renderer: 1,
                    pixels: pixels_1.as_ptr() as usize,
                    width: 2,
                    height: 1,
                    texture: tex_a
                }
            )
            .is_err());
        assert!(pending.staged.is_empty());

        // Matching slots stage cleanly, coalesce by slot_ptr, and keep distinct slots separate.
        assert_eq!(
            pending.stage(
                &m,
                TextureUpload {
                    slot_ptr: &tex_a as *const usize as usize,
                    renderer: 1,
                    pixels: pixels_1.as_ptr() as usize,
                    width: 2,
                    height: 1,
                    texture: tex_a
                }
            ),
            Ok(true)
        );
        assert_eq!(
            pending.stage(
                &m,
                TextureUpload {
                    slot_ptr: &tex_a as *const usize as usize,
                    renderer: 2,
                    pixels: pixels_2.as_ptr() as usize,
                    width: 2,
                    height: 1,
                    texture: tex_a
                }
            ),
            Ok(true)
        );
        assert_eq!(
            pending.stage(
                &m,
                TextureUpload {
                    slot_ptr: &tex_b as *const usize as usize,
                    renderer: 3,
                    pixels: pixels_1.as_ptr() as usize,
                    width: 2,
                    height: 1,
                    texture: tex_b
                }
            ),
            Ok(true)
        );
        assert_eq!(pending.staged.len(), 2);
        assert_eq!(pending.staged[0].renderer, 2);
        assert_eq!(pending.staged[0].pixels, pixels_2);
        assert_eq!(pending.staged[1].slot_ptr, &tex_b as *const usize as usize);
        assert_eq!(pending.staged[1].pixels, pixels_1);
    }

    extern "C" fn capture_slot(slot_ptr: usize) -> usize {
        slot_ptr
    }

    #[unsafe(naked)]
    extern "C" fn map_colors_rbp_slot_probe(_game_rbp: usize) -> usize {
        std::arch::naked_asm!(
            "push rbp",
            "mov rbp, rcx",
            call_rust!(
                "mov rcx, qword ptr [rbp]",
                "add rcx, {slot_off}",
                call capture_slot -> rax
            ),
            "pop rbp",
            "ret",
            slot_off = const MAP_COLORS_TEXTURE,
            capture_slot = sym capture_slot,
        );
    }

    #[test]
    fn map_colors_slot_ptr_uses_saved_game_rbp_through_call_rust() {
        let base = 0x1000_0000usize;
        assert_eq!(map_colors_rbp_slot_probe(base), base + MAP_COLORS_TEXTURE);
    }

    #[test]
    fn evict_texture_desc_clears_matching_wrapper_or_texture_entries() {
        let desc = CachedTexDesc { width: 512, height: 512, format_and_bpp: Some((0xb, 4)) };
        {
            let mut cache = TEX_DESC_CACHE.write().unwrap_or_else(|e| e.into_inner());
            cache.insert((0x9001, 0xa001), desc);
            cache.insert((0x9001, 0xa002), desc);
            cache.insert((0x9002, 0xa001), desc);
            cache.insert((0x9003, 0xa003), desc);
        }
        evict_texture_desc(0x9001, 0xa001);
        let cache = TEX_DESC_CACHE.read().unwrap_or_else(|e| e.into_inner());
        assert!(!cache.contains_key(&(0x9001, 0xa001)));
        assert!(!cache.contains_key(&(0x9001, 0xa002)), "same wrapper evicted");
        assert!(!cache.contains_key(&(0x9002, 0xa001)), "same d3d texture evicted");
        assert!(cache.contains_key(&(0x9003, 0xa003)), "unrelated entry preserved");
        drop(cache);
        evict_texture_desc(0x9003, 0xa003);
    }
}
