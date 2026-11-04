//! The countries and the provinces of a save written on several threads at
//! once (docs/game-speed.md, S8).
//!
//! Writing the game state into memory is most of a save's stop, and the
//! countries' loop and the provinces' loop are most of that
//! (docs/research/main-thread.md). The world does not change while it is
//! written, and what the loops write to memory that was there before was
//! watched (docs/research/saves.md, "What writing the game state writes"):
//! besides the output, only two static objects that hold the value being
//! written, `0x15bf70`'s (with five copies of its code inline) and
//! `0x2c59b0`'s, are written by every item.
//!
//! So at the start of each loop the framework writes the list itself, in
//! runs of items taken in turn by the threads of a pool and the main thread.
//! Each run has a writer of its own, a copy of the game's at its depth and in
//! its form, writing into a memory stream of the game's class over a buffer of
//! the framework's. On a thread of the pool the two static objects are copies
//! of that thread's own; on the main thread they are the game's, as always.
//! The runs' bytes then go into the game's stream in the list's order, and the
//! game goes on after the loop: what the stream holds is what the game's loop
//! would have written, and so is the save checksum taken over it afterwards.
//!
//! When anything is not as expected (another class of stream or writer, a
//! fault in a run), nothing is written and the game's own loop runs.
use super::{GameToken, EQUALS, NUMBER, WRITE_TOKEN};
use crate::memory::Memory;
use crate::points::{Function, Point};
use crate::Result;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// In the world's write `0x74ff30(world in r14, writer in rsi)`: where the
/// provinces' loop begins (after `provinces={`), where its items begin, and
/// where it ends (the block's `}`); the same for the countries'.
const PROVINCES: usize = 0x75163a;
const PROVINCES_ITEMS: usize = 0x751644;
const PROVINCES_END: usize = 0x75172f;
const COUNTRIES: usize = 0x75174c;
const COUNTRIES_ITEMS: usize = 0x751752;
const COUNTRIES_END: usize = 0x7517b4;

/// The world's provinces: an array of objects of `PROVINCE_SIZE` bytes at
/// `[world + PROVINCE_ARRAY]`, as many as the pointers from
/// `[world + PROVINCE_FIRST]` to `[world + PROVINCE_LAST]`; each has its id at
/// `+PROVINCE_ID` (0 for none) and writes itself with its method `+8`.
const PROVINCE_FIRST: usize = 0x1c90;
const PROVINCE_LAST: usize = 0x1c98;
const PROVINCE_ARRAY: usize = 0x1ca8;
const PROVINCE_SIZE: usize = 0x2e10;
const PROVINCE_ID: usize = 0x20;
const WRITE_ITSELF: usize = 0x8;
/// The world's countries: as many as its method `+0x40` says, pointers at
/// `[world + COUNTRY_ARRAY]`; each has its tag at `+COUNTRY_TAG`.
const COUNTRY_COUNT: usize = 0x40;
const COUNTRY_ARRAY: usize = 0x1ce8;
const COUNTRY_TAG: usize = 0x20;

/// `(string, &tag)`: a tag as a `std::string`; `(string)` lets one go;
/// `(writer, &key, country)`: a country under its tag.
const TAG_STRING: usize = 0x95240;
const DROP_STRING: usize = 0x95660;
const WRITE_COUNTRY: usize = 0x1707a20;

/// The writer's texts of its token types, made the first time one is needed:
/// `[TOKEN_TEXTS]` an array of `std::string`s, `[TOKEN_TEXTS_COUNT]` how many,
/// `[TOKEN_TEXTS_MADE]` whether they are; `MAKE_TOKEN_TEXTS` makes them.
const TOKEN_TEXTS: usize = 0x242b888;
const TOKEN_TEXTS_COUNT: usize = 0x2357b28;
const TOKEN_TEXTS_MADE: usize = 0x2357b30;
const MAKE_TOKEN_TEXTS: usize = 0x170d250;

/// The save's writer (made on `0x14e3f00`'s stack): its vtable, its stream,
/// and its size.
const WRITER_VTABLE: usize = 0x1dbf7d8;
const WRITER_STREAM: usize = 0x10;
const WRITER_SIZE: usize = 0x20;
const WRITER_DEPTH: usize = super::WRITER_DEPTH;

/// The game's memory stream (vtables `0x1d92200` and `0x1dc0358`, whose write
/// is `0x171f290`): with no growing part (`+0x38` null) it writes into the
/// buffer at `+0x40` of `+0x50` bytes, up to `+0x4c`, keeping `+0x48`, and
/// sets `+0x54` instead of writing past its end. `+0x55` and `+0x56` say
/// whether it lets its buffers go.
const MEMORY_STREAM: [usize; 2] = [0x1d92200, 0x1dc0358];
const STREAM_WRITE_SLOT: usize = 0x50;
const STREAM_WRITE: usize = 0x171f290;
const STREAM_MADE: usize = 0x171ec8f;
const STREAM_GROWING: usize = 0x38;
const STREAM_BUFFER: usize = 0x40;
const STREAM_KEPT: usize = 0x48;
const STREAM_AT: usize = 0x4c;
const STREAM_CAPACITY: usize = 0x50;
const STREAM_FULL: usize = 0x54;
const STREAM_OWNS_BUFFER: usize = 0x55;
const STREAM_OWNS_GROWING: usize = 0x56;
const STREAM_SIZE: usize = 0x58;

/// The two functions that hold the value being written in a static object
/// and return it (`(value) -> object`; the object's methods write the value):
/// `0x15bf70`'s object at `0x2458bf0`, which has the value at `+0x10` and a
/// byte at `+0x18` it clears, and `0x2c59b0`'s at `0x2458e80`, with the value
/// at `+0x10`. Each is a function-local static, made once behind its guard.
const VALUE_HOLDER: usize = 0x15bf70;
const VALUE_HOLDER_STORES: usize = 0x15bf8d;
const VALUE_OBJECT: usize = 0x2458bf0;
const VALUE_OBJECT_VALUE: usize = 0x2458c00;
const VALUE_OBJECT_BYTE: usize = 0x2458c08;
const VALUE_OBJECT_GUARD: usize = 0x2458c10;
const OTHER_HOLDER: usize = 0x2c59b0;
const OTHER_HOLDER_STORES: usize = 0x2c59cd;
const OTHER_OBJECT: usize = 0x2458e80;
const OTHER_OBJECT_VALUE: usize = 0x2458e90;
const OTHER_OBJECT_GUARD: usize = 0x2458e98;
const HOLDER_VALUE: usize = 0x10;
const HOLDER_BYTE: usize = 0x18;
const HOLDER_SIZE: usize = 0x20;

