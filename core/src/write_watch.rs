//! What a game function writes (the `dev-write-watch` feature: a player's
//! build has none of this). dev.toml's `[write-watch]` names a function by
//! its RVA; during its first `calls` calls on the main thread, the process's
//! other threads are suspended, but for the threads that write a save's
//! countries and provinces with the main thread (`saving::parallel`), whose
//! writes are recorded too, and every page of the process heap that
//! holds a block, and every writable page of `eu4.exe`, is read-only. Each
//! write to one stops at an exception handler, which records the
//! instruction, the calls it was in and what it wrote to, makes the page
//! writable for that one instruction, and makes it read-only again once it
//! has run. When the call returns, the pages get their protection back, the
//! threads go on, and the log lists the writes, grouped by instruction and
//! calls, most first.
//!
//! What was written to is one of:
//! - `static`: `eu4.exe`'s own data;
//! - `block`: a heap block that was there when the call began (its size and
//!   the offset written are given): memory the function did not make;
//! - `new`: the rest of the heap's pages, where blocks allocated during the
//!   call lie, and the heap's own records.
//!
//! With `reads-before`, the next call on the main thread of each function it
//! names, before each watched call, is watched first, its pages inaccessible
//! instead of read-only, so that what it reads stops too; the blocks and the
//! eight bytes of `eu4.exe`'s data it read or wrote are kept, and the watched
//! call's writes to them are marked "read before". So a drawing function and
//! the day after it say what the day changes of what the drawing read. With
//! `after-days`, the watch begins once that many game days have passed one
//! after another, as the watched functions see the date.
//!
//! The other threads are held because the kernel's writes for a system call
//! (a message read into a heap buffer) fail on a read-only page instead of
//! stopping, and Windows' messaging ended the game so when they ran on
//! (docs/prototype-log/2026-11-04-writing-game-state.md). A function that
//! waits for another thread waits for ever, and so does one that takes a lock
//! a held thread has: the census takes one at every allocation, so build the
//! watch without it. The game runs many times slower while watched: the watch
//! is for finding out, not for play.
use crate::analysis;
use crate::called_by_game;
use crate::hooks::Call;
use crate::memory::Memory;
use crate::objects::campaign;
use crate::points::Point;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, EXCEPTION_SINGLE_STEP};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, RtlLookupFunctionEntry, RtlVirtualUnwind, CONTEXT, EXCEPTION_POINTERS, UNW_FLAG_NHANDLER,
};
use windows_sys::Win32::System::Diagnostics::Debug::{GetThreadContext, CONTEXT_CONTROL_AMD64, CONTEXT_FULL_AMD64};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32};
use windows_sys::Win32::System::Memory::{
    GetProcessHeap, HeapLock, HeapUnlock, HeapWalk, VirtualAlloc, VirtualProtect, VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_RESERVE,
    PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY, PROCESS_HEAP_ENTRY,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, GetCurrentThreadStackLimits, OpenThread, ResumeThread, SuspendThread, THREAD_GET_CONTEXT,
    THREAD_QUERY_INFORMATION, THREAD_SUSPEND_RESUME,
};

const PAGE: usize = 0x1000;
/// A heap entry that is an allocated block (winnt.h).
const PROCESS_HEAP_ENTRY_BUSY: u32 = 4;
/// The groups the table holds; writes past them are only counted.
const GROUPS: usize = 1 << 17;
/// The calls recorded for a write, innermost first.
const FRAMES: usize = 12;
/// The calls that tell two groups apart.
const KEY_FRAMES: usize = 8;
const THREADS: usize = 1024;
/// The most threads held.
const HELD: usize = 1024;
/// What the arena reserves for the heap's blocks and the protected ranges.
const BLOCKS_RESERVED: usize = 1 << 31;
const RANGES_RESERVED: usize = 1 << 28;
/// The blocks and statics a round's reading calls' table of what they read
/// holds.
const SEEN: usize = 1 << 23;
/// The function whose writes are watched and those of `reads-before`.
const FUNCTIONS: usize = 9;
/// No function is being watched.
const NONE: usize = usize::MAX;
/// What a reading call's report lists of its groups, and a marking call's of
/// those not marked.
const LISTED: usize = 300;

const STATIC: u8 = 0;
const BLOCK: u8 = 1;
const NEW: u8 = 2;

const READ: u8 = 0;
const WRITE: u8 = 1;

#[repr(C)]
struct Group {
    key: AtomicU64,
    count: AtomicU64,
    rip: u64,
    frames: [u64; FRAMES],
    class: u8,
    main: u8,
    kind: u8,
    /// To what a reading call read before.
    seen: u8,
    address: u64,
    block: u64,
    block_size: u64,
}

