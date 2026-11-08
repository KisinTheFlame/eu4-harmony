//! Where the day goes into the interface's or the graphics' code (the
//! `dev-crossings` feature: a player's build has none of this), for the
//! drawing thread (docs/plans/drawing-thread.md, C1): each crossing (GLOSSARY)
//! a day reaches, by its call stack and how often.
//!
//! While the daily update runs on the main thread, between two of the game's
//! points for a frame inside a day (`0x769ea0`), the code of the interface and
//! the graphics is not executable: the game's own (`eu4\source\graphics`), the
//! engine's GUI and graphics, its graphics back end, animation, particles and
//! sound, and `d3d9.dll` and `d3dx9_43.dll`. A call into it stops at an
//! exception handler, which counts the call's stack and makes that page
//! executable again for the rest of the stretch; at the next point, and at
//! the day's end, every page is executable again, and the next stretch
//! begins with them all not. So each crossing is counted at least once in a
//! stretch it comes in, the first call into each page of it; a crossing into
//! a page an earlier one opened in the same stretch is not seen, which many
//! days make up for. What the day writes of the interface's data without
//! calling its code is not seen at all.
//!
//! dev.toml's `[crossings]` says how many days pass between two reports in
//! the framework log (`report-every`), each the stacks counted since the
//! start, most first.
use crate::called_by_game;
use crate::hooks::Call;
use crate::points::Point;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::EXCEPTION_ACCESS_VIOLATION;
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, RtlLookupFunctionEntry, RtlVirtualUnwind, CONTEXT, EXCEPTION_POINTERS, UNW_FLAG_NHANDLER,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READ, PAGE_READONLY};
use windows_sys::Win32::System::Threading::{GetCurrentThreadId, GetCurrentThreadStackLimits};

const PAGE: usize = 0x1000;
const FRAMES: usize = 10;
const STACKS: usize = 4096;

/// The code of the interface and the graphics in `eu4.exe`, as RVAs: the
/// game's `eu4\source\graphics\` and the code after it up to the engine's
/// (no source file named there, the units' models among it), the engine's
/// graphics and GUI (`clausewitzlib\graphics`), and its animation, sound,
/// particles and graphics back end (`pdx_anim`, `pdx_audio`, `pdx_particle`,
/// `pdx_gfx`), by where the functions naming those source files lie.
const GAME_RANGES: [(usize, usize); 4] = [(0xde6000, 0x14a6000), (0x14d9000, 0x1500000), (0x1540000, 0x1640000), (0x169e000, 0x16fb000)];
/// Functions in those ranges that are no part of the interface or the
/// graphics, read in the code: a spin wait (`SwitchToThread`) the parallel
/// stages and the flags take, a clock, a string's helper, a number's
/// formatting, and a table of the tick profiler's. A call into them opens
/// their page and is not counted.
const NOT_PRESENTATION: [usize; 5] = [crate::loading::opens::SPIN, 0x15e4f00, 0x16f7a20, 0x125b470, 0x1580370];
/// Modules whose code is the graphics' too.
const MODULES: [&str; 2] = ["d3d9.dll", "d3dx9_43.dll"];

/// One stack counted.
struct Stack {
    frames: [u64; FRAMES],
    count: u64,
    thread: u32,
    date: i32,
}

/// The ranges made not executable, as addresses: start, end, and the
/// protection each page goes back to.
static RANGES: OnceLock<Vec<(usize, usize)>> = OnceLock::new();
static STACKS_SEEN: Mutex<Vec<Stack>> = Mutex::new(Vec::new());
/// A stretch is on: the pages were made not executable.
static ON: AtomicBool = AtomicBool::new(false);
/// The daily update runs.
static IN_DAY: AtomicBool = AtomicBool::new(false);
static DAYS: AtomicUsize = AtomicUsize::new(0);
static FAULTS: AtomicU64 = AtomicU64::new(0);
static STRETCHES: AtomicU64 = AtomicU64::new(0);
static DATE: AtomicU32 = AtomicU32::new(0);
static BASE: AtomicUsize = AtomicUsize::new(0);

fn report_every() -> usize {
    crate::dev::options().crossings.report_every.max(1)
}