/// The most threads that write at once, the main thread one of them.
const MOST_THREADS: usize = 8;
/// Runs a thread: the list is cut into this many runs a thread, which the
/// threads take in turn, so that one slow run holds up little.
const RUNS_A_THREAD: usize = 8;
/// Fewer items than this are written by the game's own loop.
const FEWEST_ITEMS: usize = 64;
/// A run's first buffer when no earlier save tells how much a run writes.
const FIRST_BUFFER: usize = 1 << 20;
/// A run that would need more than this is not written in parallel.
const MOST_BUFFER: usize = 1 << 30;
/// How long the main thread waits for the pool before it writes the loop
/// itself.
const WAIT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Loop {
    Provinces,
    Countries,
}

impl Loop {
    fn name(self) -> &'static str {
        match self {
            Loop::Provinces => "provinces",
            Loop::Countries => "countries",
        }
    }
}

/// What a run writes, by loop, in the last save: the next one's first guess.
static LAST_RUN: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

// --- The two static objects, a copy for each thread of the pool ---

thread_local! {
    /// On a thread of the pool, its copies of the two objects, `0x15bf70`'s
    /// and then `0x2c59b0`'s; 0 on every other thread.
    static HOLDERS: Cell<usize> = const { Cell::new(0) };
}

#[repr(C, align(16))]
struct Holders([u8; 2 * HOLDER_SIZE]);

/// The game's object of `0x15bf70`, once asked for.
static GAME_VALUE_OBJECT: AtomicUsize = AtomicUsize::new(0);

type Holder = extern "C" fn(usize) -> usize;
static ORIGINAL_VALUE_HOLDER: Function<Holder> = Function::new();
static ORIGINAL_OTHER_HOLDER: Function<Holder> = Function::new();

extern "C" fn value_holder(value: usize) -> usize {
    match HOLDERS.with(Cell::get) {
        0 => ORIGINAL_VALUE_HOLDER.get()(value),
        copies => unsafe {
            *((copies + HOLDER_VALUE) as *mut usize) = value;
            *((copies + HOLDER_BYTE) as *mut u8) = 0;
            copies
        },
    }
}

extern "C" fn other_holder(value: usize) -> usize {
    match HOLDERS.with(Cell::get) {
        0 => ORIGINAL_OTHER_HOLDER.get()(value),
        copies => unsafe {
            let copy = copies + HOLDER_SIZE;
            *((copy + HOLDER_VALUE) as *mut usize) = value;
            copy
        },
    }
}

/// `0x15bf70`'s object for this thread: its copy on a thread of the pool, the
/// game's on any other. The stubs of the five places with the function's
/// code inline call it.
extern "C" fn value_object() -> usize {
    match HOLDERS.with(Cell::get) {
        0 => match GAME_VALUE_OBJECT.load(Ordering::Relaxed) {
            0 => {
                let object = crate::hooked_memory().at(VALUE_OBJECT);
                GAME_VALUE_OBJECT.store(object, Ordering::Relaxed);
                object
            }
            object => object,
        },
        copies => copies,
    }
}

/// The places with `0x15bf70`'s code inline: where each stores the value and
/// clears the byte, and where it takes the object's address for the call of
/// its method. Each stub does what the bytes it replaces did, with this
/// thread's object.
const INLINE_1_STORES: usize = 0x4517bd;
const INLINE_1_STORES_NEXT: usize = 0x4517cf;
const INLINE_1_OBJECT: usize = 0x4517df;
const INLINE_1_OBJECT_NEXT: usize = 0x4517ed;
const INLINE_2: usize = 0x451b38;
const INLINE_2_NEXT: usize = 0x451b58;
const INLINE_3_STORES: usize = 0x7ccc73;
const INLINE_3_STORES_NEXT: usize = 0x7ccc85;
const INLINE_3_OBJECT: usize = 0x7ccca0;
const INLINE_3_OBJECT_NEXT: usize = 0x7ccca7;
const INLINE_4: usize = 0x84c5d8;
const INLINE_4_NEXT: usize = 0x84c5fc;
const INLINE_5_STORES: usize = 0x84cbae;
const INLINE_5_STORES_NEXT: usize = 0x84cbc0;
const INLINE_5_OBJECT: usize = 0x84cbd8;
const INLINE_5_OBJECT_NEXT: usize = 0x84cbdf;

crate::resume!(INLINE_1_STORES_RESUME: INLINE_1_STORES_AT = INLINE_1_STORES_NEXT);
crate::resume!(INLINE_1_OBJECT_RESUME: INLINE_1_OBJECT_AT = INLINE_1_OBJECT_NEXT);
crate::resume!(INLINE_2_RESUME: INLINE_2_AT = INLINE_2_NEXT);
crate::resume!(INLINE_3_STORES_RESUME: INLINE_3_STORES_AT = INLINE_3_STORES_NEXT);
crate::resume!(INLINE_3_OBJECT_RESUME: INLINE_3_OBJECT_AT = INLINE_3_OBJECT_NEXT);
crate::resume!(INLINE_4_RESUME: INLINE_4_AT = INLINE_4_NEXT);
crate::resume!(INLINE_5_STORES_RESUME: INLINE_5_STORES_AT = INLINE_5_STORES_NEXT);
crate::resume!(INLINE_5_OBJECT_RESUME: INLINE_5_OBJECT_AT = INLINE_5_OBJECT_NEXT);

/// `mov [byte], r12b; lea rax, [r14 + rbp]; mov [value], rax`.
#[unsafe(naked)]
extern "C" fn inline_1_stores_stub() {
    std::arch::naked_asm!(
        "lea rax, [r14 + rbp]",
        "push rcx",
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "mov byte ptr [rcx + 0x18], r12b",
        "mov [rcx + 0x10], rax",
        "pop rcx",
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_1_STORES_AT,
    );
}

/// `mov rax, [object]; lea rcx, [object]`.
#[unsafe(naked)]
extern "C" fn inline_1_object_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "mov rax, [rcx]",
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_1_OBJECT_AT,
    );
}

