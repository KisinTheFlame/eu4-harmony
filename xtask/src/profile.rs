//! `cargo xtask profile`: samples the running game's call stacks at a steady
//! rate, from outside, with no debugger to install.
//!
//! It finds eu4.exe and, `--hz` times a second, suspends its main thread,
//! takes its registers and copies its stack, reads the date and the world's
//! flags, and resumes it; the stack is walked afterwards, outside the game,
//! with the unwind data of each module's file, so that the thread is held
//! for the copy alone. With `--threads`, every other thread of the game that
//! ran since its last sample is sampled the same way at each tick. It writes:
//!
//! - `<out>.folded`: the main thread's samples, one line per distinct stack,
//!   root first, `frame;frame;... count`, which flame graph tools read;
//! - `<out>.txt`: how many days passed; the share of samples inside a day,
//!   inside a frame that runs inside a day, and between days, and of the
//!   parts of each (as docs/research/main-thread.md counts them); the
//!   functions with the most samples inside them and with the most of their
//!   own; each thread's CPU; with `--threads`, the whole game's functions by
//!   the cores they ran on;
//! - with `--threads`, `<out>.threads.folded`: every thread's samples, each
//!   stack's root the thread (`[main 123]`, the name the game gave it, or
//!   `[thread 456]`), counted only when the sample found the thread running:
//!   one waiting in the kernel, its instruction pointer right after a
//!   `syscall`, takes no CPU, so that a thread counts by the CPU it took and
//!   not by the time it slept, and a function's samples over the ticks are
//!   the cores it ran on;
//! - with `--threads`, `<out>.game.folded`: the same with the threads
//!   merged, the whole game's CPU by where it went;
//! - with `--threads`, `<out>.waited.folded`: what the other threads ran
//!   while the main thread waited, the threads merged, each stack's root
//!   the part the main thread waited in (`[day]: the AI`). A tick's wait,
//!   its interval in microseconds, is shared out evenly among the threads
//!   that ran at it, so that a stack's count is the time the main thread
//!   waited on it: while many threads run each counts for little, and one
//!   that runs alone, as the last of a parallel stage's tasks does, counts
//!   for all; `[nothing ran]` is a wait with no other thread running. A
//!   thread waits when its sample finds it in the kernel, or with its
//!   innermost frame in a function `--waiting-in` names as the stacks name
//!   it (`0x179e90,ntdll.dll+0x1234`): a wait that spins, such as the
//!   engine's run-and-wait for a parallel stage's parts.
//!
//! Each main-thread stack's root is where in the game's time it fell:
//! `[day]`, `[frame inside a day]` or `[between days]`, and with `--slice
//! <seconds>` the stretch of the run it fell in too, as `[between days]
//! 10-20 s`, so that a run through the game's startup or a load shows what
//! each stretch did; `<out>.slices.tsv` then has a line for each stretch: its
//! dates, days and frames a second, with `--threads` the cores the whole game
//! ran, and the share of its samples in each part, so that a long run shows
//! how they change as the game goes on. In `<out>.threads.folded` the
//! stretch, when sliced, comes first, then the thread, then where in the
//! game's time; in `<out>.game.folded` the stretch, then where in the game's
//! time. `--wait` waits for eu4.exe to start, for a run from its first
//! moments; modules the process loads during the run, and threads it
//! starts, are taken up as they come.
//! `--tree` prints the stacks of a .folded file as a tree, top down, or the
//! tree below one frame.
//!
//! Frames are named by the function they lie in, from the exception
//! directory of the module's file: in eu4.exe by the function's RVA,
//! elsewhere by module and export, or module and RVA where the function is
//! not exported.
//!
//! A sample holds its thread for its registers and a copy of its stack,
//! about 0.1 ms for the main thread and less for the others (the report
//! gives it). What sampling from outside cannot see: the kernel's side of a
//! stack, and which thread a waiting one waits for, which Windows' own event
//! tracing gives an administrator; and a stack past the framework's hook
//! stubs, which have no unwind data.
use crate::{Args, Result};
use std::{
    collections::{BTreeMap, HashMap},
    hash::Hash,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree, FILETIME, HANDLE, HMODULE},
    System::{
        Diagnostics::{
            Debug::{
                GetThreadContext, ReadProcessMemory, SymFromAddrW, SymInitializeW, SymLoadModuleExW, SymSetOptions, CONTEXT, CONTEXT_FULL_AMD64, SYMBOL_INFOW,
                SYMOPT_DEFERRED_LOADS, SYMOPT_UNDNAME,
            },
            ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32},
        },
        Memory::{VirtualQueryEx, MEMORY_BASIC_INFORMATION},
        ProcessStatus::{EnumProcessModulesEx, GetModuleBaseNameW, GetModuleFileNameExW, GetModuleInformation, LIST_MODULES_ALL, MODULEINFO},
        Threading::{
            GetThreadDescription, GetThreadTimes, OpenProcess, OpenThread, ResumeThread, SuspendThread, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
            THREAD_GET_CONTEXT, THREAD_QUERY_INFORMATION, THREAD_SUSPEND_RESUME,
        },
        WindowsProgramming::QueryThreadCycleTime,
    },
};

/// The world, and in it the date (hours) and the flags a day runs and a
/// frame runs inside it (docs/research/main-thread.md).
const WORLD: usize = 0x233fe78;
const WORLD_DATE: usize = 0x1dd0;
const WORLD_DAY: usize = 0x1e10;
/// The application, and in it the frame count (docs/research/architecture.md).
const APPLICATION: usize = 0x242bf50;
const APPLICATION_FRAMES: usize = 0x320;

/// The parts of the day and of a frame (docs/research/architecture.md), as
/// docs/research/main-thread.md counts them: the first that is on the stack.
const DAY: [(&str, &str); 5] = [
    ("each country's daily update", "0x2ee280"),
    ("the monthly update", "0x75d690"),
    ("the AI", "0x75b4b0"),
    ("the provinces' passes", "0x75b2d0"),
    ("the yearly update", "0x75e260"),
];
const FRAME: [(&str, &str); 10] = [
    ("the autosave", "0x5cbb00"),
    ("POPS", "pops_api.dll"),
    ("drawing", "0x8183b0"),
    ("the GUI update", "0x14c5810"),
    ("commands", "0x20f810"),
    ("input", "0x830fd0"),
    ("the top bar's alerts", "0xdf5860"),
    ("the outliner", "0x128cd30"),
    ("the map's markers", "0x10a6d00"),
    ("the in-game frame, other", "0x8137c0"),
];
/// The day's own frame, below which a frame inside a day runs.
const FRAME_IN_DAY: &str = "0x769ea0";

pub fn run(args: &Args) -> Result<()> {
    let out = args.positional.first().ok_or("profile needs <out>, or with --tree a .folded file")?;
    if args.flag("tree") {
        return tree(out, args.value("root"), args.number("depth", 6)?, decimal(args, "min", 1.0)?);
    }
    let slice = args.value("slice").map(|_| decimal(args, "slice", 0.0)).transpose()?.filter(|s| *s > 0.0);
    let waiting_in = args.value("waiting-in").map(waiting_functions).transpose()?.unwrap_or_default();
    sample(out, decimal(args, "seconds", 60.0)?, decimal(args, "hz", 50.0)?, slice, args.flag("wait"), args.flag("threads"), &waiting_in)
}

/// The functions `--waiting-in` names, as the stacks name them: `0x179e90`
/// in eu4.exe, `ntdll.dll+0x1234` in another module; each its module and
/// RVA.
fn waiting_functions(text: &str) -> Result<Vec<(String, u32)>> {
    text.split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| {
            let (module, rva) = f.split_once('+').unwrap_or(("eu4.exe", f));
            let rva = u32::from_str_radix(rva.trim_start_matches("0x"), 16)
                .map_err(|_| format!("--waiting-in takes functions as 0x179e90 or ntdll.dll+0x1234; {f:?} is not one"))?;
            Ok((module.to_lowercase(), rva))
        })
        .collect()
}