/// A thread's pages made writable for the instruction it is stepping over.
#[repr(C)]
struct Pending {
    thread: AtomicU32,
    pages: [AtomicUsize; 2],
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Range {
    start: usize,
    end: usize,
    protection: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Block {
    start: usize,
    size: usize,
}

/// Memory outside the heap and the image, so that the handler writes
/// nothing it watches.
struct Arena {
    held: *mut HANDLE,
    groups: *mut Group,
    pending: *mut Pending,
    blocks: *mut Block,
    ranges: *mut Range,
    /// The blocks' starts and the statics' eight bytes the round's reading
    /// calls read or wrote, an open table.
    seen: *mut AtomicU64,
}
unsafe impl Send for Arena {}
unsafe impl Sync for Arena {}

/// The functions watched, in arrays, so that looking them up while a watch
/// is on reads no heap.
struct Watched {
    /// The function whose writes are watched, then those of `reads-before`.
    addresses: [usize; FUNCTIONS],
    rvas: [usize; FUNCTIONS],
    count: usize,
    calls: usize,
    after_days: usize,
}

impl Watched {
    fn reads(&self) -> bool {
        self.count > 1
    }

    fn index(&self, address: usize) -> Option<usize> {
        self.addresses[..self.count].iter().position(|&a| a == address)
    }
}

static WATCHED: OnceLock<Watched> = OnceLock::new();
static ARENA: OnceLock<Arena> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
/// The function being watched, by its index and its address, and how deep
/// its calls of itself are.
static WATCHING: AtomicUsize = AtomicUsize::new(NONE);
static WATCHING_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static DEPTH: AtomicUsize = AtomicUsize::new(0);
/// The `reads-before` functions watched this round, by bit.
static READ_DONE: AtomicU64 = AtomicU64::new(0);
/// Reads are watched too, the pages inaccessible, and what is read kept.
static READS_ON: AtomicBool = AtomicBool::new(false);
/// Writes to what was read before are marked.
static MARK: AtomicBool = AtomicBool::new(false);
static PROTECTION: AtomicU32 = AtomicU32::new(PAGE_READONLY);
static SEEN_COUNT: AtomicUsize = AtomicUsize::new(0);
static SEEN_FULL: AtomicU64 = AtomicU64::new(0);
static MARKED: AtomicU64 = AtomicU64::new(0);
/// The date last seen and the days passed one after another, for
/// `after-days`.
static LAST_DATE: AtomicI64 = AtomicI64::new(i64::MIN);
static DAYS: AtomicUsize = AtomicUsize::new(0);
static BEGUN: AtomicBool = AtomicBool::new(false);
static BLOCKS: AtomicUsize = AtomicUsize::new(0);
static RANGES: AtomicUsize = AtomicUsize::new(0);
static WRITES: AtomicU64 = AtomicU64::new(0);
/// The groups the main thread's reads and writes made: the watched call goes
/// on while new ones come, not while a loop waits, reading the same again.
static MAIN_GROUPS: AtomicU64 = AtomicU64::new(0);
static UNGROUPED: AtomicU64 = AtomicU64::new(0);
static BASE: AtomicUsize = AtomicUsize::new(0);
static IMAGE_END: AtomicUsize = AtomicUsize::new(0);
static MAIN: AtomicU32 = AtomicU32::new(0);
static HELD_COUNT: AtomicUsize = AtomicUsize::new(0);
static OPENED: AtomicUsize = AtomicUsize::new(0);
/// What the watch's start said, for the report.
static STARTED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
/// The watchdog's thread, which is never held.
static WATCHDOG: AtomicU32 = AtomicU32::new(0);
/// The main thread's stack, as the watched call began, for the watchdog.
static MAIN_STACK: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];
/// The watch was given up by the watchdog.
static GIVEN_UP: AtomicBool = AtomicBool::new(false);
/// The main thread's calls when the watchdog gave up, innermost first.
static STUCK: [AtomicU64; FRAMES] = [const { AtomicU64::new(0) }; FRAMES];
/// What `let-run` names: the threads that start in these modules go on.
static LET_RUN: OnceLock<Vec<String>> = OnceLock::new();
/// ntdll's `NtQueryInformationThread`, for where a thread starts.
type QueryThread = unsafe extern "system" fn(HANDLE, u32, *mut std::ffi::c_void, u32, *mut u32) -> i32;
static QUERY_THREAD: OnceLock<Option<QueryThread>> = OnceLock::new();

/// The file name of the module a thread starts in, lowercase, if any.
fn start_module(thread: HANDLE) -> Option<String> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    let query = *QUERY_THREAD.get_or_init(|| {
        let ntdll = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr() as _) };
        let f = unsafe { GetProcAddress(ntdll, c"NtQueryInformationThread".as_ptr() as _) }?;
        Some(unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryThread>(f) })
    });
    let mut start = 0usize;
    // ThreadQuerySetWin32StartAddress
    if unsafe { query?(thread, 9, &mut start as *mut usize as _, 8, std::ptr::null_mut()) } < 0 {
        return None;
    }
    place(start as u64).split('+').next().filter(|m| m.contains('.')).map(str::to_lowercase)
}

/// Opens the process's other threads, into the arena.
fn open_threads(a: &Arena) -> usize {
    let (process, me) = unsafe { (GetCurrentProcessId(), GetCurrentThreadId()) };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return 0;
    }
    let let_run = LET_RUN.get().map_or(&[][..], Vec::as_slice);
    let watchdog = WATCHDOG.load(Ordering::Relaxed);
    let mut n = 0;
    let mut entry = THREADENTRY32 { dwSize: std::mem::size_of::<THREADENTRY32>() as u32, ..Default::default() };
    let mut more = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while more && n < HELD {
        // The threads that write a save's countries and provinces with the
        // main thread go on, and their writes are watched too.
        let writes_saves = crate::saving::parallel::threads().contains(&entry.th32ThreadID);
        if entry.th32OwnerProcessID == process && entry.th32ThreadID != me && entry.th32ThreadID != watchdog && !writes_saves {
            let h = unsafe { OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION, 0, entry.th32ThreadID) };
            // A thread of a module `let-run` names goes on, what it reads and
            // writes recorded as another thread's.
            let runs = !h.is_null() && !let_run.is_empty() && start_module(h).is_some_and(|m| let_run.contains(&m));
            if runs {
                unsafe { CloseHandle(h) };
            } else if !h.is_null() {
                unsafe { *a.held.add(n) = h };
                n += 1;
            }
        }
        more = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    n
}