/// The code ranges, found once: `eu4.exe`'s and each module's code section.
fn ranges() -> &'static [(usize, usize)] {
    RANGES.get_or_init(|| {
        let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
        BASE.store(base, Ordering::Relaxed);
        let mut out: Vec<(usize, usize)> = GAME_RANGES.iter().map(|&(lo, hi)| (base + lo, base + hi)).collect();
        for module in MODULES {
            let name: Vec<u16> = module.encode_utf16().chain([0]).collect();
            let at = unsafe { GetModuleHandleW(name.as_ptr()) } as usize;
            if at == 0 {
                log!("crossings: {module} is not loaded, its code is not watched");
                continue;
            }
            // The module's .text: from its section table.
            let image = unsafe { std::slice::from_raw_parts(at as *const u8, PAGE) };
            let pe = u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap_or_default()) as usize;
            let sections = u16::from_le_bytes([image[pe + 6], image[pe + 7]]) as usize;
            let table = pe + 24 + u16::from_le_bytes([image[pe + 20], image[pe + 21]]) as usize;
            for i in 0..sections {
                let s = table + 40 * i;
                let characteristics = u32::from_le_bytes(image[s + 36..s + 40].try_into().unwrap_or_default());
                // IMAGE_SCN_MEM_EXECUTE
                if characteristics & 0x2000_0000 != 0 {
                    let va = u32::from_le_bytes(image[s + 12..s + 16].try_into().unwrap_or_default()) as usize;
                    let size = u32::from_le_bytes(image[s + 8..s + 12].try_into().unwrap_or_default()) as usize;
                    out.push((at + va, (at + va + size + PAGE - 1) & !(PAGE - 1)));
                }
            }
        }
        out
    })
}

fn protect_all(protection: u32) {
    for &(lo, hi) in ranges() {
        let mut old = 0u32;
        unsafe { VirtualProtect(lo as _, hi - lo, protection, &mut old) };
    }
}

fn watched(address: usize) -> bool {
    ranges().iter().any(|&(lo, hi)| (lo..hi).contains(&address))
}

/// The return addresses of the calls `context` is in, its own address
/// first.
fn frames(context: &CONTEXT) -> [u64; FRAMES] {
    let (mut low, mut high) = (0usize, 0usize);
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    let mut out = [0u64; FRAMES];
    out[0] = context.Rip;
    let mut c = *context;
    for slot in out.iter_mut().skip(1) {
        let mut image = 0u64;
        let entry = unsafe { RtlLookupFunctionEntry(c.Rip, &mut image, std::ptr::null_mut()) };
        if entry.is_null() {
            let rsp = c.Rsp as usize;
            if rsp < low || rsp + 8 > high {
                break;
            }
            c.Rip = unsafe { *(rsp as *const u64) };
            c.Rsp += 8;
        } else {
            let mut data = std::ptr::null_mut();
            let mut frame = 0u64;
            unsafe { RtlVirtualUnwind(UNW_FLAG_NHANDLER, image, c.Rip, entry, &mut c, &mut data, &mut frame, std::ptr::null_mut()) };
        }
        if c.Rip == 0 || (c.Rsp as usize) < low || c.Rsp as usize > high {
            break;
        }
        *slot = c.Rip;
    }
    out
}

const CONTINUE_EXECUTION: i32 = -1;
const CONTINUE_SEARCH: i32 = 0;

#[called_by_game]
unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let (record, context) = (&*(*info).ExceptionRecord, &*(*info).ContextRecord);
    // An execution of a page that is not executable.
    if record.ExceptionCode != EXCEPTION_ACCESS_VIOLATION || record.NumberParameters < 2 || record.ExceptionInformation[0] != 8 {
        return CONTINUE_SEARCH;
    }
    let address = record.ExceptionInformation[1];
    if !watched(address) {
        return CONTINUE_SEARCH;
    }
    let mut old = 0u32;
    VirtualProtect((address & !(PAGE - 1)) as _, PAGE, PAGE_EXECUTE_READ, &mut old);
    let base = BASE.load(Ordering::Relaxed);
    let mut image = 0u64;
    let entry = RtlLookupFunctionEntry(address as u64, &mut image, std::ptr::null_mut());
    if !entry.is_null() && image as usize == base && NOT_PRESENTATION.contains(&((*entry).BeginAddress as usize)) {
        return CONTINUE_EXECUTION;
    }
    if !ON.load(Ordering::Acquire) {
        // The stretch ended while this call was on its way.
        return CONTINUE_EXECUTION;
    }
    FAULTS.fetch_add(1, Ordering::Relaxed);
    let frames = frames(context);
    let thread = GetCurrentThreadId();
    let mut stacks = STACKS_SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = stacks.iter_mut().find(|s| s.frames == frames && s.thread == thread) {
        s.count += 1;
    } else if stacks.len() < STACKS {
        stacks.push(Stack { frames, count: 1, thread, date: DATE.load(Ordering::Relaxed) as i32 });
    }
    CONTINUE_EXECUTION
}