/// `lea rax, [rdi + 8]; mov [byte], r15b; mov [value], rax; lea rcx, [object];
/// mov rax, [object]`.
#[unsafe(naked)]
extern "C" fn inline_2_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "lea rax, [rdi + 8]",
        "mov byte ptr [rcx + 0x18], r15b",
        "mov [rcx + 0x10], rax",
        "mov rax, [rcx]",
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_2_AT,
    );
}

/// `lea rax, [rbx + 8]; mov [value], rax; mov byte ptr [byte], 0`.
#[unsafe(naked)]
extern "C" fn inline_3_stores_stub() {
    std::arch::naked_asm!(
        "lea rax, [rbx + 8]",
        "push rcx",
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "mov [rcx + 0x10], rax",
        "mov byte ptr [rcx + 0x18], 0",
        "pop rcx",
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_3_STORES_AT,
    );
}

/// `lea rcx, [object]`.
#[unsafe(naked)]
extern "C" fn inline_3_object_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_3_OBJECT_AT,
    );
}

/// `lea rax, [rsp + 0x498]; mov [byte], r15b; mov [value], rax;
/// lea rcx, [object]; mov rax, [object]`. The address is taken before the
/// stub moves the stack.
#[unsafe(naked)]
extern "C" fn inline_4_stub() {
    std::arch::naked_asm!(
        "lea rax, [rsp + 0x498]",
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "mov byte ptr [rcx + 0x18], r15b",
        "mov [rcx + 0x10], rax",
        "mov rax, [rcx]",
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_4_AT,
    );
}

/// `lea rax, [rbx + 4]; mov [value], rax; mov byte ptr [byte], 0`.
#[unsafe(naked)]
extern "C" fn inline_5_stores_stub() {
    std::arch::naked_asm!(
        "lea rax, [rbx + 4]",
        "push rcx",
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "mov [rcx + 0x10], rax",
        "mov byte ptr [rcx + 0x18], 0",
        "pop rcx",
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_5_STORES_AT,
    );
}

/// `lea rcx, [object]`.
#[unsafe(naked)]
extern "C" fn inline_5_object_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_5_OBJECT_AT,
    );
}

// --- The loops ---

crate::resume!(PROVINCES_RESUME: PROVINCES_GAME = PROVINCES_ITEMS, PROVINCES_DONE = PROVINCES_END);
crate::resume!(COUNTRIES_RESUME: COUNTRIES_GAME = COUNTRIES_ITEMS, COUNTRIES_DONE = COUNTRIES_END);

/// Replaces `mov edi, r13d; mov rax, [r14 + 0x1c98]` at the start of the
/// provinces' loop: the framework writes them, and the game goes on at the
/// loop's end, or the game's loop runs.
#[unsafe(naked)]
extern "C" fn provinces_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, r14", "mov rdx, rsi", call write, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "mov edi, r13d",
        "mov rax, [r14 + 0x1c98]",
        "jmp qword ptr [rip + {game}]",
        "2:",
        "jmp qword ptr [rip + {done}]",
        write = sym write_provinces,
        game = sym PROVINCES_GAME,
        done = sym PROVINCES_DONE,
    );
}

/// Replaces `mov edi, r13d; mov rax, [r14]` at the start of the countries'
/// loop, as [`provinces_stub`] does.
#[unsafe(naked)]
extern "C" fn countries_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, r14", "mov rdx, rsi", call write, "cmp byte ptr [rsp + 0x48], 0"),
        "jne 2f",
        "mov edi, r13d",
        "mov rax, [r14]",
        "jmp qword ptr [rip + {game}]",
        "2:",
        "jmp qword ptr [rip + {done}]",
        write = sym write_countries,
        game = sym COUNTRIES_GAME,
        done = sym COUNTRIES_DONE,
    );
}

extern "C" fn write_provinces(world: usize, writer: usize) -> bool {
    crate::guarded("parallel save", || write_loop(Loop::Provinces, world, writer))
}

extern "C" fn write_countries(world: usize, writer: usize) -> bool {
    crate::guarded("parallel save", || write_loop(Loop::Countries, world, writer))
}

/// Writes a loop's items on several threads into the writer's stream, and
/// says whether it did; when it did not, the game's loop writes them.
fn write_loop(which: Loop, world: usize, writer: usize) -> bool {
    if super::game_writes_saves() {
        return false;
    }
    let m = crate::hooked_memory();
    let start = Instant::now();
    let writing = match Writing::new(m, which, world, writer) {
        Ok(Some(writing)) => Arc::new(writing),
        Ok(None) => return false,
        Err(e) => {
            log!("saves: the game writes the {} itself: {e}", which.name());
            return false;
        }
    };
    let Some(pool) = pool() else { return false };
    let Ok(_one_at_a_time) = pool.running.try_lock() else { return false };
    let threads = pool.run(&writing);
    let outputs = match writing.outputs() {
        Ok(outputs) => outputs,
        Err(e) => {
            log!("saves: the game writes the {} itself: {e}", which.name());
            return false;
        }
    };
    let bytes: usize = outputs.iter().map(Vec::len).sum();
    LAST_RUN[which as usize].store(outputs.iter().map(Vec::len).max().unwrap_or(0), Ordering::Relaxed);
    #[cfg(feature = "dev-test-switches")]
    if crate::dev::options().test_switches.compare_saves {
        return compare::begin(m, which, writer, outputs);
    }
    if let Err(e) = append(m, writing.stream, &outputs) {
        // Part of the bytes may be in the stream: the save is not what the
        // game would have written, so the game ends rather than keep it.
        crate::fatal(&format!("saves: writing the {} into the save: {e}", which.name()));
    }
    log!("saves: wrote {} {} on {threads} threads, {bytes} bytes, in {:.1?}", writing.items.len(), which.name(), start.elapsed());
    true
}

/// Writes runs' bytes into the game's stream, in their order, with its own
/// write. A stream that fills keeps what the game's loop would have left in
/// it, the bytes being the same, and says so as the game's does.
fn append(m: Memory, stream: usize, outputs: &[Vec<u8>]) -> Result<()> {
    let write = m.vslot(stream, STREAM_WRITE_SLOT)?;
    for out in outputs.iter().filter(|o| !o.is_empty()) {
        m.call(write, &[stream, out.as_ptr() as usize, out.len()])?;
    }
    Ok(())
}

