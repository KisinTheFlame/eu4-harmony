//! The deferred calls (docs/plans/drawing-thread.md, C2): what the day asks
//! of the interface and waits for nothing of, kept and done by the main
//! thread once it holds the lock again, at the day's end or the next
//! application frame, before the frame is prepared. Until then the drawing
//! thread draws on, as nothing changed.
use super::{drawing_on, main_side, on_main, APP, APP_IDLER, APP_MAP};
use crate::called_by_game;
use crate::game_string;
use crate::memory::Memory;
use crate::objects::campaign::globals::WORLD;
use crate::points::{Function, Point};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::replace(MAP_TEXT, "48 89 5c 24 10 4c 89 4c 24 20", "a text floating on the map", map_text as MapText, &ORIGINAL_MAP_TEXT),
        // mov rcx, rsi; call string destructor: the text's type destroyed.
        Point::keep(0x6e9f68, "48 8b ce e8 f0 b6 9a ff 90 48 8b 8d a8 00 00 00", "a floating text's strings destroyed"),
        Point::replace(DAILY_REFRESH, "4c 8b dc 49 89 5b 18 55 56 57", "the interface's daily refresh", daily_refresh as DailyRefresh, &ORIGINAL_DAILY_REFRESH),
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

/// What the day asked of the interface and waits for nothing of: done at the
/// next application frame, before its update (docs/plans/drawing-thread.md,
/// C2). Until then the drawing thread draws on, as nothing changed.
enum Deferred {
    MapText { idler: usize, province: u32, kind: Vec<u8>, text: Vec<u8>, number: usize, last: usize },
    DailyRefresh { interface: usize },
    OwnerTold { map: usize, province: i32 },
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

/// Whether the map's owner mark waits for the day's end.
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

/// The calls deferred, done as the game would have done them, by the main
/// thread holding the lock; those whose idler or interface has gone since
/// are left out.
pub(super) fn done(m: &Memory) {
    let deferred = std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|e| e.into_inner()));
    if deferred.is_empty() {
        return;
    }
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
            _ => {}
        }
    }
}