/// Suspends the threads opened, each stopped once its context can be read.
fn hold(a: &Arena, n: usize) {
    for i in 0..n {
        let h = unsafe { *a.held.add(i) };
        if unsafe { SuspendThread(h) } != u32::MAX {
            let mut context: CONTEXT = unsafe { std::mem::zeroed() };
            context.ContextFlags = CONTEXT_CONTROL_AMD64;
            unsafe { GetThreadContext(h, &mut context) };
        }
    }
    HELD_COUNT.store(n, Ordering::Release);
}

/// Lets the held threads go on, and closes every thread opened.
fn let_go(a: &Arena, opened: usize) {
    let held = HELD_COUNT.swap(0, Ordering::AcqRel);
    for i in 0..opened.max(held) {
        let h = unsafe { *a.held.add(i) };
        unsafe {
            if i < held {
                ResumeThread(h);
            }
            CloseHandle(h);
        }
    }
}

fn reserve(bytes: usize, commit: bool) -> *mut u8 {
    let kind = if commit { MEM_RESERVE | MEM_COMMIT } else { MEM_RESERVE };
    unsafe { VirtualAlloc(std::ptr::null(), bytes, kind, PAGE_READWRITE) as *mut u8 }
}

/// Commits the arena's pages up to `bytes` past `start`, as a list grows.
fn commit(start: *mut u8, bytes: usize) -> bool {
    !unsafe { VirtualAlloc(start as _, bytes.max(1), MEM_COMMIT, PAGE_READWRITE) }.is_null()
}

fn arena() -> Option<&'static Arena> {
    if let Some(a) = ARENA.get() {
        return Some(a);
    }
    let held = reserve(HELD * std::mem::size_of::<HANDLE>(), true) as *mut HANDLE;
    let groups = reserve(GROUPS * std::mem::size_of::<Group>(), true) as *mut Group;
    let pending = reserve(THREADS * std::mem::size_of::<Pending>(), true) as *mut Pending;
    let blocks = reserve(BLOCKS_RESERVED, false) as *mut Block;
    let ranges = reserve(RANGES_RESERVED, false) as *mut Range;
    let seen = reserve(SEEN * std::mem::size_of::<AtomicU64>(), true) as *mut AtomicU64;
    if held.is_null() || groups.is_null() || pending.is_null() || blocks.is_null() || ranges.is_null() || seen.is_null() {
        return None;
    }
    let _ = ARENA.set(Arena { held, groups, pending, blocks, ranges, seen });
    ARENA.get()
}

fn ranges(a: &Arena) -> &'static [Range] {
    unsafe { std::slice::from_raw_parts(a.ranges, RANGES.load(Ordering::Acquire)) }
}

fn blocks(a: &Arena) -> &'static [Block] {
    unsafe { std::slice::from_raw_parts(a.blocks, BLOCKS.load(Ordering::Acquire)) }
}

/// The protected range holding `address`.
fn range_of(a: &Arena, address: usize) -> Option<Range> {
    let r = ranges(a);
    let i = r.partition_point(|x| x.end <= address);
    r.get(i).filter(|x| x.start <= address).copied()
}

/// The block that held `address` when the call began.
fn block_of(a: &Arena, address: usize) -> Option<Block> {
    let b = blocks(a);
    let i = b.partition_point(|x| x.start <= address);
    i.checked_sub(1).map(|i| b[i]).filter(|x| address < x.start + x.size)
}

/// Every busy block of the process heap, sorted, into the arena; the heap
/// is locked meanwhile, so that it does not change while it is walked.
fn take_blocks(a: &Arena) -> Result<usize, String> {
    let heap = unsafe { GetProcessHeap() };
    let mut n = 0usize;
    let mut committed = 0usize;
    let mut entry: PROCESS_HEAP_ENTRY = unsafe { std::mem::zeroed() };
    while unsafe { HeapWalk(heap, &mut entry) } != 0 {
        if entry.wFlags as u32 & PROCESS_HEAP_ENTRY_BUSY == 0 {
            continue;
        }
        let need = (n + 1) * std::mem::size_of::<Block>();
        if need > committed {
            if need > BLOCKS_RESERVED || !commit(a.blocks as *mut u8, committed + (64 << 20)) {
                return Err(format!("more than {n} blocks"));
            }
            committed = (committed + (64 << 20)).min(BLOCKS_RESERVED);
        }
        unsafe { *a.blocks.add(n) = Block { start: entry.lpData as usize, size: entry.cbData as usize } };
        n += 1;
    }
    let all = unsafe { std::slice::from_raw_parts_mut(a.blocks, n) };
    all.sort_unstable_by_key(|b| b.start);
    Ok(n)
}

/// The ranges being made, in the arena.
struct Ranges<'a> {
    arena: &'a Arena,
    n: usize,
    committed: usize,
}