/// A stretch of the day begins: from the day's start, and as each check for
/// a frame returns.
fn begin() {
    if IN_DAY.load(Ordering::Acquire) {
        STRETCHES.fetch_add(1, Ordering::Relaxed);
        protect_all(PAGE_READONLY);
        ON.store(true, Ordering::Release);
    }
}

/// A stretch ends: at each check for a frame, and at the day's end.
fn end() {
    if ON.swap(false, Ordering::AcqRel) {
        protect_all(PAGE_EXECUTE_READ);
    }
}

/// The daily update begins (`game::events`).
pub fn day_begins(m: &crate::memory::Memory) {
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    if let Ok(w) = crate::game::world_context_any(m) {
        DATE.store(w.date as u32, Ordering::Relaxed);
    }
    IN_DAY.store(true, Ordering::Release);
    begin();
}

/// The daily update has returned (`game::events`).
pub fn day_ends() {
    if crate::thread_id() != crate::main_thread() {
        return;
    }
    end();
    IN_DAY.store(false, Ordering::Release);
    let days = DAYS.fetch_add(1, Ordering::Relaxed) + 1;
    if days.is_multiple_of(report_every()) {
        report(days);
    }
}

/// The check for a frame inside a day begins (`drawing`, which observes it).
pub fn check_before(_: &mut Call) {
    if crate::thread_id() == crate::main_thread() {
        end();
    }
}

/// The check for a frame inside a day returns.
pub fn check_after(_: &mut Call) {
    if crate::thread_id() == crate::main_thread() {
        begin();
    }
}

/// Where an address is: an RVA into `eu4.exe`, or a module and an offset.
fn place(address: u64) -> String {
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    let base = BASE.load(Ordering::Relaxed) as u64;
    if address >= base && address < base + 0x1b66000 {
        return format!("{:#x}", address - base);
    }
    let mut module = std::ptr::null_mut();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    if unsafe { GetModuleHandleExW(flags, address as _, &mut module) } == 0 {
        return format!("{address:#x}");
    }
    let mut name = [0u16; 260];
    let n = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
    let path = String::from_utf16_lossy(&name[..n]);
    let file = path.rsplit('\\').next().unwrap_or(&path).to_string();
    format!("{file}+{:#x}", address - module as u64)
}

fn report(days: usize) {
    let stacks = STACKS_SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let main = crate::main_thread();
    let mut order: Vec<&Stack> = stacks.iter().collect();
    order.sort_by_key(|s| std::cmp::Reverse(s.count));
    log!(
        "crossings after {days} day(s): {} stacks, {} calls into the interface's or the graphics' code in {} stretches",
        stacks.len(),
        FAULTS.load(Ordering::Relaxed),
        STRETCHES.load(Ordering::Relaxed)
    );
    for s in order {
        let frames: Vec<String> = s.frames.iter().take_while(|&&f| f != 0).map(|&f| place(f)).collect();
        let thread = if s.thread == main { "main".to_string() } else { format!("thread {}", s.thread) };
        let date = eu4_harmony_checks::Date(s.date).to_string();
        log!("crossing: {} time(s), {thread}, first {date}: {}", s.count, frames.join(" < "));
    }
}

/// The handler, installed as startup installs the points.
pub fn start() {
    let _ = ranges();
    if unsafe { AddVectoredExceptionHandler(1, Some(handler)) }.is_null() {
        log!("crossings: no exception handler, nothing is counted");
        return;
    }
    log!("crossings: watching the day's calls into {} code ranges, a report every {} day(s)", ranges().len(), report_every());
}

/// No points of its own: the check for a frame inside a day is observed by
/// the drawing thread's speed-up, which calls [`check_before`] and
/// [`check_after`], so the drawing thread's speed-up must not be left to the
/// game while crossings are counted.
pub fn points() -> Vec<Point> {
    Vec::new()
}