/// A loop to write: its items, cut into runs, and what each run needs.
struct Writing {
    m: Memory,
    which: Loop,
    /// The game's stream, which the runs' bytes go into.
    stream: usize,
    /// The items' addresses, in the loop's order.
    items: Vec<usize>,
    runs: Vec<std::ops::Range<usize>>,
    /// The next run to write.
    next: AtomicUsize,
    failed: AtomicBool,
    outputs: Vec<Mutex<Option<Result<Vec<u8>>>>>,
    /// Copies of the game's writer and stream, which each run's start from.
    writer: [u8; WRITER_SIZE],
    stream_copy: [u8; STREAM_SIZE],
    /// What the game's two static objects hold, which each thread of the
    /// pool's copies start from.
    holders: [u8; 2 * HOLDER_SIZE],
    /// The writer's text of `=`.
    equals: Vec<u8>,
    /// The main thread's floating point control, which the pool's threads
    /// take.
    mxcsr: u32,
    first_buffer: usize,
}

impl Writing {
    /// The loop to write, or None when it is better left to the game.
    fn new(m: Memory, which: Loop, world: usize, writer: usize) -> Result<Option<Self>> {
        if m.ptr(writer)? != m.at(WRITER_VTABLE) {
            return Err("the writer is not the save's".into());
        }
        let stream = m.ptr(writer + WRITER_STREAM)?;
        // By the vtable's write, which a point checks.
        let vtable = m.ptr(stream)?;
        if !MEMORY_STREAM.iter().any(|&v| vtable + STREAM_WRITE_SLOT == m.at(v + STREAM_WRITE_SLOT)) {
            return Err("the writer's stream is not a memory stream".into());
        }
        let items = match which {
            Loop::Provinces => {
                let count = ((m.u64(world + PROVINCE_LAST)?.wrapping_sub(m.u64(world + PROVINCE_FIRST)?) as i64 >> 3) as i32).max(0) as usize;
                let array = m.ptr(world + PROVINCE_ARRAY)?;
                (0..count).map(|i| array + i * PROVINCE_SIZE).collect::<Vec<_>>()
            }
            Loop::Countries => {
                let count = (m.call(m.vslot(world, COUNTRY_COUNT)?, &[world])? as i32).max(0) as usize;
                let array = m.ptr(world + COUNTRY_ARRAY)?;
                (0..count).map(|i| m.ptr(array + i * 8)).collect::<Result<Vec<_>>>()?
            }
        };
        let threads = pool().map_or(1, |p| p.threads.len() + 1);
        if items.len() < FEWEST_ITEMS || threads < 2 {
            return Ok(None);
        }
        // What is made the first time it is needed is made now, on this
        // thread: the writer's texts, and the two static objects.
        if m.u8(m.at(TOKEN_TEXTS_MADE))? == 0 {
            m.call(m.at(MAKE_TOKEN_TEXTS), &[])?;
        }
        if m.i32(m.at(VALUE_OBJECT_GUARD))? == 0 {
            ORIGINAL_VALUE_HOLDER.get()(0);
        }
        if m.i32(m.at(OTHER_OBJECT_GUARD))? == 0 {
            ORIGINAL_OTHER_HOLDER.get()(0);
        }
        let mut holders = [0; 2 * HOLDER_SIZE];
        holders[..HOLDER_SIZE].copy_from_slice(&m.bytes(m.at(VALUE_OBJECT), HOLDER_SIZE)?);
        holders[HOLDER_SIZE..].copy_from_slice(&m.bytes(m.at(OTHER_OBJECT), HOLDER_SIZE)?);
        let texts = m.ptr(m.at(TOKEN_TEXTS))?;
        let text = if m.i32(m.at(TOKEN_TEXTS_COUNT))? >= EQUALS as i32 { texts + 0x20 * EQUALS as usize } else { texts };
        let equals = std_string(m, text)?;
        let runs = runs(items.len(), threads);
        let last = LAST_RUN[which as usize].load(Ordering::Relaxed);
        Ok(Some(Self {
            m,
            which,
            stream,
            outputs: runs.iter().map(|_| Mutex::new(None)).collect(),
            items,
            runs,
            next: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
            writer: m.bytes(writer, WRITER_SIZE)?.try_into().unwrap(),
            stream_copy: m.bytes(stream, STREAM_SIZE)?.try_into().unwrap(),
            holders,
            equals,
            mxcsr: mxcsr(),
            first_buffer: if last == 0 { FIRST_BUFFER } else { last + last / 4 },
        }))
    }

    /// Writes runs until none is left; on the pool's threads with their own
    /// copies of the two static objects.
    fn work(&self, holders: Option<&mut Holders>) {
        if let Some(h) = holders {
            h.0.copy_from_slice(&self.holders);
            set_mxcsr(self.mxcsr);
        }
        loop {
            let i = self.next.fetch_add(1, Ordering::SeqCst);
            if i >= self.runs.len() || self.failed.load(Ordering::SeqCst) {
                return;
            }
            let written = self.write_run(self.runs[i].clone());
            if written.is_err() {
                self.failed.store(true, Ordering::SeqCst);
            }
            *self.outputs[i].lock().unwrap_or_else(|e| e.into_inner()) = Some(written);
        }
    }

    /// One run's bytes, written as the game's loop writes them, into a buffer
    /// that is made larger until they fit.
    fn write_run(&self, items: std::ops::Range<usize>) -> Result<Vec<u8>> {
        let m = self.m;
        let mut capacity = self.first_buffer;
        loop {
            let mut out: Vec<u8> = Vec::with_capacity(capacity);
            let mut stream = Object::<STREAM_SIZE>(self.stream_copy);
            stream.set(STREAM_GROWING, 0usize);
            stream.set(STREAM_BUFFER, out.as_mut_ptr() as usize);
            stream.set(STREAM_KEPT, 0u32);
            stream.set(STREAM_AT, 0u32);
            stream.set(STREAM_CAPACITY, capacity as u32);
            stream.set(STREAM_FULL, 0u8);
            stream.set(STREAM_OWNS_BUFFER, 0u8);
            stream.set(STREAM_OWNS_GROWING, 0u8);
            let mut writer = Object::<WRITER_SIZE>(self.writer);
            writer.set(WRITER_STREAM, stream.address());
            let depth: i32 = writer.get(WRITER_DEPTH);
            for &item in &self.items[items.clone()] {
                self.write_item(m, writer.address(), item)?;
            }
            if stream.get::<u8>(STREAM_FULL) != 0 {
                capacity *= 2;
                if capacity > MOST_BUFFER {
                    return Err(format!("a run of the {} writes more than {MOST_BUFFER} bytes", self.which.name()));
                }
                continue;
            }
            if writer.get::<i32>(WRITER_DEPTH) != depth {
                return Err(format!("a run of the {} ends at another depth", self.which.name()));
            }
            unsafe { out.set_len(stream.get::<u32>(STREAM_AT) as usize) };
            return Ok(out);
        }
    }

