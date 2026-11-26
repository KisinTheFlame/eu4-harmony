//! The countries and the provinces of a save written on several threads at
//! once .
//!
//! Writing the game state into memory is most of a save's stop, and the
//! countries' loop and the provinces' loop are most of that
//! (docs/game/day/frame-scheduling.md). The world does not change while it is
//! written, and what the loops write to memory that was there before was
//! watched (docs/game/program/saves.md, "What writing the game state writes"):
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
//! The pool does not keep the game waiting: at the provinces' loop both
//! loops are handed to it, and the game writes on past each loop, the rest of
//! the game state, into its stream as always. Where the world has written its
//! game state, before the save data's entry, the main thread takes the runs
//! still left and waits for the pool, and the loops' bytes are put into the
//! stream where each loop began, what the game wrote after it moved after
//! them.
//!
//! When anything is not as expected (another class of stream or writer, a
//! fault in a run), nothing is written and the game's own loop runs.
//!
//! The pool takes other work as well: the save checksum's sum
//! (`super::sum`).
use crate::base::called_by_game;
use crate::base::Result;
use crate::native::memory::Memory;
use crate::native::objects::commands_rva::{STREAM_VTABLE, TOKENS_LAST, TOKENS_READY, TOKEN_NAMES, WRITER_VTABLE};
use crate::native::objects::saves_rva::{GameToken, EQUALS, NUMBER, WRITER_DEPTH, WRITE_TOKEN};
use crate::native::objects::stream;
use crate::native::points::{Function, GameFunction, Point};
use crate::native::speed_up::{self as speed, SpeedUp};
use crate::speed::pool::{Work, WorkerPool};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The save's course: what the hooks' moments make of the loops handed to the
/// pool, and where their bytes go into the game's stream.
mod course;
pub(crate) use course::join;

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
use crate::native::objects::campaign::{province::SIZE as PROVINCE_SIZE, world::PROVINCES as PROVINCE_ARRAY};
const PROVINCE_ID: usize = 0x20;
const WRITE_ITSELF: usize = 0x8;
/// The world's countries: as many as its method `+0x40` says, pointers at
/// `[world + COUNTRY_ARRAY]`; each has its tag at `+COUNTRY_TAG`.
const COUNTRY_COUNT: usize = 0x40;
const COUNTRY_ARRAY: usize = 0x1ce8;
const COUNTRY_TAG: usize = 0x20;

/// `(string, &tag)`: a tag as a `std::string`, which
/// `game_string::destroy_checked` lets go; `(writer, &key, country)`: a
/// country under its tag.
const TAG_STRING: GameFunction<2> = GameFunction::at(0x95240);
const WRITE_COUNTRY: GameFunction<3> = GameFunction::at(0x1707a20);

/// Makes the token table (`game::commands`' [`TOKEN_NAMES`], the writer's
/// texts of its token types) the first time one is needed.
const MAKE_TOKEN_TEXTS: GameFunction<0> = GameFunction::at(0x170d250);

/// The save's writer (made on `0x14e3f00`'s stack): its vtable is
/// `game::commands`' [`WRITER_VTABLE`]; its stream, and its size.
const WRITER_STREAM: usize = 0x10;
const WRITER_SIZE: usize = 0x20;

/// The game's memory stream, whose layout is `crate::native::objects::stream`.
const MEMORY_STREAM: [usize; 2] = [0x1d92200, STREAM_VTABLE];
const STREAM_WRITE_SLOT: usize = 0x50;
const STREAM_WRITE: usize = 0x171f290;

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
pub(super) struct Holders([u8; 2 * HOLDER_SIZE]);

/// The game's object of `0x15bf70`, once asked for.
static GAME_VALUE_OBJECT: AtomicUsize = AtomicUsize::new(0);

type Holder = extern "C" fn(usize) -> usize;
static ORIGINAL_VALUE_HOLDER: Function<Holder> = Function::new();
static ORIGINAL_OTHER_HOLDER: Function<Holder> = Function::new();

#[called_by_game]
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