impl Ranges<'_> {
    fn push(&mut self, start: usize, end: usize, protection: u32) -> Result<(), String> {
        if start >= end {
            return Ok(());
        }
        let need = (self.n + 1) * std::mem::size_of::<Range>();
        if need > self.committed {
            if need > RANGES_RESERVED || !commit(self.arena.ranges as *mut u8, self.committed + (1 << 20)) {
                return Err(format!("more than {} ranges", self.n));
            }
            self.committed += 1 << 20;
        }
        let previous = (self.n > 0).then(|| unsafe { &mut *self.arena.ranges.add(self.n - 1) });
        match previous {
            Some(p) if p.end == start && p.protection == protection => p.end = end,
            _ => {
                unsafe { *self.arena.ranges.add(self.n) = Range { start, end, protection } };
                self.n += 1;
            }
        }
        Ok(())
    }
}

/// The pages to make read-only: those of the blocks and the image's
/// writable sections, merged, and of them the committed ones that are
/// writable now, each with its protection; but not the `kept` pages, sorted,
/// which the exception's own dispatch writes to.
fn take_ranges(a: &Arena, m: &Memory, kept: &[usize]) -> Result<usize, String> {
    let mut pages: Vec<(usize, usize)> = Vec::new();
    let image = unsafe { analysis::loaded(m.base) };
    let pe = u32::from_le_bytes(image[0x3c..0x40].try_into().unwrap()) as usize;
    let sections = u16::from_le_bytes(image[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let table = pe + 24 + u16::from_le_bytes(image[pe + 20..pe + 22].try_into().unwrap()) as usize;
    for s in 0..sections {
        let at = table + 40 * s;
        let size = u32::from_le_bytes(image[at + 8..at + 12].try_into().unwrap()) as usize;
        let rva = u32::from_le_bytes(image[at + 12..at + 16].try_into().unwrap()) as usize;
        let flags = u32::from_le_bytes(image[at + 36..at + 40].try_into().unwrap());
        // IMAGE_SCN_MEM_WRITE
        if flags & 0x8000_0000 != 0 {
            pages.push((m.base + rva, (m.base + rva + size).next_multiple_of(PAGE)));
        }
    }
    let mut last: Option<(usize, usize)> = None;
    for b in blocks(a) {
        let (start, end) = (b.start & !(PAGE - 1), (b.start + b.size).next_multiple_of(PAGE));
        last = match last {
            Some((s, e)) if start <= e => Some((s, e.max(end))),
            Some(done) => {
                pages.push(done);
                Some((start, end))
            }
            None => Some((start, end)),
        };
    }
    pages.extend(last);
    pages.sort_unstable();
    let mut ranges = Ranges { arena: a, n: 0, committed: 0 };
    for (start, end) in pages {
        let mut at = start;
        while at < end {
            let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { VirtualQuery(at as _, &mut info, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
                break;
            }
            let region_end = (info.BaseAddress as usize + info.RegionSize).min(end);
            let writable = info.Protect == PAGE_READWRITE || info.Protect == PAGE_WRITECOPY;
            if info.State == MEM_COMMIT && writable {
                let mut from = at;
                for &page in kept.iter().filter(|&&p| (at..region_end).contains(&p)) {
                    ranges.push(from, page, info.Protect)?;
                    from = page + PAGE;
                }
                ranges.push(from, region_end, info.Protect)?;
            }
            at = region_end;
        }
    }
    Ok(ranges.n)
}

fn pending(a: &Arena, thread: u32) -> Option<&'static Pending> {
    let start = thread as usize % THREADS;
    for i in 0..THREADS {
        let p = unsafe { &*a.pending.add((start + i) % THREADS) };
        match p.thread.compare_exchange(0, thread, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Some(p),
            Err(t) if t == thread => return Some(p),
            Err(_) => {}
        }
    }
    None
}

fn protect(page: usize, protection: u32) {
    let mut old = 0u32;
    unsafe { VirtualProtect(page as _, PAGE, protection, &mut old) };
}

/// The return addresses of the calls `context` is in, innermost first.
fn frames(context: &CONTEXT) -> [u64; FRAMES] {
    let (mut low, mut high) = (0usize, 0usize);
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    frames_in(context, low, high)
}

/// [`frames`] of a thread whose stack lies from `low` to `high`.
fn frames_in(context: &CONTEXT, low: usize, high: usize) -> [u64; FRAMES] {
    let mut out = [0u64; FRAMES];
    let mut c = *context;
    for slot in out.iter_mut() {
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

fn seen_slot(key: u64, i: usize) -> usize {
    ((key.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40) as usize + i) % SEEN
}

/// Keeps `key` in the table of what was read.
fn see(a: &Arena, key: u64) {
    for i in 0..64 {
        let slot = unsafe { &*a.seen.add(seen_slot(key, i)) };
        match slot.compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                SEEN_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Err(k) if k == key => return,
            Err(_) => {}
        }
    }
    SEEN_FULL.fetch_add(1, Ordering::Relaxed);
}

fn was_seen(a: &Arena, key: u64) -> bool {
    for i in 0..64 {
        match unsafe { &*a.seen.add(seen_slot(key, i)) }.load(Ordering::Acquire) {
            0 => return false,
            k if k == key => return true,
            _ => {}
        }
    }
    false
}

fn record(a: &Arena, context: &CONTEXT, address: usize, thread: u32, kind: u8) {
    WRITES.fetch_add(1, Ordering::Relaxed);
    let base = BASE.load(Ordering::Relaxed);
    let (class, block) = if (base..IMAGE_END.load(Ordering::Relaxed)).contains(&address) {
        (STATIC, None)
    } else {
        match block_of(a, address) {
            Some(b) => (BLOCK, Some(b)),
            None => (NEW, None),
        }
    };
    // What a reading call keeps of what it touched, and a marking call
    // looks for: a block by its start, `eu4.exe`'s data by its eight bytes.
    let what = match block {
        Some(b) => b.start as u64,
        None if class == STATIC => address as u64 & !7,
        None => 0,
    };
    let mut seen = 0u8;
    if what != 0 {
        if READS_ON.load(Ordering::Relaxed) {
            see(a, what);
        } else if MARK.load(Ordering::Relaxed) && was_seen(a, what) {
            seen = 1;
            MARKED.fetch_add(1, Ordering::Relaxed);
        }
    }
    let main = (thread == MAIN.load(Ordering::Relaxed)) as u8;
    let frames = frames(context);
    // FNV-1a over the instruction, the calls, the class, the thread, the
    // kind and the mark.
    let mut key = 0xcbf2_9ce4_8422_2325u64;
    for v in [context.Rip, class as u64, main as u64, kind as u64, seen as u64].into_iter().chain(frames[..KEY_FRAMES].iter().copied()) {
        key = (key ^ v).wrapping_mul(0x100_0000_01b3);
    }
    let key = key.max(1);
    let start = key as usize % GROUPS;
    for i in 0..64 {
        let g = unsafe { &mut *a.groups.add((start + i) % GROUPS) };
        match g.key.compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                if main == 1 {
                    MAIN_GROUPS.fetch_add(1, Ordering::Relaxed);
                }
                g.rip = context.Rip;
                g.frames = frames;
                g.class = class;
                g.main = main;
                g.kind = kind;
                g.seen = seen;
                g.address = address as u64;
                g.block = block.map_or(0, |b| b.start as u64);
                g.block_size = block.map_or(0, |b| b.size as u64);
                g.count.fetch_add(1, Ordering::Release);
                return;
            }
            Err(k) if k == key => {
                g.count.fetch_add(1, Ordering::Relaxed);
                return;
            }
            Err(_) => {}
        }
    }
    UNGROUPED.fetch_add(1, Ordering::Relaxed);
}

const CONTINUE_EXECUTION: i32 = -1;
const CONTINUE_SEARCH: i32 = 0;
const TRAP_FLAG: u32 = 0x100;

#[called_by_game]
unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let Some(a) = ARENA.get() else { return CONTINUE_SEARCH };
    let (record_, context) = (&*(*info).ExceptionRecord, &mut *(*info).ContextRecord);
    let thread = GetCurrentThreadId();
    match record_.ExceptionCode {
        // A write, or a read while reads are watched: the pages are
        // inaccessible only then.
        EXCEPTION_ACCESS_VIOLATION
            if record_.NumberParameters >= 2
                && (record_.ExceptionInformation[0] == 1 || record_.ExceptionInformation[0] == 0 && READS_ON.load(Ordering::Acquire)) =>
        {
            let kind = if record_.ExceptionInformation[0] == 1 { WRITE } else { READ };
            let address = record_.ExceptionInformation[1];
            let Some(range) = range_of(a, address) else { return CONTINUE_SEARCH };
            let page = address & !(PAGE - 1);
            protect(page, range.protection);
            if !ACTIVE.load(Ordering::Acquire) {
                // The watch ended while this write was on its way.
                return CONTINUE_EXECUTION;
            }
            record(a, context, address, thread, kind);
            if let Some(p) = pending(a, thread) {
                let slot = if p.pages[0].load(Ordering::Relaxed) == 0 { 0 } else { 1 };
                p.pages[slot].store(page, Ordering::Relaxed);
            }
            context.EFlags |= TRAP_FLAG;
            CONTINUE_EXECUTION
        }
        EXCEPTION_SINGLE_STEP => {
            let Some(p) = pending(a, thread) else { return CONTINUE_SEARCH };
            let pages = [p.pages[0].swap(0, Ordering::Relaxed), p.pages[1].swap(0, Ordering::Relaxed)];
            if pages == [0, 0] {
                return CONTINUE_SEARCH;
            }
            if ACTIVE.load(Ordering::Acquire) {
                for page in pages.into_iter().filter(|&p| p != 0) {
                    protect(page, PROTECTION.load(Ordering::Relaxed));
                }
            }
            context.EFlags &= !TRAP_FLAG;
            CONTINUE_EXECUTION
        }
        _ => CONTINUE_SEARCH,
    }
}