/// The number `--key` gives, or `default`.
fn decimal(args: &Args, key: &str, default: f64) -> Result<f64> {
    args.value(key).map_or(Ok(default), |v| v.parse().map_err(|_| format!("--{key} takes a number; {v:?} is not one")))
}

/// Counts in the order keys were first met, so that ties keep it.
struct Tally<K> {
    index: HashMap<K, usize>,
    counts: Vec<(K, u64)>,
}

impl<K: Hash + Eq + Clone> Tally<K> {
    fn new() -> Self {
        Tally { index: HashMap::new(), counts: Vec::new() }
    }

    fn add(&mut self, key: K, n: u64) {
        match self.index.get(&key) {
            Some(&i) => self.counts[i].1 += n,
            None => {
                self.index.insert(key.clone(), self.counts.len());
                self.counts.push((key, n));
            }
        }
    }

    /// The counts, most first.
    fn most(&self) -> Vec<&(K, u64)> {
        let mut most: Vec<_> = self.counts.iter().collect();
        most.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        most
    }
}

// ---------- the call tree of a .folded file ----------

#[derive(Default)]
struct Node {
    n: u64,
    children: Vec<(String, Node)>,
}

/// The call tree of a .folded file, top down, in percent of its samples.
fn tree(path: &str, root: Option<&str>, depth: u32, min: f64) -> Result<()> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let (mut total, mut top) = (0, Node::default());
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (stack, n) = line.rsplit_once(' ').ok_or_else(|| format!("not a .folded line: {line:?}"))?;
        let n: u64 = n.parse().map_err(|_| format!("not a .folded line: {line:?}"))?;
        let mut frames: Vec<&str> = stack.split(';').collect();
        total += n;
        if let Some(root) = root {
            let Some(at) = frames.iter().position(|f| *f == root) else { continue };
            frames.drain(..at);
        }
        let mut node = &mut top;
        for frame in frames {
            let i = match node.children.iter().position(|(f, _)| f == frame) {
                Some(i) => i,
                None => {
                    node.children.push((frame.to_string(), Node::default()));
                    node.children.len() - 1
                }
            };
            node = &mut node.children[i].1;
            node.n += n;
        }
    }
    fn show(node: &Node, total: u64, depth: u32, indent: &str, min: f64) {
        let mut children: Vec<_> = node.children.iter().collect();
        children.sort_by_key(|(_, c)| std::cmp::Reverse(c.n));
        for (frame, child) in children {
            let share = 100.0 * child.n as f64 / total as f64;
            if share < min {
                continue;
            }
            println!("{indent}{share:5.1}%  {frame}");
            if depth > 0 {
                show(child, total, depth - 1, &format!("{indent}  "), min);
            }
        }
    }
    show(&top, total.max(1), depth, "", min);
    Ok(())
}

// ---------- which function an address lies in, and how to unwind it ----------

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// A module's file, read by RVA through its sections.
#[derive(Default)]
struct Image {
    data: Vec<u8>,
    /// Each section's RVA, size and offset in the file.
    sections: Vec<(u32, u32, u32)>,
}

impl Image {
    fn read(data: Vec<u8>) -> Option<Self> {
        let pe = u32_at(&data, 0x3c)? as usize;
        let count = u16_at(&data, pe + 6)? as usize;
        let optional = u16_at(&data, pe + 20)? as usize;
        let mut sections = Vec::new();
        for i in 0..count {
            let s = pe + 24 + optional + 40 * i;
            let (size, rva, raw_size, raw) = (u32_at(&data, s + 8)?, u32_at(&data, s + 12)?, u32_at(&data, s + 16)?, u32_at(&data, s + 20)?);
            sections.push((rva, size.max(raw_size), raw));
        }
        Some(Image { data, sections })
    }

    /// The `n` bytes at `rva`, where the file has them.
    fn at(&self, rva: u32, n: usize) -> Option<&[u8]> {
        let &(start, _, raw) = self.sections.iter().find(|(start, size, _)| *start <= rva && rva < start + size)?;
        let offset = (raw + rva - start) as usize;
        self.data.get(offset..offset + n)
    }
}

/// A module's functions from its exception directory, sorted by start: their
/// ends, their unwind data, and the function each belongs to, a chained
/// entry (a function's later part) counting as the function it continues.
#[derive(Default)]
struct Functions {
    starts: Vec<u32>,
    ends: Vec<u32>,
    unwind: Vec<u32>,
    primary: Vec<u32>,
}

/// UNW_FLAG_CHAININFO: the unwind data continues another entry's.
const CHAINED: u8 = 4;

impl Functions {
    fn read(image: &Image) -> Option<Self> {
        let pe = u32_at(&image.data, 0x3c)? as usize;
        let (directory, directory_size) = (u32_at(&image.data, pe + 24 + 112 + 3 * 8)?, u32_at(&image.data, pe + 24 + 112 + 3 * 8 + 4)?);
        let mut entries = Vec::new();
        if directory != 0 {
            let table = image.at(directory, directory_size as usize)?;
            for entry in table.chunks_exact(12) {
                entries.push((u32_at(entry, 0)?, u32_at(entry, 4)?, u32_at(entry, 8)?));
            }
        }
        let primary = |start: u32, unwind: u32| -> Option<u32> {
            let (mut at, mut info) = (start, unwind & !1);
            for _ in 0..32 {
                let head = image.at(info, 4)?;
                if head[0] >> 3 & CHAINED == 0 {
                    break;
                }
                let codes = u32::from(head[2]);
                let chained = image.at(info + 4 + 2 * (codes + (codes & 1)), 12)?;
                at = u32_at(chained, 0)?;
                info = u32_at(chained, 8)? & !1;
            }
            Some(at)
        };
        let mut functions: Vec<_> = entries.iter().map(|&(start, end, unwind)| (start, end, unwind, primary(start, unwind).unwrap_or(start))).collect();
        functions.sort();
        Some(Functions {
            starts: functions.iter().map(|f| f.0).collect(),
            ends: functions.iter().map(|f| f.1).collect(),
            unwind: functions.iter().map(|f| f.2).collect(),
            primary: functions.iter().map(|f| f.3).collect(),
        })
    }

    /// The entry `rva` lies in.
    fn entry(&self, rva: u32) -> Option<usize> {
        let i = self.starts.partition_point(|&s| s <= rva).checked_sub(1)?;
        (rva < self.ends[i]).then_some(i)
    }

    /// The RVA of the function `rva` lies in, or none (a leaf function).
    fn of(&self, rva: u32) -> Option<u32> {
        self.entry(rva).map(|i| self.primary[i])
    }
}

/// A thread's integer registers, by the numbers unwind data gives them (rax,
/// rcx, rdx, rbx, rsp, rbp, rsi, rdi, r8 to r15), and its instruction
/// pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Registers {
    rip: u64,
    r: [u64; 16],
}

const RSP: usize = 4;

/// A copy of a thread's stack, from its stack pointer up.
#[derive(Default)]
struct Stack {
    from: u64,
    bytes: Vec<u8>,
}

impl Stack {
    fn u64(&self, at: u64) -> Option<u64> {
        let offset = usize::try_from(at.checked_sub(self.from)?).ok()?;
        Some(u64::from_le_bytes(self.bytes.get(offset..offset + 8)?.try_into().ok()?))
    }
}