#[called_by_game]
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
#[called_by_game]
extern "C" fn value_object() -> usize {
    match HOLDERS.with(Cell::get) {
        0 => match GAME_VALUE_OBJECT.load(Ordering::Relaxed) {
            0 => {
                let object = crate::native::hooked_memory().at(VALUE_OBJECT);
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

resume!(INLINE_1_STORES_RESUME: INLINE_1_STORES_AT = INLINE_1_STORES_NEXT);
resume!(INLINE_1_OBJECT_RESUME: INLINE_1_OBJECT_AT = INLINE_1_OBJECT_NEXT);
resume!(INLINE_2_RESUME: INLINE_2_AT = INLINE_2_NEXT);
resume!(INLINE_3_STORES_RESUME: INLINE_3_STORES_AT = INLINE_3_STORES_NEXT);
resume!(INLINE_3_OBJECT_RESUME: INLINE_3_OBJECT_AT = INLINE_3_OBJECT_NEXT);
resume!(INLINE_4_RESUME: INLINE_4_AT = INLINE_4_NEXT);
resume!(INLINE_5_STORES_RESUME: INLINE_5_STORES_AT = INLINE_5_STORES_NEXT);
resume!(INLINE_5_OBJECT_RESUME: INLINE_5_OBJECT_AT = INLINE_5_OBJECT_NEXT);

/// `mov [byte], r12b; lea rax, [r14 + rbp]; mov [value], rax`.
#[unsafe(naked)]
extern "C" fn inline_1_stores_stub() {
    std::arch::naked_asm!(
        "lea rax, [r14 + rbp]",
        "push rcx",
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
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
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
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
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
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
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
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
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
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
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
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
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
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
        call_rust!(call object, "mov rcx, [rsp + 0x48]"),
        "jmp qword ptr [rip + {next}]",
        object = sym value_object,
        next = sym INLINE_5_OBJECT_AT,
    );
}

// --- The loops ---

resume!(PROVINCES_RESUME: PROVINCES_GAME = PROVINCES_ITEMS, PROVINCES_DONE = PROVINCES_END);
resume!(COUNTRIES_RESUME: COUNTRIES_GAME = COUNTRIES_ITEMS, COUNTRIES_DONE = COUNTRIES_END);

/// Replaces `mov edi, r13d; mov rax, [r14 + 0x1c98]` at the start of the
/// provinces' loop: the framework writes them, and the game goes on at the
/// loop's end, or the game's loop runs.
#[unsafe(naked)]
extern "C" fn provinces_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rcx, r14", "mov rdx, rsi", call write, "cmp byte ptr [rsp + 0x48], 0"),
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
        call_rust!("mov rcx, r14", "mov rdx, rsi", call write, "cmp byte ptr [rsp + 0x48], 0"),
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

#[called_by_game]
extern "C" fn write_provinces(world: usize, writer: usize) -> bool {
    course::provinces(world, writer)
}

#[called_by_game]
extern "C" fn write_countries(world: usize, writer: usize) -> bool {
    course::countries(world, writer)
}

/// Writes runs' bytes into the game's stream, in their order, with its own
/// write. A stream that fills keeps what the game's loop would have left in
/// it, the bytes being the same, and says so as the game's does.
fn append(m: Memory, stream: usize, outputs: &[Vec<u8>]) -> Result<()> {
    write_pieces(m, stream, &outputs.iter().map(Vec::as_slice).collect::<Vec<_>>())
}

fn write_pieces(m: Memory, stream: usize, pieces: &[&[u8]]) -> Result<()> {
    let write = m.vslot(stream, STREAM_WRITE_SLOT)?;
    for piece in pieces.iter().filter(|p| !p.is_empty()) {
        m.call(write, &[stream, piece.as_ptr() as usize, piece.len()])?;
    }
    Ok(())
}

// --- Writing beside the game ---

/// The provinces and the countries of a save, in that order, written by the
/// pool while the game writes on.
struct Ahead {
    writings: [Arc<Writing>; 2],
}

impl Work for Ahead {
    fn work(&self, on_worker: bool) {
        for writing in &self.writings {
            writing.work(on_worker);
        }
    }

    fn abandon(&self) {
        self.writings.iter().for_each(|w| w.abandon());
    }
}

/// Puts runs' bytes into the stream at the places given, in order, and after
/// them what the stream held from each place on: the stream as if each
/// loop's bytes had been written where it began.
fn splice(m: Memory, stream: usize, inserts: &[(u32, Vec<Vec<u8>>)]) -> Result<()> {
    let Some(&(first, _)) = inserts.first() else { return Ok(()) };
    let end = position(m, stream)?;
    if inserts.windows(2).any(|w| w[1].0 < w[0].0) || inserts.last().is_some_and(|(at, _)| *at > end) {
        return Err("the places to put the loops' bytes are out of order".into());
    }
    let tail = read(m, stream, first, end)?;
    let mut pieces: Vec<&[u8]> = Vec::new();
    let mut from = first;
    for (at, runs) in inserts {
        pieces.push(&tail[(from - first) as usize..(at - first) as usize]);
        pieces.extend(runs.iter().map(Vec::as_slice));
        from = *at;
    }
    pieces.push(&tail[(from - first) as usize..]);
    // The stream writes over what it holds from where it stands.
    unsafe { *((stream + stream::AT) as *mut u32) = first };
    write_pieces(m, stream, &pieces)?;
    let expected = end as usize + inserts.iter().flat_map(|(_, runs)| runs).map(Vec::len).sum::<usize>();
    match position(m, stream)? as usize {
        at if at == expected => Ok(()),
        at => Err(format!("the stream stands at {at}, not {expected}")),
    }
}

/// Where a memory stream stands: how many bytes were written into it.
fn position(m: Memory, stream: usize) -> Result<u32> {
    m.u32(stream + stream::AT)
}

/// The bytes a memory stream holds from `from` to `to`: in its buffer up to
/// its capacity, and past that in its growing part.
fn read(m: Memory, stream: usize, from: u32, to: u32) -> Result<Vec<u8>> {
    let (from, to) = (from as usize, to as usize);
    let capacity = m.u32(stream + stream::CAPACITY)? as usize;
    let mut out = Vec::with_capacity(to.saturating_sub(from));
    let fixed = to.min(capacity);
    if from < fixed {
        let buffer = m.ptr(stream + stream::BUFFER)?;
        if buffer == 0 || (m.u32(stream + stream::KEPT)? as usize) < fixed {
            return Err("the stream's buffer does not hold what was written".into());
        }
        out.extend(m.bytes(buffer + from, fixed - from)?);
    }
    if to > capacity {
        let growing = m.ptr(stream + stream::GROWING)?;
        if growing == 0 {
            return Err("the stream holds more than its buffer and has no growing part".into());
        }
        let (start, stop) = (from.max(capacity) - capacity, to - capacity);
        let begin = m.ptr(growing)?;
        if stop > m.ptr(growing + 8)? - begin {
            return Err("the stream's growing part does not hold what was written".into());
        }
        out.extend(m.bytes(begin + start, stop - start)?);
    }
    Ok(out)
}

/// Checked (`crate::speed`): the game's own loops write as well, beside the
/// pool, and their bytes stay in the save.
fn comparing() -> bool {
    speed::checked(&SPEED_UP)
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
    stream_copy: [u8; stream::SIZE],
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
        let threads = pool().map_or(1, WorkerPool::width);
        if items.len() < FEWEST_ITEMS || threads < 2 {
            return Ok(None);
        }
        // What is made the first time it is needed is made now, on this
        // thread: the writer's texts, and the two static objects.
        if m.u8(m.at(TOKENS_READY))? == 0 {
            MAKE_TOKEN_TEXTS.call(&m, [])?;
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
        let texts = m.ptr(m.at(TOKEN_NAMES))?;
        let text = if m.i32(m.at(TOKENS_LAST))? >= EQUALS as i32 { texts + 0x20 * EQUALS as usize } else { texts };
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
            stream_copy: m.bytes(stream, stream::SIZE)?.try_into().unwrap(),
            holders,
            equals,
            mxcsr: mxcsr(),
            first_buffer: if last == 0 { FIRST_BUFFER } else { last + last / 4 },
        }))
    }

    fn abandon(&self) {
        self.failed.store(true, Ordering::SeqCst);
    }

    /// Writes runs until none is left; on the pool's threads with their own
    /// copies of the two static objects.
    fn work(&self, on_worker: bool) {
        if on_worker {
            let copies = HOLDERS.with(Cell::get);
            if copies != 0 {
                unsafe {
                    std::ptr::copy_nonoverlapping(self.holders.as_ptr(), copies as *mut u8, self.holders.len());
                }
            }
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
            let mut stream = Object::<{ stream::SIZE }>(self.stream_copy);
            stream.set(stream::GROWING, 0usize);
            stream.set(stream::BUFFER, out.as_mut_ptr() as usize);
            stream.set(stream::KEPT, 0u32);
            stream.set(stream::AT, 0u32);
            stream.set(stream::CAPACITY, capacity as u32);
            stream.set(stream::FULL, 0u8);
            stream.set(stream::OWNS_BUFFER, 0u8);
            stream.set(stream::OWNS_GROWING, 0u8);
            let mut writer = Object::<WRITER_SIZE>(self.writer);
            writer.set(WRITER_STREAM, stream.address());
            let depth: i32 = writer.get(WRITER_DEPTH);
            for &item in &self.items[items.clone()] {
                self.write_item(m, writer.address(), item)?;
            }
            if stream.get::<u8>(stream::FULL) != 0 {
                capacity *= 2;
                if capacity > MOST_BUFFER {
                    return Err(format!("a run of the {} writes more than {MOST_BUFFER} bytes", self.which.name()));
                }
                continue;
            }
            if writer.get::<i32>(WRITER_DEPTH) != depth {
                return Err(format!("a run of the {} ends at another depth", self.which.name()));
            }
            unsafe { out.set_len(stream.get::<u32>(stream::AT) as usize) };
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
                WRITE_TOKEN.call(&m, [writer, &key as *const _ as usize])?;
                let equals = GameToken::from_bytes(EQUALS, &self.equals);
                WRITE_TOKEN.call(&m, [writer, &equals as *const _ as usize])?;
                m.call(m.vslot(item, WRITE_ITSELF)?, &[item, writer])?;
            }
            Loop::Countries => {
                let tag = m.u64(item + COUNTRY_TAG)?;
                let mut key = Object::<0x20>([0; 0x20]);
                TAG_STRING.call(&m, [key.address(), &tag as *const u64 as usize])?;
                let written = WRITE_COUNTRY.call(&m, [writer, key.address(), item]);
                crate::native::game_string::destroy_checked(&m, key.address())?;
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

impl Work for Writing {
    fn work(&self, on_worker: bool) {
        Writing::work(self, on_worker)
    }

    fn abandon(&self) {
        Writing::abandon(self)
    }
}

const TIMEOUT_NOTE: &str = "saves: a thread of the pool did not finish in time; saves are written by the game alone from now on";

static POOL: OnceLock<WorkerPool> = OnceLock::new();

/// The pool, made at startup; None when it broke.
pub(super) fn pool() -> Option<&'static WorkerPool> {
    let pool = POOL.get_or_init(|| {
        let wanted = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MOST_THREADS) - 1;
        let pool = WorkerPool::new("eu4-harmony save writer", "parallel save", wanted, || {
            let holders = Box::leak(Box::new(Holders([0; 2 * HOLDER_SIZE])));
            HOLDERS.with(|h| h.set(holders.0.as_ptr() as usize));
        });
        log!("saves: {} threads to write saves with", pool.width().saturating_sub(1));
        pool
    });
    (!pool.is_broken()).then_some(pool)
}

/// Starts the pool's threads, which wait for the first save.
pub fn start() {
    let _ = pool();
}

/// The pool's threads, which the write watch leaves running.
#[cfg(feature = "dev")]
pub fn threads() -> &'static [u32] {
    POOL.get().map_or(&[], WorkerPool::thread_ids)
}

/// `f` of each of `0..count`, taken in turn by the pool's threads and this
/// one; None when there is no pool, it is in use, or a part was not done.
/// What `f` reads must stay as it is until this returns: the parts are done
/// by then.
pub(super) fn each<T: Send + 'static>(count: usize, f: impl Fn(usize) -> T + Send + Sync + 'static) -> Option<Vec<T>> {
    pool()?.each(count, WAIT, TIMEOUT_NOTE, f)
}