/// Watches the call on its way: its reads too when `reads`, into the table
/// of what was read, emptied first at the round's first reading call; or
/// its writes, those to what the round's reading calls read marked.
fn start(m: &Memory, reads: bool, first_read: bool) -> Result<String, String> {
    let a = arena().ok_or("no memory for the arena")?;
    static HANDLER: OnceLock<usize> = OnceLock::new();
    let entry = *HANDLER.get_or_init(|| unsafe { AddVectoredExceptionHandler(1, Some(handler)) } as usize);
    // The handler's entry in the process's list is a heap block, whose count
    // of the calls under way the dispatch of every exception writes, holding
    // the list's lock: read-only, the first exception waits on itself. The
    // count lies in the entry or, in some Windows versions, in a block of its
    // own the entry points to at +0x10.
    let mut kept = vec![entry & !(PAGE - 1), (entry + 0x30) & !(PAGE - 1)];
    if let Ok(count) = crate::memory().ok_or(()).and_then(|m| m.ptr(entry + 0x10).map_err(|_| ())) {
        if count != 0 {
            kept.extend([count & !(PAGE - 1), (count + 8) & !(PAGE - 1)]);
        }
    }
    kept.sort_unstable();
    kept.dedup();
    unsafe { std::ptr::write_bytes(a.groups, 0, GROUPS) };
    if first_read {
        unsafe { std::ptr::write_bytes(a.seen, 0, SEEN) };
        SEEN_COUNT.store(0, Ordering::Relaxed);
        SEEN_FULL.store(0, Ordering::Relaxed);
    }
    let protection = if reads { PAGE_NOACCESS } else { PAGE_READONLY };
    PROTECTION.store(protection, Ordering::Relaxed);
    READS_ON.store(reads, Ordering::Release);
    MARK.store(!reads && SEEN_COUNT.load(Ordering::Relaxed) > 0, Ordering::Release);
    MARKED.store(0, Ordering::Relaxed);
    WRITES.store(0, Ordering::Relaxed);
    UNGROUPED.store(0, Ordering::Relaxed);
    BASE.store(m.base, Ordering::Relaxed);
    IMAGE_END.store(m.base + unsafe { analysis::loaded(m.base) }.len(), Ordering::Relaxed);
    MAIN.store(crate::main_thread(), Ordering::Relaxed);
    let (mut low, mut high) = (0usize, 0usize);
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    MAIN_STACK[0].store(low, Ordering::Relaxed);
    MAIN_STACK[1].store(high, Ordering::Relaxed);
    GIVEN_UP.store(false, Ordering::Relaxed);
    let began = std::time::Instant::now();
    let threads = open_threads(a);
    OPENED.store(threads, Ordering::Release);
    let heap = unsafe { GetProcessHeap() };
    // The heap stays locked until the other threads are held, so that none
    // makes a block after the walk, and none is held with the heap's lock;
    // the lists are in the arena, outside it.
    unsafe { HeapLock(heap) };
    let taken = take_blocks(a);
    BLOCKS.store(*taken.as_ref().unwrap_or(&0), Ordering::Release);
    let ranges = taken.and_then(|_| take_ranges(a, m, &kept));
    if let Ok(n) = ranges {
        hold(a, threads);
        RANGES.store(n, Ordering::Release);
        ACTIVE.store(true, Ordering::Release);
        for r in self::ranges(a) {
            let mut old = 0u32;
            unsafe { VirtualProtect(r.start as _, r.end - r.start, protection, &mut old) };
        }
    }
    unsafe { HeapUnlock(heap) };
    let n = ranges?;
    let bytes: usize = self::ranges(a).iter().map(|r| r.end - r.start).sum();
    Ok(format!(
        "{threads} other threads held, {} blocks, {n} ranges of {} MB {} in {} ms",
        BLOCKS.load(Ordering::Relaxed),
        bytes >> 20,
        if reads { "inaccessible" } else { "read-only" },
        began.elapsed().as_millis()
    ))
}