/// Undoes the rest of the epilog `code` (the bytes at the instruction
/// pointer) is in, the way Windows recognizes one: an `add rsp` or `lea
/// rsp`, pops, then a `ret` or a jump out. False when it is in none.
fn undo_epilog(code: &[u8], registers: &mut Registers, stack: &Stack) -> Option<bool> {
    let mut r = *registers;
    let mut i = 0;
    match code {
        [0x48, 0x83, 0xc4, n, ..] => {
            r.r[RSP] += u64::from(*n);
            i = 4;
        }
        [0x48, 0x81, 0xc4, a, b, c, d, ..] => {
            r.r[RSP] += u64::from(u32::from_le_bytes([*a, *b, *c, *d]));
            i = 7;
        }
        // lea rsp, [base + displacement]
        [prefix @ (0x48 | 0x49), 0x8d, modrm, ..] if modrm >> 3 & 7 == RSP as u8 && modrm >> 6 != 3 && modrm & 7 != RSP as u8 => {
            let base = usize::from(modrm & 7) + if *prefix == 0x49 { 8 } else { 0 };
            let (displacement, length) = match modrm >> 6 {
                1 => (i64::from(*code.get(3)? as i8), 4),
                2 => (i64::from(i32::from_le_bytes(code.get(3..7)?.try_into().ok()?)), 7),
                _ => (0, 3),
            };
            r.r[RSP] = r.r[base].wrapping_add_signed(displacement);
            i = length;
        }
        _ => {}
    }
    loop {
        match code.get(i..)? {
            [n @ 0x58..=0x5f, ..] => {
                r.r[usize::from(n - 0x58)] = stack.u64(r.r[RSP])?;
                r.r[RSP] += 8;
                i += 1;
            }
            [0x41, n @ 0x58..=0x5f, ..] => {
                r.r[usize::from(n - 0x58) + 8] = stack.u64(r.r[RSP])?;
                r.r[RSP] += 8;
                i += 2;
            }
            [0xc3 | 0xc2, ..] => break,
            // A jump out ends an epilog only after what undoes the frame.
            [0xe9, ..] | [0xff, 0x25, ..] if i > 0 => break,
            _ => return Some(false),
        }
    }
    r.rip = stack.u64(r.r[RSP])?;
    r.r[RSP] += 8;
    *registers = r;
    Some(true)
}

/// Whether the prolog has set the frame register, `done` bytes into it.
fn set_fpreg_done(codes: &[u8], done: u32) -> bool {
    codes.chunks_exact(2).any(|c| c[1] & 15 == 3 && u32::from(c[0]) <= done)
}

/// How far the prolog has yet to move the stack pointer, `done` bytes into
/// it: its pushes and allocations not yet run.
fn not_yet_moved(codes: &[u8], done: u32) -> Option<u64> {
    let (mut i, mut moved) = (0, 0);
    while i < codes.len() / 2 {
        let (offset, op, op_info) = (u32::from(codes[2 * i]), codes[2 * i + 1] & 15, u64::from(codes[2 * i + 1] >> 4));
        let slot = |n: usize| -> Option<u64> { Some(u64::from(u16_at(codes, 2 * (i + n))?)) };
        let (size, slots) = match op {
            0 => (8, 1),
            1 if op_info == 0 => (slot(1)? * 8, 2),
            1 => (slot(1)? | slot(2)? << 16, 3),
            2 => (op_info * 8 + 8, 1),
            4 | 8 => (0, 2),
            5 | 9 => (0, 3),
            _ => (0, 1),
        };
        if offset > done {
            moved += size;
        }
        i += slots;
    }
    Some(moved)
}

/// Unwinds one frame of the function the instruction pointer is in, in
/// `module` loaded at `base`: the registers as they were at the call into
/// it. `innermost` is the stack's first frame, which may be anywhere in its
/// function; the others are at calls.
fn unwind(module: &Module, base: u64, registers: &mut Registers, stack: &Stack, innermost: bool) -> Option<()> {
    let rva = u32::try_from(registers.rip.checked_sub(base)?).ok()?;
    let Some(entry) = module.functions.entry(rva) else {
        // A leaf function, which does not move the stack pointer.
        if !innermost {
            return None;
        }
        registers.rip = stack.u64(registers.r[RSP])?;
        registers.r[RSP] += 8;
        return Some(());
    };
    if innermost {
        if let Some(code) = module.image.at(rva, 16).or_else(|| module.image.at(rva, 8)) {
            if undo_epilog(code, registers, stack)? {
                return Some(());
            }
        }
    }
    let mut r = *registers;
    let (mut start, mut info) = (module.functions.starts[entry], module.functions.unwind[entry] & !1);
    let mut first = true;
    let mut machine_frame = false;
    for _ in 0..32 {
        let head = module.image.at(info, 4)?;
        let (flags, prolog, count) = (head[0] >> 3, u32::from(head[1]), usize::from(head[2]));
        let (frame_register, frame_offset) = (usize::from(head[3] & 15), u64::from(head[3] >> 4) * 16);
        let codes = module.image.at(info + 4, 2 * count)?;
        // Only the first entry's prolog can be part done.
        let done = if first { rva - start } else { u32::MAX };
        // What the saves are relative to: the stack pointer as the prolog
        // leaves it, which the frame register keeps once it is set; in a
        // prolog part done, less what it has yet to push and allocate.
        let frame = if frame_register != 0 && (done >= prolog || set_fpreg_done(codes, done)) {
            r.r[frame_register].wrapping_sub(frame_offset)
        } else if done < prolog {
            r.r[RSP].wrapping_sub(not_yet_moved(codes, done)?)
        } else {
            r.r[RSP]
        };
        let mut i = 0;
        while i < count {
            let (offset, op, op_info) = (u32::from(codes[2 * i]), codes[2 * i + 1] & 15, usize::from(codes[2 * i + 1] >> 4));
            let slot = |n: usize| -> Option<u64> { Some(u64::from(u16_at(codes, 2 * (i + n))?)) };
            let applies = done >= prolog || offset <= done;
            i += match op {
                // UWOP_PUSH_NONVOL
                0 => {
                    if applies {
                        r.r[op_info] = stack.u64(r.r[RSP])?;
                        r.r[RSP] += 8;
                    }
                    1
                }
                // UWOP_ALLOC_LARGE
                1 => {
                    let (size, slots) = if op_info == 0 { (slot(1)? * 8, 2) } else { (slot(1)? | slot(2)? << 16, 3) };
                    if applies {
                        r.r[RSP] += size;
                    }
                    slots
                }
                // UWOP_ALLOC_SMALL
                2 => {
                    if applies {
                        r.r[RSP] += op_info as u64 * 8 + 8;
                    }
                    1
                }
                // UWOP_SET_FPREG
                3 => {
                    if applies {
                        r.r[RSP] = r.r[frame_register].wrapping_sub(frame_offset);
                    }
                    1
                }
                // UWOP_SAVE_NONVOL, UWOP_SAVE_NONVOL_FAR
                4 | 5 => {
                    let (at, slots) = if op == 4 { (slot(1)? * 8, 2) } else { (slot(1)? | slot(2)? << 16, 3) };
                    if applies {
                        r.r[op_info] = stack.u64(frame + at)?;
                    }
                    slots
                }
                // UWOP_EPILOG (version 2): where an epilog is, which
                // undo_epilog finds by its instructions.
                6 => 1,
                // UWOP_SAVE_XMM128, UWOP_SAVE_XMM128_FAR: no integer register.
                8 => 2,
                9 => 3,
                // UWOP_PUSH_MACHFRAME: an interrupt's or exception's frame.
                10 => {
                    if applies {
                        let at = r.r[RSP] + if op_info == 1 { 8 } else { 0 };
                        r.rip = stack.u64(at)?;
                        r.r[RSP] = stack.u64(at + 24)?;
                        machine_frame = true;
                    }
                    1
                }
                _ => return None,
            };
        }
        if flags & CHAINED == 0 {
            break;
        }
        let count = count as u32;
        let chained = module.image.at(info + 4 + 2 * (count + (count & 1)), 12)?;
        (start, info) = (u32_at(chained, 0)?, u32_at(chained, 8)? & !1);
        first = false;
    }
    if !machine_frame {
        r.rip = stack.u64(r.r[RSP])?;
        r.r[RSP] += 8;
    }
    *registers = r;
    Some(())
}

/// The instruction pointer and return addresses of a stack, innermost
/// first, unwound from its registers through the modules' unwind data;
/// they end where an address lies in no module, or the unwind data cannot
/// go on.
fn frames(modules: &[Module], mut registers: Registers, stack: &Stack) -> Vec<u64> {
    let mut pcs = Vec::new();
    while pcs.len() < 200 && registers.rip != 0 {
        pcs.push(registers.rip);
        let Some(module) = modules.iter().find(|m| m.base <= registers.rip && registers.rip < m.end) else { break };
        let sp = registers.r[RSP];
        if unwind(module, module.base, &mut registers, stack, pcs.len() == 1).is_none() || registers.r[RSP] <= sp {
            break;
        }
    }
    pcs
}