    /// What the game's loop does for one item.
    fn write_item(&self, m: Memory, writer: usize, item: usize) -> Result<()> {
        match self.which {
            Loop::Provinces => {
                let id = m.i32(item + PROVINCE_ID)?;
                if id == 0 {
                    return Ok(());
                }
                let key = GameToken::new(NUMBER, &id.wrapping_neg().to_string());
                m.call(m.at(WRITE_TOKEN), &[writer, &key as *const _ as usize])?;
                let equals = GameToken::from_bytes(EQUALS, &self.equals);
                m.call(m.at(WRITE_TOKEN), &[writer, &equals as *const _ as usize])?;
                m.call(m.vslot(item, WRITE_ITSELF)?, &[item, writer])?;
            }
            Loop::Countries => {
                let tag = m.u64(item + COUNTRY_TAG)?;
                let mut key = Object::<0x20>([0; 0x20]);
                m.call(m.at(TAG_STRING), &[key.address(), &tag as *const u64 as usize])?;
                let written = m.call(m.at(WRITE_COUNTRY), &[writer, key.address(), item]);
                m.call(m.at(DROP_STRING), &[key.address()])?;
                written?;
            }
        }
        Ok(())
    }

    /// Every run's bytes, in order, once every thread is done.
    fn outputs(&self) -> Result<Vec<Vec<u8>>> {
        let mut all = Vec::with_capacity(self.outputs.len());
        for (i, out) in self.outputs.iter().enumerate() {
            match out.lock().unwrap_or_else(|e| e.into_inner()).take() {
                Some(Ok(bytes)) => all.push(bytes),
                Some(Err(e)) => return Err(e),
                None => return Err(format!("run {i} of the {} was not written", self.which.name())),
            }
        }
        Ok(all)
    }
}

/// The list of `items` cut into runs for `threads` threads, in its order.
fn runs(items: usize, threads: usize) -> Vec<std::ops::Range<usize>> {
    let count = (threads * RUNS_A_THREAD).min(items);
    (0..count).map(|i| i * items / count..(i + 1) * items / count).collect()
}

/// A game object the framework makes: its bytes, aligned as the game's are.
#[repr(C, align(16))]
struct Object<const N: usize>([u8; N]);

impl<const N: usize> Object<N> {
    fn address(&mut self) -> usize {
        self.0.as_mut_ptr() as usize
    }

    fn set<T: Copy>(&mut self, at: usize, value: T) {
        assert!(at + std::mem::size_of::<T>() <= N);
        unsafe { std::ptr::write_unaligned(self.0.as_mut_ptr().add(at) as *mut T, value) }
    }

    fn get<T: Copy>(&self, at: usize) -> T {
        assert!(at + std::mem::size_of::<T>() <= N);
        unsafe { std::ptr::read_unaligned(self.0.as_ptr().add(at) as *const T) }
    }
}

/// The bytes of a `std::string` of the game's, with its NUL.
fn std_string(m: Memory, at: usize) -> Result<Vec<u8>> {
    let len = m.u64(at + 0x10)? as usize;
    let data = if m.u64(at + 0x18)? >= 0x10 { m.ptr(at)? } else { at };
    m.bytes(data, len + 1)
}

fn mxcsr() -> u32 {
    let mut value = 0u32;
    unsafe { std::arch::asm!("stmxcsr [{}]", in(reg) &mut value, options(nostack)) };
    value
}

fn set_mxcsr(value: u32) {
    unsafe { std::arch::asm!("ldmxcsr [{}]", in(reg) &value, options(nostack)) };
}

// --- The pool ---

struct Pool {
    /// Its threads' ids.
    threads: Vec<u32>,
    state: Mutex<State>,
    start: Condvar,
    done: Condvar,
    /// Held while a loop is written: one at a time.
    running: Mutex<()>,
    /// A thread did not come back in time: the pool is not used again.
    broken: AtomicBool,
}

struct State {
    writing: Option<Arc<Writing>>,
    /// Counts the loops handed out, which tells a thread a new one is there.
    round: u64,
    /// The threads still writing the current loop.
    busy: usize,
}

static POOL: OnceLock<Option<Pool>> = OnceLock::new();

/// The pool, made at startup; None when it has no thread or broke.
fn pool() -> Option<&'static Pool> {
    let pool = POOL
        .get_or_init(|| {
            let wanted = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MOST_THREADS) - 1;
            let pool = Pool {
                threads: Vec::new(),
                state: Mutex::new(State { writing: None, round: 0, busy: 0 }),
                start: Condvar::new(),
                done: Condvar::new(),
                running: Mutex::new(()),
                broken: AtomicBool::new(false),
            };
            let ids = Arc::new(Mutex::new(Vec::new()));
            let started: Vec<_> = (0..wanted)
                .filter_map(|i| {
                    let ids = ids.clone();
                    std::thread::Builder::new()
                        .name(format!("eu4-harmony save writer {i}"))
                        .spawn(move || {
                            ids.lock().unwrap_or_else(|e| e.into_inner()).push(crate::thread_id());
                            worker();
                        })
                        .ok()
                })
                .collect();
            // Each thread says its id before it waits for work.
            let deadline = Instant::now() + Duration::from_secs(5);
            while ids.lock().unwrap_or_else(|e| e.into_inner()).len() < started.len() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            let threads = ids.lock().unwrap_or_else(|e| e.into_inner()).clone();
            log!("saves: {} threads to write saves with", threads.len());
            Some(Pool { threads, ..pool })
        })
        .as_ref()?;
    (!pool.broken.load(Ordering::SeqCst)).then_some(pool)
}

/// Starts the pool's threads, which wait for the first save.
pub fn start() {
    let _ = pool();
}

/// The pool's threads, which the write watch leaves running.
pub fn threads() -> &'static [u32] {
    POOL.get().and_then(Option::as_ref).map_or(&[], |p| &p.threads)
}