/// Checked, the game's own loops write into the save as always, beside the
/// pool, and where the world has written its game state the pool's bytes are
/// compared with what each loop wrote. Where the game's loops end is observed
/// only when `checked` includes `parallel-saves` in a developer build.
#[cfg(feature = "dev")]
mod compare {
    use super::*;

    /// Where the game's loop ends, `writer` in rsi: where the stream stands.
    #[called_by_game]
    extern "C" fn end(writer: usize, which: usize) {
        course::loop_ended(writer, which);
    }

    resume!(PROVINCES_END_RESUME: PROVINCES_END_AT = PROVINCES_END + 8);
    resume!(COUNTRIES_END_RESUME: COUNTRIES_END_AT = COUNTRIES_END + 8);

    /// Replaces `mov rcx, rsi; call <close a block>` at a loop's end.
    #[unsafe(naked)]
    extern "C" fn provinces_end_stub() {
        std::arch::naked_asm!(
            call_rust!("mov rcx, rsi", "xor edx, edx", call end),
            "mov rcx, rsi",
            "call qword ptr [rip + {close}]",
            "jmp qword ptr [rip + {next}]",
            end = sym end,
            close = sym crate::native::objects::saves_rva::WRITE_CLOSE_ADDR,
            next = sym PROVINCES_END_AT,
        );
    }