// ---------- the process and its threads ----------

struct Module {
    base: u64,
    end: u64,
    name: String,
    image: Image,
    functions: Functions,
}

/// Creation time and CPU time (user and kernel), in 100 ns.
fn thread_times(thread: HANDLE) -> (u64, u64) {
    let value = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    let mut t: [FILETIME; 4] = unsafe { std::mem::zeroed() };
    let [c, e, k, u] = &mut t;
    unsafe { GetThreadTimes(thread, c, e, k, u) };
    (value(t[0]), value(t[2]) + value(t[3]))
}

fn threads_of(process: u32) -> Vec<(u32, HANDLE)> {
    let mut threads = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut more = Thread32First(snapshot, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == process {
                let thread = OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    threads.push((entry.th32ThreadID, thread));
                }
            }
            more = Thread32Next(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    threads
}

/// The process's modules, each loaded into dbghelp too.
fn modules_of(process: HANDLE) -> Vec<Module> {
    let mut modules = Vec::new();
    take_up_modules(process, &mut modules);
    modules
}

/// Adds the modules the process loaded since `modules` was made, each
/// loaded into dbghelp too.
fn take_up_modules(process: HANDLE, modules: &mut Vec<Module>) {
    let mut handles: Vec<HMODULE> = vec![std::ptr::null_mut(); 1024];
    let mut needed = 0;
    unsafe { EnumProcessModulesEx(process, handles.as_mut_ptr(), (handles.len() * size_of::<HMODULE>()) as u32, &mut needed, LIST_MODULES_ALL) };
    handles.truncate(needed as usize / size_of::<HMODULE>());
    let known: std::collections::HashSet<u64> = modules.iter().map(|m| m.base).collect();
    let new: Vec<Module> = handles
        .into_iter()
        .filter(|module| !known.contains(&(*module as u64)))
        .map(|module| unsafe {
            let mut info: MODULEINFO = std::mem::zeroed();
            GetModuleInformation(process, module, &mut info, size_of::<MODULEINFO>() as u32);
            let mut name = [0u16; 260];
            let len = GetModuleBaseNameW(process, module, name.as_mut_ptr(), name.len() as u32) as usize;
            let mut path = vec![0u16; 520];
            let path_len = GetModuleFileNameExW(process, module, path.as_mut_ptr(), path.len() as u32) as usize;
            path.truncate(path_len);
            let base = info.lpBaseOfDll as u64;
            path.push(0);
            SymLoadModuleExW(process, std::ptr::null_mut(), path.as_ptr(), std::ptr::null(), base, info.SizeOfImage, std::ptr::null(), 0);
            path.pop();
            let image = std::fs::read(String::from_utf16_lossy(&path)).ok().and_then(Image::read).unwrap_or_default();
            let functions = Functions::read(&image).unwrap_or_default();
            Module { base, end: base + u64::from(info.SizeOfImage), name: String::from_utf16_lossy(&name[..len]).to_lowercase(), image, functions }
        })
        .collect();
    modules.extend(new);
}

/// The most of a stack a sample copies.
const STACK_COPIED: u64 = 512 << 10;

/// A suspended thread's registers and a copy of its stack, from its stack
/// pointer to the stack's end, walked later.
fn capture(process: HANDLE, thread: HANDLE) -> Option<(Registers, Stack)> {
    // GetThreadContext fails on a context that is not 16-byte aligned.
    #[repr(C, align(16))]
    struct Aligned(CONTEXT);
    let mut aligned = Aligned(unsafe { std::mem::zeroed() });
    let c = &mut aligned.0;
    c.ContextFlags = CONTEXT_FULL_AMD64;
    if unsafe { GetThreadContext(thread, c) } == 0 {
        return None;
    }
    let registers = Registers { rip: c.Rip, r: [c.Rax, c.Rcx, c.Rdx, c.Rbx, c.Rsp, c.Rbp, c.Rsi, c.Rdi, c.R8, c.R9, c.R10, c.R11, c.R12, c.R13, c.R14, c.R15] };
    // The committed pages from the stack pointer up end where the stack does.
    let mut region: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { VirtualQueryEx(process, c.Rsp as *const _, &mut region, size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
        return None;
    }
    let end = (region.BaseAddress as u64 + region.RegionSize as u64).min(c.Rsp + STACK_COPIED);
    let mut bytes = vec![0u8; usize::try_from(end.checked_sub(c.Rsp)?).ok()?];
    let mut got = 0;
    let ok = unsafe { ReadProcessMemory(process, c.Rsp as *const _, bytes.as_mut_ptr().cast(), bytes.len(), &mut got) };
    if ok == 0 {
        return None;
    }
    bytes.truncate(got);
    Some((registers, Stack { from: c.Rsp, bytes }))
}

/// Whether a stack's thread was running, in user mode, rather than in the
/// kernel, where a thread that waits is: its instruction pointer right after
/// a `syscall`.
fn running(modules: &[Module], pcs: &[u64]) -> bool {
    let Some(&rip) = pcs.first() else { return false };
    let Some(module) = modules.iter().find(|m| m.base <= rip && rip < m.end) else { return true };
    let rva = (rip - module.base) as u32;
    rva < 2 || module.image.at(rva - 2, 2) != Some(&[0x0f, 0x05][..])
}

/// Whether a running thread's innermost frame lies in one of `waiting_in`,
/// functions in which a thread waits without sleeping.
fn spins(modules: &[Module], pcs: &[u64], waiting_in: &[(String, u32)]) -> bool {
    let Some(&rip) = pcs.first() else { return false };
    let Some(module) = modules.iter().find(|m| m.base <= rip && rip < m.end) else { return false };
    let Some(function) = module.functions.of((rip - module.base) as u32) else { return false };
    waiting_in.iter().any(|(m, f)| *m == module.name && *f == function)
}

/// A tick's wait shared out evenly among the stacks that ran at it; with
/// none, the whole of it to none.
fn share_out<K>(tick: f64, ran: Vec<K>) -> Vec<(Option<K>, f64)> {
    if ran.is_empty() {
        return vec![(None, tick)];
    }
    let each = tick / ran.len() as f64;
    ran.into_iter().map(|k| (Some(k), each)).collect()
}

/// The CPU cycles the thread ran.
fn cycles(thread: HANDLE) -> u64 {
    let mut n = 0;
    unsafe { QueryThreadCycleTime(thread, &mut n) };
    n
}

fn read<const N: usize>(process: HANDLE, address: u64) -> Option<[u8; N]> {
    let mut buffer = [0u8; N];
    let mut got = 0;
    let ok = unsafe { ReadProcessMemory(process, address as *const _, buffer.as_mut_ptr().cast(), N, &mut got) };
    (ok != 0 && got == N).then_some(buffer)
}

/// The date (hours), whether a day runs and whether a frame runs inside it.
fn world_state(process: HANDLE, exe_base: u64) -> Option<(i32, bool, bool)> {
    let world = u64::from_le_bytes(read(process, exe_base + WORLD as u64)?);
    if world == 0 {
        return None;
    }
    let date = i32::from_le_bytes(read(process, world + WORLD_DATE as u64)?);
    let flags: [u8; 9] = read(process, world + WORLD_DAY as u64)?;
    Some((date, flags[0] != 0, flags[8] != 0))
}

/// The application's frame count.
fn frame_count(process: HANDLE, exe_base: u64) -> Option<u64> {
    let application = u64::from_le_bytes(read(process, exe_base + APPLICATION as u64)?);
    if application == 0 {
        return None;
    }
    Some(u64::from_le_bytes(read(process, application + APPLICATION_FRAMES as u64)?))
}

/// Where in the game's time the sample fell, by the world's flags.
fn tag(state: Option<(i32, bool, bool)>) -> &'static str {
    match state {
        None => "[no world]",
        Some((_, _, true)) => "[frame inside a day]",
        Some((_, true, _)) => "[day]",
        Some(_) => "[between days]",
    }
}

/// `0x758430`, the RVA of a function of the exe; elsewhere the module and
/// the function's export, or the module and the function's RVA. A return
/// address is looked up one byte back, in the call.
fn name(process: HANDLE, modules: &[Module], at: u64) -> String {
    let Some(module) = modules.iter().find(|m| m.base <= at && at < m.end) else { return "?".into() };
    let function = module.functions.of((at - module.base) as u32);
    if module.name == "eu4.exe" {
        return function.map_or_else(|| format!("eu4.exe+0x{:x}", at - module.base), |f| format!("0x{f:x}"));
    }
    let start = function.map_or(at, |f| module.base + u64::from(f));
    // SYMBOL_INFOW with room for a name of 255 characters after it.
    let mut buffer = [0u64; (size_of::<SYMBOL_INFOW>() + 2 * 256).div_ceil(8)];
    let symbol = buffer.as_mut_ptr().cast::<SYMBOL_INFOW>();
    let mut displacement = 0;
    unsafe {
        (*symbol).SizeOfStruct = size_of::<SYMBOL_INFOW>() as u32;
        (*symbol).MaxNameLen = 255;
        if SymFromAddrW(process, start, &mut displacement, symbol) != 0 && displacement == 0 {
            let len = (*symbol).NameLen.min(255) as usize;
            let name = std::slice::from_raw_parts((&raw const (*symbol).Name).cast::<u16>(), len);
            return format!("{}!{}", module.name, String::from_utf16_lossy(name));
        }
    }
    format!("{}+0x{:x}", module.name, start - module.base)
}

/// `[where]: part`, for a stack innermost first with its tag last.
fn part(frames: &[String]) -> String {
    let (where_, rest) = frames.split_last().expect("every stack has its tag");
    let mut frames: Vec<&str> = rest.iter().rev().map(String::as_str).collect();
    if where_ == "[frame inside a day]" {
        if let Some(at) = frames.iter().rposition(|f| *f == FRAME_IN_DAY) {
            frames.drain(..at);
        }
    }
    let parts: &[(&str, &str)] = if where_ == "[day]" { &DAY } else { &FRAME };
    for (label, f) in parts {
        if frames.iter().any(|fr| fr.starts_with(f)) {
            return format!("{where_}: {label}");
        }
    }
    format!("{where_}: other")
}

/// The thread's name, as the game gave it with SetThreadDescription.
fn description(thread: HANDLE) -> String {
    let mut text: windows_sys::core::PWSTR = std::ptr::null_mut();
    if unsafe { GetThreadDescription(thread, &mut text) } < 0 || text.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    let name = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
    unsafe { LocalFree(text as _) };
    name
}

/// A thread other than the main thread, with `--threads`: the CPU cycles it
/// had run at its last sample.
struct Other {
    id: u32,
    handle: HANDLE,
    cycles: u64,
}

/// `[main 123]`, `[LoadingThread 456]` or `[thread 789]`: a thread as the
/// root of its stacks.
fn thread_frame(id: u32, main: bool, name: &str) -> String {
    match (main, name.is_empty()) {
        (true, _) => format!("[main {id}]"),
        (false, true) => format!("[thread {id}]"),
        (false, false) => format!("[{name} {id}]"),
    }
}

/// A wait of the main thread's: the stretch of the run, where in the game's
/// time, the main thread's stack, and the stack of a thread that ran
/// meanwhile, or none.
type Wait = (Option<u32>, &'static str, Vec<u64>, Option<Vec<u64>>);

fn sample(out: &str, seconds: f64, hz: f64, slice: Option<f64>, wait: bool, all: bool, waiting_in: &[(String, u32)]) -> Result<()> {
    let waited = Instant::now();
    let id = loop {
        match eu4_harmony_client::game::processes()?.first() {
            Some(&id) => break id,
            None if wait && waited.elapsed() < Duration::from_secs(300) => std::thread::sleep(Duration::from_millis(20)),
            None => return Err("eu4.exe is not running".into()),
        }
    };
    let process = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, id) };
    if process.is_null() {
        return Err(format!("cannot open eu4.exe ({id})"));
    }
    let threads = threads_of(id);
    let &(main_id, main) = threads.iter().min_by_key(|(_, t)| thread_times(*t).0).ok_or("eu4.exe has no thread to open")?;
    unsafe {
        SymSetOptions(SYMOPT_DEFERRED_LOADS | SYMOPT_UNDNAME);
        if SymInitializeW(process, std::ptr::null(), 0) == 0 {
            return Err(format!("SymInitialize failed: {}", std::io::Error::last_os_error()));
        }
    }
    let mut modules = modules_of(process);
    let exe_base = modules.iter().find(|m| m.name == "eu4.exe").ok_or("eu4.exe has no module eu4.exe")?.base;

    let before: HashMap<u32, (u64, u64)> = threads.iter().map(|&(t, h)| (t, (thread_times(h).1, cycles(h)))).collect();
    let start_state = world_state(process, exe_base);
    let mut samples = Tally::new();
    // With --threads, every thread's samples taken as it ran, and each
    // stretch's ticks.
    let (mut thread_samples, mut stretch_ticks, mut waiting) = (Tally::new(), HashMap::new(), 0u64);
    // With --threads, the microseconds the main thread waited, by its stack
    // and the stack of each thread that ran meanwhile (none: no thread ran).
    let mut waited: HashMap<Wait, f64> = HashMap::new();
    let tick = 1e6 / hz;
    // With --slice, each stretch's first and last sample.
    let mut stretches: BTreeMap<u32, (Mark, Mark)> = BTreeMap::new();
    let interval = Duration::from_secs_f64(1.0 / hz);
    let (mut suspended, mut count) = (Duration::ZERO, 0u64);
    let (mut held, mut held_count) = (Duration::ZERO, 0u64);
    let began = Instant::now();
    let mut next = began;
    let mut looked = began;
    let mut others: Vec<Other> = Vec::new();
    while began.elapsed().as_secs_f64() < seconds {
        if looked.elapsed() >= Duration::from_secs(1) || (all && others.is_empty()) {
            take_up_modules(process, &mut modules);
            if all {
                for (t, h) in threads_of(id) {
                    if t == main_id || others.iter().any(|o| o.id == t) {
                        unsafe { CloseHandle(h) };
                    } else {
                        let cycles = before.get(&t).map_or_else(|| cycles(h), |b| b.1);
                        others.push(Other { id: t, handle: h, cycles });
                    }
                }
            }
            looked = Instant::now();
        }
        let now = Instant::now();
        if now < next {
            std::thread::sleep((next - now).saturating_sub(Duration::from_micros(500)));
            continue;
        }
        // A tick that came late is not made up for.
        next = (next + interval).max(now);
        let t0 = Instant::now();
        if unsafe { SuspendThread(main) } == u32::MAX {
            break;
        }
        let captured = capture(process, main);
        let state = world_state(process, exe_base);
        unsafe { ResumeThread(main) };
        suspended += t0.elapsed();
        count += 1;
        let pcs = captured.map(|(registers, stack)| frames(&modules, registers, &stack)).unwrap_or_default();
        let at = began.elapsed().as_secs_f64();
        let stretch = slice.map(|slice| (at / slice).floor() as u32);
        if let (Some(n), Some((date, ..)), Some(frames)) = (stretch, state, frame_count(process, exe_base)) {
            let mark = Mark { at, date, frames };
            stretches.entry(n).and_modify(|(_, last)| *last = mark).or_insert((mark, mark));
        }
        if all {
            if let Some(stretch) = stretch {
                *stretch_ticks.entry(stretch).or_insert(0u64) += 1;
            }
            let main_runs = running(&modules, &pcs);
            match main_runs {
                true => thread_samples.add((stretch, main_id, tag(state), pcs.clone()), 1),
                false => waiting += 1,
            }
            let mut ran = Vec::new();
            others.retain_mut(|other| {
                let now = cycles(other.handle);
                if now <= other.cycles {
                    return true;
                }
                let t0 = Instant::now();
                if unsafe { SuspendThread(other.handle) } == u32::MAX {
                    // The thread ended.
                    unsafe { CloseHandle(other.handle) };
                    return false;
                }
                let captured = capture(process, other.handle);
                unsafe { ResumeThread(other.handle) };
                held += t0.elapsed();
                held_count += 1;
                let pcs = captured.map(|(registers, stack)| frames(&modules, registers, &stack)).unwrap_or_default();
                match running(&modules, &pcs) {
                    true => {
                        if !spins(&modules, &pcs, waiting_in) {
                            ran.push(pcs.clone());
                        }
                        thread_samples.add((stretch, other.id, tag(state), pcs), 1);
                    }
                    false => waiting += 1,
                }
                other.cycles = now;
                true
            });
            if !main_runs || spins(&modules, &pcs, waiting_in) {
                for (other, us) in share_out(tick, ran) {
                    *waited.entry((stretch, tag(state), pcs.clone(), other)).or_insert(0.0) += us;
                }
            }
        }
        samples.add((tag(state), stretch, pcs), 1);
    }
    let wall = began.elapsed().as_secs_f64();
    let end_state = world_state(process, exe_base);
    take_up_modules(process, &mut modules);
    // The threads at the end, those the process started during the run
    // with all their time.
    let ended = threads_of(id);
    let after: HashMap<u32, (u64, u64)> = ended.iter().map(|&(t, h)| (t, (thread_times(h).1, cycles(h)))).collect();
    let names_of_threads: HashMap<u32, String> = ended.iter().map(|&(t, h)| (t, description(h))).collect();

    let mut names: HashMap<u64, String> = HashMap::new();
    let mut frames_of = |pcs: &[u64]| -> Vec<String> {
        pcs.iter()
            .enumerate()
            .map(|(i, &pc)| {
                let at = if i == 0 { pc } else { pc - 1 };
                names.entry(at).or_insert_with(|| name(process, &modules, at)).clone()
            })
            .collect()
    };
    let (mut stacks, mut flags) = (Tally::new(), Tally::new());
    for ((tag, stretch, pcs), n) in &samples.counts {
        let mut frames = frames_of(pcs);
        frames.push(tag.to_string());
        stacks.add((frames, *stretch), *n);
        flags.add(*tag, *n);
    }
    // Each thread's stacks, and the whole game's with the threads merged.
    let (mut thread_stacks, mut game_stacks, mut stretch_cpu) = (Tally::new(), Tally::new(), HashMap::new());
    for ((stretch, thread, tag, pcs), n) in &thread_samples.counts {
        let mut frames = frames_of(pcs);
        frames.push(tag.to_string());
        let mut game = frames.clone();
        frames.push(thread_frame(*thread, *thread == main_id, names_of_threads.get(thread).map_or("", String::as_str)));
        if let (Some(stretch), Some(slice)) = (stretch, slice) {
            frames.push(format!("[{}]", seconds_of(*stretch, slice)));
            game.push(format!("[{}]", seconds_of(*stretch, slice)));
            *stretch_cpu.entry(*stretch).or_insert(0) += n;
        }
        thread_stacks.add(frames, *n);
        game_stacks.add(game, *n);
    }
    // What the other threads ran while the main thread waited, rooted in
    // the part it waited in.
    let (mut waited_stacks, mut waited_parts): (HashMap<Vec<String>, f64>, HashMap<String, f64>) = (HashMap::new(), HashMap::new());
    for ((stretch, tag, main_pcs, other), us) in &waited {
        let mut main_frames = frames_of(main_pcs);
        main_frames.push(tag.to_string());
        let mut frames = other.as_ref().map_or_else(|| vec!["[nothing ran]".to_string()], |pcs| frames_of(pcs));
        frames.push(part(&main_frames));
        if let (Some(stretch), Some(slice)) = (stretch, slice) {
            frames.push(format!("[{}]", seconds_of(*stretch, slice)));
        }
        *waited_stacks.entry(frames).or_insert(0.0) += us;
        *waited_parts.entry(part(&main_frames)).or_insert(0.0) += us;
    }
    for (_, thread) in threads.iter().chain(&ended) {
        unsafe { CloseHandle(*thread) };
    }
    for other in &others {
        unsafe { CloseHandle(other.handle) };
    }
    unsafe { CloseHandle(process) };

    // The root of each folded stack is its tag, and with --slice its stretch.
    let folded: String = stacks
        .most()
        .iter()
        .map(|((frames, stretch), n)| {
            let mut frames: Vec<String> = frames.iter().rev().cloned().collect();
            if let (Some(stretch), Some(slice)) = (stretch, slice) {
                frames[0] = format!("{} {}", frames[0], seconds_of(*stretch, slice));
            }
            format!("{} {n}\n", frames.join(";"))
        })
        .collect();
    std::fs::write(format!("{out}.folded"), folded).map_err(|e| format!("cannot write {out}.folded: {e}"))?;
    if all {
        for (stacks, file) in [(&thread_stacks, "threads"), (&game_stacks, "game")] {
            let folded: String = stacks
                .most()
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(frames, n)| format!("{} {n}\n", frames.iter().rev().cloned().collect::<Vec<_>>().join(";")))
                .collect();
            std::fs::write(format!("{out}.{file}.folded"), folded).map_err(|e| format!("cannot write {out}.{file}.folded: {e}"))?;
        }
        let mut stacks: Vec<_> = waited_stacks.iter().map(|(frames, us)| (frames, us.round() as u64)).filter(|(_, us)| *us > 0).collect();
        stacks.sort_by_key(|(_, us)| std::cmp::Reverse(*us));
        let folded: String = stacks.iter().map(|(frames, us)| format!("{} {us}\n", frames.iter().rev().cloned().collect::<Vec<_>>().join(";"))).collect();
        std::fs::write(format!("{out}.waited.folded"), folded).map_err(|e| format!("cannot write {out}.waited.folded: {e}"))?;
    }

    let (mut inclusive, mut own, mut parts, mut stretch_parts) = (Tally::new(), Tally::new(), HashMap::new(), HashMap::new());
    for ((frames, stretch), n) in &stacks.counts {
        own.add(frames[0].clone(), *n);
        let mut seen = std::collections::HashSet::new();
        for frame in frames {
            if seen.insert(frame) {
                inclusive.add(frame.clone(), *n);
            }
        }
        *parts.entry(part(frames)).or_insert(0) += n;
        if let Some(stretch) = stretch {
            *stretch_parts.entry((*stretch, part(frames))).or_insert(0) += n;
        }
    }
    let framework: u64 = stacks.counts.iter().filter(|((f, _), _)| f.iter().any(|fr| fr.starts_with("eu4_harmony_core.dll"))).map(|(_, n)| n).sum();
    if let Some(slice) = slice {
        // With --threads, the cores the whole game ran in each stretch.
        let cores: HashMap<u32, f64> =
            stretch_cpu.iter().map(|(&stretch, &n)| (stretch, n as f64 / stretch_ticks.get(&stretch).copied().unwrap_or(1).max(1) as f64)).collect();
        let table = stretch_table(&stretches, &stretch_parts, all.then_some(&cores), slice);
        std::fs::write(format!("{out}.slices.tsv"), table).map_err(|e| format!("cannot write {out}.slices.tsv: {e}"))?;
    }

    let total = count.max(1) as f64;
    let share = |n: u64| 100.0 * n as f64 / total;
    let mut lines = vec![format!(
        "{count} samples in {wall:.1} s ({:.0} a second); the main thread (id {main_id}) was suspended {:.1}% of the time, {:.3} ms a sample",
        count as f64 / wall,
        100.0 * suspended.as_secs_f64() / wall,
        1000.0 * suspended.as_secs_f64() / total
    )];
    if all {
        lines.push(format!(
            "{held_count} samples of the other threads, each holding its thread {:.3} ms; of every thread's, {waiting} found it waiting in the kernel",
            1000.0 * held.as_secs_f64() / held_count.max(1) as f64
        ));
    }
    if let (Some(start), Some(end)) = (start_state, end_state) {
        let days = f64::from(end.0 - start.0) / 24.0;
        lines.push(format!("{days:.0} days passed, {:.2} a second", days / wall));
    }
    lines.extend(["".into(), "Samples by where in the game's time (the world's flags):".into()]);
    lines.extend(flags.most().iter().map(|(k, n)| format!("  {k}: {:.1}%", share(*n))));
    lines.extend(["".into(), "Samples by part:".into()]);
    let mut parts: Vec<_> = parts.into_iter().collect();
    parts.sort();
    lines.extend(parts.iter().map(|(label, n)| format!("  {:5.1}%  {label}", share(*n))));
    lines.push(format!("  ({:.1}% of samples have the framework's core on the stack)", share(framework)));
    lines.extend(["".into(), "Most samples inside (inclusive):".into()]);
    lines.extend(inclusive.most().iter().take(60).map(|(f, n)| format!("  {:5.1}%  {f}", share(*n))));
    lines.extend(["".into(), "Most samples of their own (self):".into()]);
    lines.extend(own.most().iter().take(60).map(|(f, n)| format!("  {:5.1}%  {f}", share(*n))));
    lines.extend(["".into(), "CPU of each thread over the run, in cores (busiest first):".into()]);
    let mut cpu: Vec<(f64, u32)> = ended.iter().map(|(t, _)| ((after[t].0 - before.get(t).map_or(0, |b| b.0)) as f64 / 1e7 / wall, *t)).collect();
    cpu.sort_by(|a, b| b.partial_cmp(a).expect("CPU times are numbers"));
    for (c, t) in cpu.iter().filter(|(c, _)| *c >= 0.005) {
        lines.push(format!("  {c:5.2}  {}", thread_frame(*t, *t == main_id, &names_of_threads[t])));
    }
    lines.push(format!("  {:5.2}  in all", cpu.iter().map(|(c, _)| c).sum::<f64>()));
    if all {
        // The whole game's functions, every thread's merged, in cores: the
        // CPU their samples stand for over the run.
        let (mut inclusive, mut own) = (Tally::new(), Tally::new());
        for (frames, n) in &game_stacks.counts {
            own.add(frames[0].clone(), *n);
            let mut seen = std::collections::HashSet::new();
            for frame in frames.iter().filter(|f| !f.starts_with('[')) {
                if seen.insert(frame) {
                    inclusive.add(frame.clone(), *n);
                }
            }
        }
        // Each tick samples every thread that runs, so a function's samples
        // over the ticks are the cores it ran on.
        let cores = |n: u64| n as f64 / total;
        let sampled: u64 = game_stacks.counts.iter().map(|(_, n)| n).sum();
        lines.extend([
            "".into(),
            format!("The whole game's CPU inside each function, every thread's merged, in cores of the {:.2} sampled (inclusive):", cores(sampled)),
        ]);
        lines.extend(inclusive.most().iter().take(60).map(|(f, n)| format!("  {:5.2}  {f}", cores(*n))));
        lines.extend(["".into(), "The whole game's CPU in each function's own code, in cores (self):".into()]);
        lines.extend(own.most().iter().take(60).map(|(f, n)| format!("  {:5.2}  {f}", cores(*n))));
        // While the main thread waited: where, and on what, in milliseconds
        // a second of the run.
        let ms = |us: u64| us as f64 / 1000.0 / wall;
        let mut parts = Tally::new();
        for (p, us) in &waited_parts {
            parts.add(p.clone(), us.round() as u64);
        }
        lines.extend(["".into(), "The main thread's waits, by the part it waited in, in ms a second of the run:".into()]);
        lines.extend(parts.most().iter().map(|(p, us)| format!("  {:6.1}  {p}", ms(*us))));
        let (mut inclusive, mut own) = (Tally::new(), Tally::new());
        for (frames, us) in &waited_stacks {
            let us = us.round() as u64;
            own.add(frames[0].clone(), us);
            let mut seen = std::collections::HashSet::new();
            for frame in frames.iter().filter(|f| !f.starts_with('[') || *f == "[nothing ran]") {
                if seen.insert(frame) {
                    inclusive.add(frame.clone(), us);
                }
            }
        }
        lines.extend(["".into(), "What ran while the main thread waited, each wait shared out among the threads that ran, in ms a second (inclusive):".into()]);
        lines.extend(inclusive.most().iter().take(60).map(|(f, us)| format!("  {:6.1}  {f}", ms(*us))));
        lines.extend(["".into(), "The same in each function's own code (self):".into()]);
        lines.extend(own.most().iter().take(40).map(|(f, us)| format!("  {:6.1}  {f}", ms(*us))));
    }
    std::fs::write(format!("{out}.txt"), lines.join("\n") + "\n").map_err(|e| format!("cannot write {out}.txt: {e}"))?;
    println!("{}", lines[..lines.len().min(40)].join("\n"));
    Ok(())
}