fn stop() {
    let Some(a) = ARENA.get() else { return };
    ACTIVE.store(false, Ordering::Release);
    for r in ranges(a) {
        let mut old = 0u32;
        unsafe { VirtualProtect(r.start as _, r.end - r.start, r.protection, &mut old) };
    }
    let_go(a, OPENED.swap(0, Ordering::AcqRel));
}

/// Ends a watch that has seen nothing new of the main thread for `seconds`: it
/// then waits on a thread held, or on a lock one has, maybe in a loop. Where it waits is kept for
/// the report, and the threads go on.
fn watchdog(seconds: u64) {
    WATCHDOG.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
    let (mut last, mut quiet) = (u64::MAX, 0u64);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !ACTIVE.load(Ordering::Acquire) {
            (last, quiet) = (u64::MAX, 0);
            continue;
        }
        let seen = MAIN_GROUPS.load(Ordering::Relaxed);
        if seen != last {
            (last, quiet) = (seen, 0);
            continue;
        }
        quiet += 500;
        if quiet < seconds * 1000 {
            continue;
        }
        // Nothing here reads the heap until the watch has ended.
        let main = unsafe { OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT, 0, MAIN.load(Ordering::Relaxed)) };
        if !main.is_null() {
            if unsafe { SuspendThread(main) } != u32::MAX {
                let mut context: CONTEXT = unsafe { std::mem::zeroed() };
                context.ContextFlags = CONTEXT_FULL_AMD64;
                if unsafe { GetThreadContext(main, &mut context) } != 0 {
                    let calls = frames_in(&context, MAIN_STACK[0].load(Ordering::Relaxed), MAIN_STACK[1].load(Ordering::Relaxed));
                    for (slot, f) in STUCK.iter().zip(std::iter::once(context.Rip).chain(calls)) {
                        slot.store(f, Ordering::Relaxed);
                    }
                }
                unsafe { ResumeThread(main) };
            }
            unsafe { CloseHandle(main) };
        }
        GIVEN_UP.store(true, Ordering::Release);
        stop();
        let calls: Vec<String> = STUCK.iter().map(|f| f.load(Ordering::Relaxed)).take_while(|&f| f != 0).map(place).collect();
        log!("write watch: given up after {seconds} s with nothing watched, the main thread at {}", calls.join(" < "));
        (last, quiet) = (u64::MAX, 0);
    }
}