    #[unsafe(naked)]
    extern "C" fn countries_end_stub() {
        std::arch::naked_asm!(
            call_rust!("mov rcx, rsi", "mov edx, 1", call end),
            "mov rcx, rsi",
            "call qword ptr [rip + {close}]",
            "jmp qword ptr [rip + {next}]",
            end = sym end,
            close = sym crate::native::objects::saves_rva::WRITE_CLOSE_ADDR,
            next = sym COUNTRIES_END_AT,
        );
    }

    pub(super) fn points() -> Vec<Point> {
        vec![
            Point::jump(PROVINCES_END, "48 8b ce e8 a9 70 fb 00", "provinces written", provinces_end_stub as *const () as usize, &PROVINCES_END_RESUME),
            Point::jump(COUNTRIES_END, "48 8b ce e8 24 70 fb 00", "countries written", countries_end_stub as *const () as usize, &COUNTRIES_END_RESUME),
        ]
    }
}

/// Checked, the game's own loops write into the save as always, beside the
/// pool, and where the world has written its game state the pool's bytes
/// are compared with what each loop wrote.
#[cfg(feature = "dev")]
pub const SPEED_UP: SpeedUp = SpeedUp::new("parallel-saves", "saving", points, true).with_compare(replace_compare_points);
#[cfg(not(feature = "dev"))]
pub const SPEED_UP: SpeedUp = SpeedUp::new("parallel-saves", "saving", points, true);