/// A sample's time into the run, the game's date (hours) and the frame count.
#[derive(Clone, Copy)]
struct Mark {
    at: f64,
    date: i32,
    frames: u64,
}

/// `0-60 s`, the seconds of a stretch of the run.
fn seconds_of(stretch: u32, slice: f64) -> String {
    format!("{}-{} s", f64::from(stretch) * slice, f64::from(stretch + 1) * slice)
}

/// One line for each stretch of the run, tab-separated: its seconds, the
/// dates of its first and last sample, days and frames a second between
/// them, with `--threads` the cores the whole game ran, and the share of
/// the main thread's samples in each part.
fn stretch_table(stretches: &BTreeMap<u32, (Mark, Mark)>, parts: &HashMap<(u32, String), u64>, cores: Option<&HashMap<u32, f64>>, slice: f64) -> String {
    let mut labels: Vec<&String> = parts.keys().map(|(_, label)| label).collect();
    labels.sort();
    labels.dedup();
    let mut lines = vec![["seconds", "from", "to", "days a second", "frames a second"]
        .iter()
        .chain(cores.map(|_| &"the game's cores"))
        .map(|s| s.to_string())
        .chain(labels.iter().map(|l| l.to_string()))
        .collect::<Vec<_>>()
        .join("\t")];
    for (&stretch, (first, last)) in stretches {
        let total: u64 = parts.iter().filter(|((s, _), _)| *s == stretch).map(|(_, n)| n).sum();
        let span = last.at - first.at;
        let rate = |n: f64| if span > 0.0 { format!("{:.2}", n / span) } else { String::new() };
        let mut row = vec![
            seconds_of(stretch, slice),
            eu4_harmony_remote::Date(first.date).to_string(),
            eu4_harmony_remote::Date(last.date).to_string(),
            rate(f64::from(last.date - first.date) / 24.0),
            rate((last.frames - first.frames) as f64),
        ];
        if let Some(cores) = cores {
            row.push(format!("{:.2}", cores.get(&stretch).copied().unwrap_or(0.0)));
        }
        row.extend(labels.iter().map(|label| {
            let n = parts.get(&(stretch, (*label).clone())).copied().unwrap_or(0);
            format!("{:.1}", 100.0 * n as f64 / total.max(1) as f64)
        }));
        lines.push(row.join("\t"));
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(text: &str) -> Vec<String> {
        text.split(';').map(String::from).collect()
    }

    #[test]
    fn a_stack_counts_as_the_first_part_on_it() {
        // Innermost first, the tag last.
        assert_eq!(part(&frames("0x1;0x75b4b0;0x2ee280;[day]")), "[day]: each country's daily update");
        assert_eq!(part(&frames("0x1;0x5cbb00;0x769ea0;0x8183b0;[frame inside a day]")), "[frame inside a day]: the autosave");
        assert_eq!(part(&frames("0x1;pops_api.dll+0x10;0x8137c0;[between days]")), "[between days]: POPS");
        assert_eq!(part(&frames("0x1;[between days]")), "[between days]: other");
    }

    #[test]
    fn each_stretch_has_its_dates_rates_and_shares() {
        // 1444.11.11 and ten days later, in hours.
        let date = eu4_harmony_remote::Date::parse("1444.11.11").unwrap().0;
        let stretches = BTreeMap::from([(1, (Mark { at: 60.0, date, frames: 100 }, Mark { at: 62.0, date: date + 240, frames: 140 }))]);
        let parts = HashMap::from([((1, "[day]: the AI".to_string()), 3), ((1, "[between days]: drawing".to_string()), 1)]);
        assert_eq!(
            stretch_table(&stretches, &parts, None, 60.0),
            "seconds\tfrom\tto\tdays a second\tframes a second\t[between days]: drawing\t[day]: the AI\n\
             60-120 s\t1444.11.11\t1444.11.21\t5.00\t20.00\t25.0\t75.0\n"
        );
        let cores = HashMap::from([(1, 3.25)]);
        assert_eq!(
            stretch_table(&stretches, &parts, Some(&cores), 60.0),
            "seconds\tfrom\tto\tdays a second\tframes a second\tthe game's cores\t[between days]: drawing\t[day]: the AI\n\
             60-120 s\t1444.11.11\t1444.11.21\t5.00\t20.00\t3.25\t25.0\t75.0\n"
        );
    }

    const BASE: u64 = 0x1_4000_0000;

    /// A module of two functions: one at 0x100 whose prolog is
    /// `mov [rsp+8], rbx; push rdi; sub rsp, 0x20` and whose epilog at 0x1f0
    /// is `add rsp, 0x20; pop rdi; ret`; one at 0x300 that keeps its frame
    /// in rbp: `push rbp; sub rsp, 0x40; lea rbp, [rsp+0x20]`.
    fn module() -> Module {
        let mut data = vec![0u8; 0x400];
        // The first's unwind data: version 1, a prolog of 10 bytes, 4 slots.
        data[0x10..0x1c].copy_from_slice(&[1, 0x0a, 4, 0, 0x0a, 3 << 4 | 2, 0x06, 7 << 4, 0x05, 3 << 4 | 4, 6, 0]);
        // The second's: rbp at 2 × 16 above the stack pointer.
        data[0x40..0x4a].copy_from_slice(&[1, 0x0a, 3, 2 << 4 | 5, 0x0a, 3, 0x05, 7 << 4 | 2, 0x01, 5 << 4]);
        data[0x100..0x10a].copy_from_slice(&[0x48, 0x89, 0x5c, 0x24, 0x08, 0x57, 0x48, 0x83, 0xec, 0x20]);
        data[0x1f0..0x1f6].copy_from_slice(&[0x48, 0x83, 0xc4, 0x20, 0x5f, 0xc3]);
        Module {
            base: BASE,
            end: BASE + 0x400,
            name: "eu4.exe".into(),
            image: Image { data, sections: vec![(0, 0x400, 0)] },
            functions: Functions { starts: vec![0x100, 0x300], ends: vec![0x200, 0x400], unwind: vec![0x10, 0x40], primary: vec![0x100, 0x300] },
        }
    }

    /// A stack from `from` with these values at these addresses.
    fn stack(from: u64, len: usize, values: &[(u64, u64)]) -> Stack {
        let mut bytes = vec![0u8; len];
        for &(at, value) in values {
            let at = (at - from) as usize;
            bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
        }
        Stack { from, bytes }
    }

    fn registers(rip: u64, rsp: u64, others: &[(usize, u64)]) -> Registers {
        let mut r = Registers { rip, r: [0; 16] };
        r.r[RSP] = rsp;
        for &(i, v) in others {
            r.r[i] = v;
        }
        r
    }

    #[test]
    fn a_frame_is_unwound_by_its_functions_unwind_data() {
        let (module, caller) = (module(), 0x7ff0_1234);
        // rdi pushed at 0x1020, the return address at 0x1028, rbx saved in
        // the caller's space at 0x1030.
        let stack = stack(0x1000, 0x40, &[(0x1020, 0x77), (0x1028, caller), (0x1030, 0x33)]);
        let mut r = registers(BASE + 0x120, 0x1000, &[]);
        unwind(&module, BASE, &mut r, &stack, true).unwrap();
        assert_eq!(r, registers(caller, 0x1030, &[(3, 0x33), (7, 0x77)]));

        // Stopped after `push rdi`, before `sub rsp`: the allocation is not
        // undone, and the save is found where the prolog will leave it.
        let mut r = registers(BASE + 0x106, 0x1020, &[]);
        unwind(&module, BASE, &mut r, &stack, true).unwrap();
        assert_eq!(r, registers(caller, 0x1030, &[(3, 0x33), (7, 0x77)]));

        // In the epilog: its own instructions are followed.
        let mut r = registers(BASE + 0x1f0, 0x1000, &[(3, 0x33)]);
        unwind(&module, BASE, &mut r, &stack, true).unwrap();
        assert_eq!(r, registers(caller, 0x1030, &[(3, 0x33), (7, 0x77)]));
    }

    #[test]
    fn a_frame_kept_in_a_register_is_unwound_through_it() {
        let (module, caller) = (module(), 0x7ff0_1234);
        // The prolog left the stack pointer at 0x2000 and rbp at 0x2020; the
        // function then took more stack, to 0x1f00.
        let stack = stack(0x1f00, 0x200, &[(0x2040, 0x5555), (0x2048, caller)]);
        let mut r = registers(BASE + 0x320, 0x1f00, &[(5, 0x2020)]);
        unwind(&module, BASE, &mut r, &stack, true).unwrap();
        assert_eq!(r, registers(caller, 0x2050, &[(5, 0x5555)]));
    }

    #[test]
    fn a_stack_is_walked_until_an_address_lies_in_no_module() {
        let module = module();
        let stack = stack(0x1000, 0x40, &[(0x1000, BASE + 0x130), (0x1028, 0x77), (0x1030, 0x7ff0_1234)]);
        // A leaf function at 0x250 returns into the first function, which
        // returns out of the module.
        let pcs = super::frames(std::slice::from_ref(&module), registers(BASE + 0x250, 0x1000, &[]), &stack);
        assert_eq!(pcs, [BASE + 0x250, BASE + 0x130, 0x7ff0_1234]);
    }

    #[test]
    fn a_thread_is_named_by_its_name_or_number() {
        assert_eq!(thread_frame(1, true, "SDL"), "[main 1]");
        assert_eq!(thread_frame(2, false, "LoadingThread"), "[LoadingThread 2]");
        assert_eq!(thread_frame(3, false, ""), "[thread 3]");
    }

    #[test]
    fn a_wait_is_shared_out_among_the_threads_that_ran() {
        assert_eq!(share_out(20_000.0, vec!["a", "b"]), [(Some("a"), 10_000.0), (Some("b"), 10_000.0)]);
        assert_eq!(share_out(20_000.0, vec!["a"]), [(Some("a"), 20_000.0)]);
        assert_eq!(share_out::<&str>(20_000.0, vec![]), [(None, 20_000.0)]);
    }

    #[test]
    fn waiting_functions_are_named_as_the_stacks_name_them() {
        assert_eq!(waiting_functions("0x179e90, ntdll.dll+0x1234").unwrap(), [("eu4.exe".to_string(), 0x179e90), ("ntdll.dll".to_string(), 0x1234)]);
        assert!(waiting_functions("SwitchToThread").is_err());
        let modules = [module()];
        let waiting_in = [("eu4.exe".to_string(), 0x100)];
        // In the first function, in the second, and in no function.
        assert!(spins(&modules, &[BASE + 0x120], &waiting_in));
        assert!(!spins(&modules, &[BASE + 0x320], &waiting_in));
        assert!(!spins(&modules, &[BASE + 0x250], &waiting_in));
    }

    #[test]
    fn ties_keep_the_order_first_met() {
        let mut tally = Tally::new();
        for (k, n) in [("a", 1), ("b", 2), ("c", 2), ("a", 0)] {
            tally.add(k, n);
        }
        assert_eq!(tally.most().iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["b", "c", "a"]);
    }
}