/// `eu4.exe` RVAs as such, other code as its module and offset.
fn place(address: u64) -> String {
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    let base = BASE.load(Ordering::Relaxed) as u64;
    if (base..IMAGE_END.load(Ordering::Relaxed) as u64).contains(&address) {
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

fn report(rva: usize, call: usize, reads: bool) {
    let Some(a) = ARENA.get() else { return };
    let mut groups: Vec<&Group> = (0..GROUPS).map(|i| unsafe { &*a.groups.add(i) }).filter(|g| g.key.load(Ordering::Acquire) != 0).collect();
    groups.sort_unstable_by_key(|g| (std::cmp::Reverse(g.seen), std::cmp::Reverse(g.count.load(Ordering::Relaxed))));
    let marking = MARK.load(Ordering::Relaxed);
    log!("write watch {rva:#x} #{call}: {}", STARTED.lock().unwrap_or_else(|e| e.into_inner()));
    log!(
        "write watch {rva:#x} #{call}: {} {}{} in {} groups ({} not grouped)",
        WRITES.load(Ordering::Relaxed),
        if reads { "reads and writes" } else { "writes" },
        if GIVEN_UP.load(Ordering::Acquire) { " until the watch was given up" } else { "" },
        groups.len(),
        UNGROUPED.load(Ordering::Relaxed)
    );
    if reads {
        // What was read, by the size of its block: the kind of object it is.
        let mut sizes = std::collections::BTreeMap::<usize, usize>::new();
        let mut statics = 0usize;
        for i in 0..SEEN {
            let key = unsafe { &*a.seen.add(i) }.load(Ordering::Relaxed) as usize;
            if key == 0 {
                continue;
            }
            match block_of(a, key) {
                Some(b) if b.start == key => *sizes.entry(b.size).or_default() += 1,
                _ => statics += 1,
            }
        }
        log!(
            "write watch {rva:#x} #{call}: what the round read so far: {} blocks of {} sizes, {statics} statics ({} not kept)",
            sizes.values().sum::<usize>(),
            sizes.len(),
            SEEN_FULL.load(Ordering::Relaxed)
        );
        let mut by_count: Vec<(usize, usize)> = sizes.into_iter().collect();
        by_count.sort_unstable_by_key(|&(size, n)| (std::cmp::Reverse(n), size));
        for (size, n) in by_count.into_iter().take(60) {
            log!("write watch {rva:#x} #{call}: read {n} blocks of {size:#x} bytes");
        }
    }
    if marking {
        log!(
            "write watch {rva:#x} #{call}: {} writes to what the reading calls read before, in {} groups",
            MARKED.load(Ordering::Relaxed),
            groups.iter().filter(|g| g.seen == 1).count()
        );
    }
    // Everything a plain watch wrote; else the marked groups and the largest
    // others.
    let listed = groups.iter().filter(|g| g.seen == 1).count() + if reads || marking { LISTED } else { groups.len() };
    for g in groups.into_iter().take(listed) {
        let class = ["static", "block", "new"][g.class as usize];
        let thread = if g.main == 1 { "main" } else { "other" };
        let kind = if g.kind == READ { "read" } else { "write" };
        let calls: Vec<String> = g.frames.iter().take_while(|&&f| f != 0).map(|&f| place(f)).collect();
        let target = match g.class {
            STATIC => place(g.address),
            BLOCK => format!("{:#x} bytes +{:#x}", g.block_size, g.address - g.block),
            _ => format!("{:#x}", g.address),
        };
        let seen = if g.seen == 1 { " (read before)" } else { "" };
        log!("write watch #{call}: {} {thread} {kind} {class} at {} to {target}{seen} in {}", g.count.load(Ordering::Relaxed), place(g.rip), calls.join(" < "));
    }
}

/// Whether the watch has begun: at once, or once `after-days` game days
/// have passed one after another, as the date is at the watched functions'
/// calls.
fn begun(w: &Watched, m: &Memory) -> bool {
    if w.after_days == 0 || BEGUN.load(Ordering::Relaxed) {
        return true;
    }
    let date = m.ptr(m.at(campaign::globals::WORLD)).and_then(|world| match world {
        0 => Err("no world".into()),
        world => m.i32(world + campaign::world::DATE),
    });
    let Ok(date) = date else { return false };
    let date = date as i64;
    let last = LAST_DATE.swap(date, Ordering::Relaxed);
    // A date is hours; a step of more than a month is a load, not days.
    match date - last {
        0 => {}
        step if last != i64::MIN && step > 0 && step <= 31 * 24 => {
            DAYS.fetch_add(step as usize / 24, Ordering::Relaxed);
        }
        _ => DAYS.store(0, Ordering::Relaxed),
    }
    let begun = DAYS.load(Ordering::Relaxed) >= w.after_days;
    if begun {
        BEGUN.store(true, Ordering::Relaxed);
        log!("write watch: begins after {} game days, at the date {date:#x}", w.after_days);
    }
    begun
}

fn before(call: &mut Call) {
    // While a watch is on, a call of the watched function within itself only
    // counts; the observed function is looked up in the hooks' registry, a
    // read of the heap the watch may record.
    if WATCHING.load(Ordering::SeqCst) != NONE {
        if crate::thread_id() == crate::main_thread() && call.observed() == Some(WATCHING_ADDRESS.load(Ordering::SeqCst)) {
            DEPTH.fetch_add(1, Ordering::SeqCst);
        }
        return;
    }
    let Some(w) = WATCHED.get() else { return };
    let Some(address) = call.observed() else { return };
    let Some(index) = w.index(address) else { return };
    let m = call.memory();
    if crate::thread_id() != crate::main_thread() || !begun(w, &m) || CALLS.load(Ordering::SeqCst) >= w.calls {
        return;
    }
    let done = READ_DONE.load(Ordering::SeqCst);
    let reads = index > 0;
    if reads && done & (1 << (index - 1)) != 0 {
        return;
    }
    // The watched call waits for something read before it in its round.
    if !reads && w.reads() && done == 0 {
        return;
    }
    WATCHING.store(index, Ordering::SeqCst);
    WATCHING_ADDRESS.store(address, Ordering::SeqCst);
    DEPTH.store(0, Ordering::SeqCst);
    match start(&m, reads, reads && done == 0) {
        // Logged with the report: the log's lock may be a held thread's.
        Ok(what) => *STARTED.lock().unwrap_or_else(|e| e.into_inner()) = what,
        Err(e) => {
            stop();
            WATCHING.store(NONE, Ordering::SeqCst);
            log!("write watch {:#x} #{}: not watched: {e}", w.rvas[index], CALLS.load(Ordering::SeqCst));
        }
    }
}

fn after(call: &mut Call) {
    // Before anything else, so that the framework's own work here is not
    // watched; a return from within the watched call only counts.
    let index = WATCHING.load(Ordering::SeqCst);
    if index == NONE || crate::thread_id() != crate::main_thread() || call.observed() != Some(WATCHING_ADDRESS.load(Ordering::SeqCst)) {
        return;
    }
    if DEPTH.load(Ordering::SeqCst) > 0 {
        DEPTH.fetch_sub(1, Ordering::SeqCst);
        return;
    }
    stop();
    WATCHING.store(NONE, Ordering::SeqCst);
    let Some(w) = WATCHED.get() else { return };
    let number = CALLS.load(Ordering::SeqCst);
    report(w.rvas[index], number, index > 0);
    if index > 0 {
        READ_DONE.fetch_or(1 << (index - 1), Ordering::SeqCst);
    } else {
        READ_DONE.store(0, Ordering::SeqCst);
        CALLS.fetch_add(1, Ordering::SeqCst);
    }
}

/// A function to observe: its RVA, its argument count and the bytes it holds
/// now.
type Function = (usize, u32, &'static str);

/// The functions dev.toml names, chosen to be watched: what the watch keeps
/// of them, and each one to observe.
pub struct Chosen(Option<(Watched, Vec<Function>)>);

/// The functions dev.toml names, when they can be watched.
pub fn chosen(m: &Memory) -> Chosen {
    let options = &crate::dev::options().write_watch;
    if options.function.is_empty() {
        return Chosen(None);
    }
    let image = unsafe { analysis::loaded(m.base) };
    let names: Vec<&str> = std::iter::once(options.function.as_str()).chain(options.reads_before.iter().map(String::as_str)).collect();
    if names.len() > FUNCTIONS {
        log!("write watch: {} functions, more than {FUNCTIONS}", names.len());
        return Chosen(None);
    }
    let mut watched = Watched { addresses: [0; FUNCTIONS], rvas: [0; FUNCTIONS], count: 0, calls: options.calls, after_days: options.after_days };
    let mut functions = Vec::new();
    for name in names {
        let Some(rva) = usize::from_str_radix(name.trim().trim_start_matches("0x"), 16).ok().filter(|&r| r < image.len()) else {
            log!("write watch: {name:?} is not an RVA in the game");
            return Chosen(None);
        };
        let Some(argc) = analysis::bounds(image, rva).map(|range| analysis::arguments(&image[range], rva)) else {
            log!("write watch: {rva:#x} starts no function in the exception directory");
            return Chosen(None);
        };
        if watched.index(m.base + rva).is_some() {
            log!("write watch: {rva:#x} is named twice");
            return Chosen(None);
        }
        let what = if watched.count == 0 { "writes" } else { "reads, before each watched call" };
        log!("write watch: {rva:#x}, {argc} argument(s), its {what}, the first {} round(s) on the main thread", options.calls);
        watched.addresses[watched.count] = m.base + rva;
        watched.rvas[watched.count] = rva;
        watched.count += 1;
        // The bytes there now: the watch checks nothing about the game.
        let bytes: Vec<String> = image[rva..rva + 8].iter().map(|b| format!("{b:02x}")).collect();
        functions.push((rva, argc, &*bytes.join(" ").leak()));
    }
    Chosen(Some((watched, functions)))
}

impl Chosen {
    /// Their observe points.
    pub fn points(&self) -> Vec<Point> {
        let functions = self.0.iter().flat_map(|(_, functions)| functions);
        functions.map(|&(rva, argc, bytes)| Point::observe(rva, bytes, "watched function", argc.min(12), Some(before), Some(after))).collect()
    }

    /// Watches them, with the watchdog `give-up-after` asks for.
    pub fn start(self) {
        let Some((watched, _)) = self.0 else { return };
        if WATCHED.set(watched).is_err() {
            return;
        }
        let options = &crate::dev::options().write_watch;
        let _ = LET_RUN.set(options.let_run.iter().map(|m| m.to_lowercase()).collect());
        if options.give_up_after > 0 {
            let seconds = options.give_up_after;
            let spawned = std::thread::Builder::new().name("write watch".into()).spawn(move || watchdog(seconds));
            if let Err(e) = spawned {
                log!("write watch: no watchdog: {e}");
            }
        }
    }
}