/// Replaces the read-only `PROVINCES_END` and `COUNTRIES_END` points with
/// jump stubs that record where the game's own loops end for comparison.
#[cfg(feature = "dev")]
pub fn replace_compare_points(points: &mut Vec<Point>) {
    points.retain(|p| !matches!(p.place, crate::native::points::Place::Rva(PROVINCES_END | COUNTRIES_END)));
    points.extend(compare::points());
}

fn points() -> Vec<Point> {
    vec![
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
            TOKEN_NAMES,
            TOKENS_LAST,
            TOKENS_READY,
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
        Point::keep(WRITE_COUNTRY, "48 89 5c 24 08 48 89 74 24 10 57 48 81 ec 30 02", "save writer writes a country"),
        Point::keep(MAKE_TOKEN_TEXTS, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 57", "save writer makes its texts"),
        // The writer `0x14e3f00` makes: its vtable, depth, stream and bytes.
        Point::keep(
            0x14e3f64,
            "4c 8d 3d 6d b8 8d 00 4c 89 7c 24 30 44 89 64 24 38 c6 44 24 49 00 8b 4b 10 85 c9 0f 94 44 24 48 48 89 5c 24 40",
            "save writer made",
        )
        .uses(&[WRITER_VTABLE, WRITER_STREAM]),
        Point::keep(
            STREAM_WRITE,
            "40 57 41 54 41 56 41 57 48 83 ec 38 48 83 79 38 00 4d 8b f8 4c 63 71 50 4c 8b e2 48 8b f9 75 1c 48 63 51 4c 49 03 d0 49 3b d6 7e 10 c6 41 54 01",
            "memory stream writes",
        )
        .uses(&[stream::GROWING, stream::CAPACITY, stream::AT, stream::FULL]),
        Point::method(MEMORY_STREAM[0], STREAM_WRITE_SLOT, STREAM_WRITE, "memory stream's write"),
        Point::method(MEMORY_STREAM[1], STREAM_WRITE_SLOT, STREAM_WRITE, "other memory stream's write"),
        Point::replace(VALUE_HOLDER, "40 53 48 83 ec 20 65 48 8b 04 25 58 00 00 00 48", "save value holder", value_holder as Holder, &ORIGINAL_VALUE_HOLDER),
        Point::keep(VALUE_HOLDER_STORES, "39 05 7d cc 2f 02 7f 1b 48 89 1d 64 cc 2f 02 48 8d 05 4d cc 2f 02 c6 05 5e cc 2f 02 00", "save value holder stores")
            .uses(&[VALUE_OBJECT_GUARD, VALUE_OBJECT_VALUE, VALUE_OBJECT, VALUE_OBJECT_BYTE]),
        Point::replace(
            OTHER_HOLDER,
            "40 53 48 83 ec 20 65 48 8b 04 25 58 00 00 00 48",
            "other save value holder",
            other_holder as Holder,
            &ORIGINAL_OTHER_HOLDER,
        ),
        Point::keep(OTHER_HOLDER_STORES, "39 05 c5 34 19 02 7f 14 48 89 1d b4 34 19 02 48 8d 05 9d 34 19 02", "other save value holder stores").uses(&[
            OTHER_OBJECT_GUARD,
            OTHER_OBJECT_VALUE,
            OTHER_OBJECT,
        ]),
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
        Point::jump(
            INLINE_3_OBJECT,
            "48 8d 0d 49 bf c8 01",
            "save value holder inline (3)",
            inline_3_object_stub as *const () as usize,
            &INLINE_3_OBJECT_RESUME,
        )
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
        Point::jump(
            INLINE_5_OBJECT,
            "48 8d 0d 11 c0 c0 01",
            "save value holder inline (5)",
            inline_5_object_stub as *const () as usize,
            &INLINE_5_OBJECT_RESUME,
        )
        .uses(&[VALUE_OBJECT]),
    ]
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
            TOKEN_NAMES,
            TOKENS_LAST,
            TOKENS_READY,
            MAKE_TOKEN_TEXTS.rva(),
            VALUE_OBJECT,
            VALUE_OBJECT_GUARD,
            OTHER_OBJECT,
            OTHER_OBJECT_GUARD,
            WRITE_TOKEN.rva(),
            TAG_STRING.rva(),
            WRITE_COUNTRY.rva(),
        ];
        for rva in taken {
            assert!(crate::native::points::checks(rva), "{rva:#x}");
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

    /// In-process double-segment memory stream layout matching `native::objects::stream`
    /// so `read()` and `splice()` can be exercised directly across fixed + growing segments.
    struct FakeStream {
        vtable: Box<[usize; 16]>,
        stream: Box<Object<{ stream::SIZE }>>,
        fixed: Box<[u8]>,
        growing_buf: Vec<u8>,
        growing_hdr: Box<[usize; 3]>,
    }

    extern "C" fn fake_stream_write(stream_ptr: usize, data_ptr: usize, len: usize) -> usize {
        unsafe {
            let bytes = std::slice::from_raw_parts(data_ptr as *const u8, len);
            let at_ptr = (stream_ptr + stream::AT) as *mut u32;
            let kept_ptr = (stream_ptr + stream::KEPT) as *mut u32;
            let cap = *((stream_ptr + stream::CAPACITY) as *const u32) as usize;
            let fixed = *((stream_ptr + stream::BUFFER) as *const usize) as *mut u8;
            let growing = *((stream_ptr + stream::GROWING) as *const usize) as *mut usize;
            let mut pos = *at_ptr as usize;
            for &b in bytes {
                if pos < cap {
                    *fixed.add(pos) = b;
                    if pos + 1 > *kept_ptr as usize {
                        *kept_ptr = (pos + 1) as u32;
                    }
                } else {
                    let g_begin = *growing as *mut u8;
                    let g_idx = pos - cap;
                    *g_begin.add(g_idx) = b;
                    let new_end = g_begin.add(g_idx + 1) as usize;
                    if new_end > *growing.add(1) {
                        *growing.add(1) = new_end;
                    }
                }
                pos += 1;
            }
            *at_ptr = pos as u32;
        }
        len
    }

    impl FakeStream {
        fn new(fixed_cap: usize, growing_cap: usize) -> Self {
            let mut vtable = Box::new([0usize; 16]);
            vtable[STREAM_WRITE_SLOT / 8] = fake_stream_write as *const () as usize;
            let fixed = vec![0u8; fixed_cap].into_boxed_slice();
            let mut growing_buf = vec![0u8; growing_cap];
            let g_start = growing_buf.as_mut_ptr() as usize;
            let growing_hdr = Box::new([g_start, g_start, g_start + growing_cap]);
            let mut stream = Box::new(Object::<{ stream::SIZE }>([0; stream::SIZE]));
            stream.set(0, vtable.as_ptr() as usize);
            stream.set(stream::BUFFER, fixed.as_ptr() as usize);
            stream.set(stream::CAPACITY, fixed_cap as u32);
            stream.set(stream::GROWING, growing_hdr.as_ptr() as usize);
            Self { vtable, stream, fixed, growing_buf, growing_hdr }
        }

        fn addr(&mut self) -> usize {
            self.stream.address()
        }

        fn write_all(&mut self, data: &[u8]) {
            let ptr = self.addr();
            fake_stream_write(ptr, data.as_ptr() as usize, data.len());
        }

        fn contents(&mut self) -> Vec<u8> {
            let m = Memory { base: 0 };
            let end = position(m, self.addr()).unwrap();
            read(m, self.addr(), 0, end).unwrap()
        }
    }

    #[test]
    fn memory_stream_read_and_splice_handle_fixed_and_growing_segments() {
        let mut s = FakeStream::new(10, 64);
        // Initial stream: "HEADER|MID|TAIL!" (16 bytes: 10 in fixed, 6 in growing).
        s.write_all(b"HEADER|MID|TAIL!");
        let m = Memory { base: 0 };
        let addr = s.addr();
        assert_eq!(read(m, addr, 0, 16).unwrap(), b"HEADER|MID|TAIL!");
        assert_eq!(read(m, addr, 7, 13).unwrap(), b"MID|TA");
        assert_eq!(read(m, addr, 11, 16).unwrap(), b"TAIL!");

        // Splice provinces at offset 7 (in fixed) and countries at offset 11 (in growing).
        splice(m, addr, &[(7, vec![b"[P1]".to_vec(), b"[P2]".to_vec()]), (11, vec![b"[C1]".to_vec()])]).unwrap();
        assert_eq!(s.contents(), b"HEADER|[P1][P2]MID|[C1]TAIL!");

        // Out-of-order or past-end insert offsets fail cleanly without mutating.
        let end = position(m, addr).unwrap();
        assert!(splice(m, addr, &[(10, vec![]), (5, vec![])]).is_err());
        assert!(splice(m, addr, &[(end + 1, vec![])]).is_err());
        let _ = (&s.vtable, &s.fixed, &s.growing_buf, &s.growing_hdr);
    }
}