fn worker() {
    // Waits for the pool to be made, which happens as this thread starts.
    let pool = loop {
        if let Some(Some(pool)) = POOL.get() {
            break pool;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let holders = Box::leak(Box::new(Holders([0; 2 * HOLDER_SIZE])));
    HOLDERS.with(|h| h.set(holders.0.as_ptr() as usize));
    let mut seen = 0;
    loop {
        let writing = {
            let mut state = pool.state.lock().unwrap_or_else(|e| e.into_inner());
            while state.round == seen {
                state = pool.start.wait(state).unwrap_or_else(|e| e.into_inner());
            }
            seen = state.round;
            state.writing.clone()
        };
        if let Some(writing) = writing {
            crate::guarded("parallel save", || writing.work(Some(holders)));
        }
        let mut state = pool.state.lock().unwrap_or_else(|e| e.into_inner());
        state.busy -= 1;
        pool.done.notify_all();
    }
}

impl Pool {
    /// Writes the loop on every thread of the pool and this one; how many
    /// threads wrote.
    fn run(&self, writing: &Arc<Writing>) -> usize {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.writing = Some(writing.clone());
            state.round += 1;
            state.busy = self.threads.len();
            self.start.notify_all();
        }
        writing.work(None);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = Instant::now() + WAIT;
        while state.busy > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.broken.store(true, Ordering::SeqCst);
                writing.failed.store(true, Ordering::SeqCst);
                log!("saves: a thread writing the {} did not finish in {WAIT:?}; saves are written by the game alone from now on", writing.which.name());
                break;
            }
            state = self.done.wait_timeout(state, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        state.writing = None;
        self.threads.len() + 1
    }
}

/// A developer build's test switch `compare-saves`: the game's own loop runs
/// as well, into a stream of the framework's, and what it wrote is compared
/// with what the threads wrote; the game's bytes go into the save.
#[cfg(feature = "dev-test-switches")]
mod compare {
    use super::*;

    struct Pending {
        which: Loop,
        writer: usize,
        stream: usize,
        parallel: Vec<u8>,
        capture: Box<Object<STREAM_SIZE>>,
        buffer: Vec<u8>,
    }

    static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

    /// Points the writer at a stream of the framework's and lets the game's
    /// loop run.
    pub(super) fn begin(m: Memory, which: Loop, writer: usize, outputs: Vec<Vec<u8>>) -> bool {
        let parallel = outputs.concat();
        let outcome = (|| {
            let stream = m.ptr(writer + WRITER_STREAM)?;
            let capacity = parallel.len() * 2 + (64 << 20);
            let mut buffer: Vec<u8> = Vec::with_capacity(capacity);
            let mut capture = Box::new(Object::<STREAM_SIZE>(m.bytes(stream, STREAM_SIZE)?.try_into().unwrap()));
            capture.set(STREAM_GROWING, 0usize);
            capture.set(STREAM_BUFFER, buffer.as_mut_ptr() as usize);
            capture.set(STREAM_KEPT, 0u32);
            capture.set(STREAM_AT, 0u32);
            capture.set(STREAM_CAPACITY, capacity as u32);
            capture.set(STREAM_FULL, 0u8);
            capture.set(STREAM_OWNS_BUFFER, 0u8);
            capture.set(STREAM_OWNS_GROWING, 0u8);
            unsafe { *((writer + WRITER_STREAM) as *mut usize) = capture.address() };
            Ok::<_, String>(Pending { which, writer, stream, parallel, capture, buffer })
        })();
        match outcome {
            Ok(pending) => *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(pending),
            Err(e) => log!("saves: compare-saves: {e}"),
        }
        false
    }

    /// At the loop's end: the writer back on the save's stream, the two
    /// compared, and the game's bytes written into the save.
    pub(super) extern "C" fn end(writer: usize) {
        crate::guarded("parallel save comparison", || {
            let Some(mut p) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
            if p.writer != writer {
                return;
            }
            unsafe { *((writer + WRITER_STREAM) as *mut usize) = p.stream };
            let m = crate::hooked_memory();
            let (full, at) = (p.capture.get::<u8>(STREAM_FULL), p.capture.get::<u32>(STREAM_AT) as usize);
            let game = if full == 0 {
                unsafe { p.buffer.set_len(at) };
                std::mem::take(&mut p.buffer)
            } else {
                log!("saves: compare-saves: the {}' stream was full; the threads' bytes go into the save", p.which.name());
                p.parallel.clone()
            };
            match game.iter().zip(&p.parallel).position(|(a, b)| a != b) {
                None if game.len() == p.parallel.len() => log!("saves: compare-saves: the {} the same, {} bytes", p.which.name(), game.len()),
                at => log!(
                    "saves: compare-saves: the {} differ at byte {}, the game wrote {} bytes and the threads {}",
                    p.which.name(),
                    at.unwrap_or(game.len().min(p.parallel.len())),
                    game.len(),
                    p.parallel.len()
                ),
            }
            if let Err(e) = append(m, p.stream, &[game]) {
                crate::fatal(&format!("saves: compare-saves: writing the {} into the save: {e}", p.which.name()));
            }
        });
    }

    crate::resume!(PROVINCES_END_RESUME: PROVINCES_END_AT = PROVINCES_END + 8);
    crate::resume!(COUNTRIES_END_RESUME: COUNTRIES_END_AT = COUNTRIES_END + 8);
    static CLOSE: AtomicUsize = AtomicUsize::new(0);

    /// Replaces `mov rcx, rsi; call <close a block>` at a loop's end.
    #[unsafe(naked)]
    extern "C" fn provinces_end_stub() {
        std::arch::naked_asm!(
            crate::call_rust!("mov rcx, rsi", call end),
            "mov rcx, rsi",
            "call qword ptr [rip + {close}]",
            "jmp qword ptr [rip + {next}]",
            end = sym end,
            close = sym CLOSE,
            next = sym PROVINCES_END_AT,
        );
    }

    #[unsafe(naked)]
    extern "C" fn countries_end_stub() {
        std::arch::naked_asm!(
            crate::call_rust!("mov rcx, rsi", call end),
            "mov rcx, rsi",
            "call qword ptr [rip + {close}]",
            "jmp qword ptr [rip + {next}]",
            end = sym end,
            close = sym CLOSE,
            next = sym COUNTRIES_END_AT,
        );
    }

    pub(super) fn points() -> Vec<Point> {
        vec![
            Point::jump(PROVINCES_END, "48 8b ce e8 a9 70 fb 00", "provinces written", provinces_end_stub as *const () as usize, &PROVINCES_END_RESUME),
            Point::jump(COUNTRIES_END, "48 8b ce e8 24 70 fb 00", "countries written", countries_end_stub as *const () as usize, &COUNTRIES_END_RESUME),
            Point::keep(super::super::WRITE_CLOSE, "40 53 48 81 ec 30 02 00 00 ff 49 08 48 8b d9", "save writer closes a block (compare-saves)")
                .recorded(&CLOSE),
        ]
    }
}

pub fn points() -> Vec<Point> {
    #[cfg_attr(not(feature = "dev-test-switches"), expect(unused_mut, reason = "only compare-saves changes them"))]
    let mut points = vec![
        Point::jump(PROVINCES, "41 8b fd 49 8b 86 98 1c 00 00", "provinces' loop", provinces_stub as *const () as usize, &PROVINCES_RESUME)
            .uses(&[PROVINCE_LAST]),
        // The loop the framework writes as the game does: each province with
        // an id, `-id`, `=` and the province's own write.
        Point::keep(
            PROVINCES_ITEMS,
            "49 2b 86 90 1c 00 00 48 c1 f8 03 85 c0 0f 8e d8 00 00 00 49 8b dd 66 0f 1f 44 00 00 49 8b 86 a8 1c 00 00 44 8b 44 03 20 45 85 c0 \
             0f 84 97 00 00 00 41 f7 d8 c7 45 e0 0c 00 00 00 c6 85 e4 01 00 00 00 48 8d 15 c7 36 61 01 48 8d 4d e4 e8 9a ab a0 ff 48 8d 55 e0 \
             48 8b ce e8 ce 71 fb 00 80 3d 87 64 c0 01 00 75 05 e8 a0 bb fb 00 48 8b 15 d1 a1 cd 01 83 3d 6a 64 c0 01 01 7c 04 48 83 c2 20 c6 \
             85 e4 01 00 00 00 c7 45 e0 01 00 00 00 8b 42 10 48 83 7a 18 10 72 03 48 8b 12 ff c0 4c 63 c0 48 8d 4d e4 e8 63 40 2e 01 48 8d 55 \
             e0 48 8b ce e8 77 71 fb 00 49 8b 8e a8 1c 00 00 48 03 cb 48 8b 01 48 8b d6 ff 50 08 ff c7 48 81 c3 10 2e 00 00 49 8b 86 98 1c 00 \
             00 49 2b 86 90 1c 00 00 48 c1 f8 03 3b f8 0f 8c 31 ff ff ff",
            "provinces' items",
        )
        .uses(&[
            PROVINCE_FIRST,
            PROVINCE_ARRAY,
            PROVINCE_ID,
            PROVINCE_SIZE,
            NUMBER as usize,
            EQUALS as usize,
            TOKEN_TEXTS,
            TOKEN_TEXTS_COUNT,
            TOKEN_TEXTS_MADE,
            WRITE_ITSELF,
            0x1d64d54,
        ]),
        Point::keep(PROVINCES_END, "48 8b ce e8 a9 70 fb 00", "provinces' loop ends"),
        Point::jump(COUNTRIES, "41 8b fd 49 8b 06", "countries' loop", countries_stub as *const () as usize, &COUNTRIES_RESUME),
        // Each country, its tag as a string, written under it.
        Point::keep(
            COUNTRIES_ITEMS,
            "49 8b ce ff 50 40 85 c0 7e 58 0f 1f 40 00 8b cf 49 8b 86 e8 1c 00 00 48 8b 1c c8 48 8b 43 20 48 89 85 c8 ac 00 00 48 8d 95 c8 ac \
             00 00 48 8d 4c 24 30 e8 b7 3a 94 ff 90 4c 8b c3 48 8d 54 24 30 48 8b ce e8 86 62 fb 00 90 48 8d 4c 24 30 e8 bb 3e 94 ff ff c7 49 \
             8b 06 49 8b ce ff 50 40 3b f8 7c ac",
            "countries' items",
        )
        .uses(&[COUNTRY_COUNT, COUNTRY_ARRAY, COUNTRY_TAG]),
        Point::keep(COUNTRIES_END, "48 8b ce e8 24 70 fb 00", "countries' loop ends"),
        Point::keep(TAG_STRING, "40 53 48 83 ec 20 33 c0 48 c7 41 18 0f 00 00 00", "tag as a string"),
        Point::keep(DROP_STRING, "40 53 48 83 ec 20 48 8b 51 18 48 8b d9 48 83 fa", "string let go"),
        Point::keep(WRITE_COUNTRY, "48 89 5c 24 08 48 89 74 24 10 57 48 81 ec 30 02", "save writer writes a country"),
        Point::keep(MAKE_TOKEN_TEXTS, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 57", "save writer makes its texts"),
        // The writer `0x14e3f00` makes: its vtable, depth, stream and bytes.
        Point::keep(
            0x14e3f64,
            "4c 8d 3d 6d b8 8d 00 4c 89 7c 24 30 44 89 64 24 38 c6 44 24 49 00 8b 4b 10 85 c9 0f 94 44 24 48 48 89 5c 24 40",
            "save writer made",
        )
        .uses(&[WRITER_VTABLE, WRITER_STREAM]),
        Point::keep(STREAM_WRITE, "40 57 41 54 41 56 41 57 48 83 ec 38 48 83 79 38 00 4d 8b f8 4c 63 71 50 4c 8b e2 48 8b f9 75 1c 48 63 51 4c 49 03 d0 49 3b d6 7e 10 c6 41 54 01", "memory stream writes")
            .uses(&[STREAM_GROWING, STREAM_CAPACITY, STREAM_AT, STREAM_FULL]),
        Point::keep(
            STREAM_MADE,
            "48 8d 05 c2 16 6a 00 48 89 03 48 89 73 38 48 89 6b 40 4c 89 73 48 89 7b 50 c6 43 54 00 0f b6 84 24 80 00 00 00 88 43 55 0f b6 84 24 88 00 00 00 88 43 56",
            "memory stream made",
        )
        .uses(&[MEMORY_STREAM[1], STREAM_GROWING, STREAM_BUFFER, STREAM_KEPT, STREAM_CAPACITY, STREAM_FULL, STREAM_OWNS_BUFFER, STREAM_OWNS_GROWING]),
        Point::method(MEMORY_STREAM[0], STREAM_WRITE_SLOT, STREAM_WRITE, "memory stream's write"),
        Point::method(MEMORY_STREAM[1], STREAM_WRITE_SLOT, STREAM_WRITE, "other memory stream's write"),
        Point::replace(VALUE_HOLDER, "40 53 48 83 ec 20 65 48 8b 04 25 58 00 00 00 48", "save value holder", value_holder as Holder, &ORIGINAL_VALUE_HOLDER),
        Point::keep(
            VALUE_HOLDER_STORES,
            "39 05 7d cc 2f 02 7f 1b 48 89 1d 64 cc 2f 02 48 8d 05 4d cc 2f 02 c6 05 5e cc 2f 02 00",
            "save value holder stores",
        )
        .uses(&[VALUE_OBJECT_GUARD, VALUE_OBJECT_VALUE, VALUE_OBJECT, VALUE_OBJECT_BYTE]),
        Point::replace(OTHER_HOLDER, "40 53 48 83 ec 20 65 48 8b 04 25 58 00 00 00 48", "other save value holder", other_holder as Holder, &ORIGINAL_OTHER_HOLDER),
        Point::keep(OTHER_HOLDER_STORES, "39 05 c5 34 19 02 7f 14 48 89 1d b4 34 19 02 48 8d 05 9d 34 19 02", "other save value holder stores")
            .uses(&[OTHER_OBJECT_GUARD, OTHER_OBJECT_VALUE, OTHER_OBJECT]),
        Point::jump(
            INLINE_1_STORES,
            "44 88 25 44 74 00 02 49 8d 04 2e 48 89 05 31 74 00 02",
            "save value held inline (1)",
            inline_1_stores_stub as *const () as usize,
            &INLINE_1_STORES_RESUME,
        )
        .uses(&[VALUE_OBJECT_BYTE, VALUE_OBJECT_VALUE]),
        Point::jump(
            INLINE_1_OBJECT,
            "48 8b 05 0a 74 00 02 48 8d 0d 03 74 00 02",
            "save value holder inline (1)",
            inline_1_object_stub as *const () as usize,
            &INLINE_1_OBJECT_RESUME,
        )
        .uses(&[VALUE_OBJECT]),
        Point::jump(
            INLINE_2,
            "48 8d 47 08 44 88 3d c5 70 00 02 48 89 05 b6 70 00 02 48 8d 0d 9f 70 00 02 48 8b 05 98 70 00 02",
            "save value held inline (2)",
            inline_2_stub as *const () as usize,
            &INLINE_2_RESUME,
        )
        .uses(&[VALUE_OBJECT_BYTE, VALUE_OBJECT_VALUE, VALUE_OBJECT]),
        Point::jump(
            INLINE_3_STORES,
            "48 8d 43 08 48 89 05 82 bf c8 01 c6 05 83 bf c8 01 00",
            "save value held inline (3)",
            inline_3_stores_stub as *const () as usize,
            &INLINE_3_STORES_RESUME,
        )
        .uses(&[VALUE_OBJECT_VALUE, VALUE_OBJECT_BYTE]),
        Point::jump(INLINE_3_OBJECT, "48 8d 0d 49 bf c8 01", "save value holder inline (3)", inline_3_object_stub as *const () as usize, &INLINE_3_OBJECT_RESUME)
            .uses(&[VALUE_OBJECT]),
        Point::jump(
            INLINE_4,
            "48 8d 84 24 98 04 00 00 44 88 3d 21 c6 c0 01 48 89 05 12 c6 c0 01 48 8d 0d fb c5 c0 01 48 8b 05 f4 c5 c0 01",
            "save value held inline (4)",
            inline_4_stub as *const () as usize,
            &INLINE_4_RESUME,
        )
        .uses(&[VALUE_OBJECT_BYTE, VALUE_OBJECT_VALUE, VALUE_OBJECT]),
        Point::jump(
            INLINE_5_STORES,
            "48 8d 43 04 48 89 05 47 c0 c0 01 c6 05 48 c0 c0 01 00",
            "save value held inline (5)",
            inline_5_stores_stub as *const () as usize,
            &INLINE_5_STORES_RESUME,
        )
        .uses(&[VALUE_OBJECT_VALUE, VALUE_OBJECT_BYTE]),
        Point::jump(INLINE_5_OBJECT, "48 8d 0d 11 c0 c0 01", "save value holder inline (5)", inline_5_object_stub as *const () as usize, &INLINE_5_OBJECT_RESUME)
            .uses(&[VALUE_OBJECT]),
    ];
    #[cfg(feature = "dev-test-switches")]
    {
        points.retain(|p| !matches!(p.place, crate::points::Place::Rva(PROVINCES_END | COUNTRIES_END)));
        points.extend(compare::points());
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the module asks `Memory::at` for is what its points check, or
    /// the game ends at the first save.
    #[test]
    fn every_address_taken_is_checked() {
        let taken = [
            WRITER_VTABLE,
            MEMORY_STREAM[0] + STREAM_WRITE_SLOT,
            MEMORY_STREAM[1] + STREAM_WRITE_SLOT,
            TOKEN_TEXTS,
            TOKEN_TEXTS_COUNT,
            TOKEN_TEXTS_MADE,
            MAKE_TOKEN_TEXTS,
            VALUE_OBJECT,
            VALUE_OBJECT_GUARD,
            OTHER_OBJECT,
            OTHER_OBJECT_GUARD,
            WRITE_TOKEN,
            TAG_STRING,
            WRITE_COUNTRY,
            DROP_STRING,
        ];
        for rva in taken {
            assert!(crate::points::checks(rva), "{rva:#x}");
        }
    }

    #[test]
    fn runs_cover_the_list_in_order() {
        for (items, threads) in [(64, 8), (65, 8), (1000, 8), (5000, 3), (70, 2)] {
            let runs = runs(items, threads);
            assert!(runs.len() <= threads * RUNS_A_THREAD);
            assert!(runs.iter().all(|r| !r.is_empty()));
            assert_eq!(runs.first().unwrap().start, 0);
            assert_eq!(runs.last().unwrap().end, items);
            assert!(runs.windows(2).all(|w| w[0].end == w[1].start));
        }
    }

    #[test]
    fn an_object_is_aligned_and_read_as_written() {
        let mut o = Object::<WRITER_SIZE>([0; WRITER_SIZE]);
        o.set(WRITER_DEPTH, -1i32);
        o.set(WRITER_STREAM, 0x1234_5678usize);
        assert_eq!((o.get::<i32>(WRITER_DEPTH), o.get::<usize>(WRITER_STREAM)), (-1, 0x1234_5678));
        assert_eq!(o.address() % 16, 0);
    }
}
