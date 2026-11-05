//! `cargo xtask profile`: samples the running game's main thread, its call
//! stack at a steady rate, with Windows' own dbghelp.dll and no debugger to
//! install.
//!
//! It finds eu4.exe, suspends its main thread `--hz` times a second, walks
//! its call stack, reads the date and the world's flags, and resumes it. It
//! writes:
//!
//! - `<out>.folded`: one line per distinct stack, root first,
//!   `frame;frame;... count`, which flame graph tools read;
//! - `<out>.txt`: how many days passed; the share of samples inside a day,
//!   inside a frame that runs inside a day, and between days, and of the
//!   parts of each (as docs/research/main-thread.md counts them); the
//!   functions with the most samples inside them and with the most of their
//!   own; each thread's CPU.
//!
//! Each stack's root is where in the game's time it fell: `[day]`,
//! `[frame inside a day]` or `[between days]`. `--tree` prints the stacks of
//! a .folded file as a tree, top down, or the tree below one frame.
//!
//! Frames are named by the function they lie in, from the exception
//! directory of the module's file: in eu4.exe by the function's RVA,
//! elsewhere by module and export, or module and RVA where the function is
//! not exported.
//!
//! Suspending the thread for each walk slows the game, about 1.4 ms a sample
//! (the report gives it): at 50 samples a second the game runs about 7%
//! slower, at 200 about 30%, and a slower day brings more frames inside
//! days. Measure shares at 50.
use crate::{Args, Result};
use std::{
    collections::HashMap,
    hash::Hash,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE, HMODULE},
    System::{
        Diagnostics::{
            Debug::{
                GetThreadContext, ReadProcessMemory, StackWalk64, SymFromAddrW, SymFunctionTableAccess64, SymGetModuleBase64, SymInitializeW, SymLoadModuleExW,
                SymSetOptions, CONTEXT, CONTEXT_FULL_AMD64, STACKFRAME64, SYMBOL_INFOW, SYMOPT_DEFERRED_LOADS, SYMOPT_UNDNAME,
            },
            ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32},
        },
        ProcessStatus::{EnumProcessModulesEx, GetModuleBaseNameW, GetModuleFileNameExW, GetModuleInformation, LIST_MODULES_ALL, MODULEINFO},
        SystemInformation::IMAGE_FILE_MACHINE_AMD64,
        Threading::{
            GetThreadTimes, OpenProcess, OpenThread, ResumeThread, SuspendThread, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, THREAD_GET_CONTEXT,
            THREAD_QUERY_INFORMATION, THREAD_SUSPEND_RESUME,
        },
    },
};

/// The world, and in it the date (hours) and the flags a day runs and a
/// frame runs inside it (docs/research/main-thread.md).
const WORLD: usize = 0x233fe78;
const WORLD_DATE: usize = 0x1dd0;
const WORLD_DAY: usize = 0x1e10;

/// The parts of the day and of a frame (docs/research/architecture.md), as
/// docs/research/main-thread.md counts them: the first that is on the stack.
const DAY: [(&str, &str); 5] = [
    ("each country's daily update", "0x2ee280"),
    ("the monthly update", "0x75d690"),
    ("the AI", "0x75b4b0"),
    ("the provinces' passes", "0x75b2d0"),
    ("the yearly update", "0x75e260"),
];
const FRAME: [(&str, &str); 7] = [
    ("the autosave", "0x5cbb00"),
    ("POPS", "pops_api.dll"),
    ("drawing", "0x8183b0"),
    ("the GUI update", "0x14c5810"),
    ("commands", "0x20f810"),
    ("input", "0x830fd0"),
    ("the in-game frame, other", "0x8137c0"),
];
/// The day's own frame, below which a frame inside a day runs.
const FRAME_IN_DAY: &str = "0x769ea0";

pub fn run(args: &Args) -> Result<()> {
    let out = args.positional.first().ok_or("profile needs <out>, or with --tree a .folded file")?;
    if args.flag("tree") {
        return tree(out, args.value("root"), args.number("depth", 6)?, decimal(args, "min", 1.0)?);
    }
    sample(out, decimal(args, "seconds", 60.0)?, decimal(args, "hz", 50.0)?)
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

// ---------- which function an address lies in ----------

/// A module's functions from its exception directory: their starts, sorted,
/// their ends, and the function each belongs to, a chained entry (a
/// function's later part) counting as the function it continues.
#[derive(Default)]
struct Functions {
    starts: Vec<u32>,
    ends: Vec<u32>,
    primary: Vec<u32>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

impl Functions {
    fn read(data: &[u8]) -> Option<Self> {
        let pe = u32_at(data, 0x3c)? as usize;
        let sections = u16_at(data, pe + 6)? as usize;
        let optional = u16_at(data, pe + 20)? as usize;
        let (directory, directory_size) = (u32_at(data, pe + 24 + 112 + 3 * 8)?, u32_at(data, pe + 24 + 112 + 3 * 8 + 4)?);
        let mut table = Vec::new();
        for i in 0..sections {
            let s = pe + 24 + optional + 40 * i;
            let (size, rva, raw_size, raw) = (u32_at(data, s + 8)?, u32_at(data, s + 12)?, u32_at(data, s + 16)?, u32_at(data, s + 20)?);
            table.push((rva, size.max(raw_size), raw));
        }
        let offset = |rva: u32| table.iter().find(|(start, size, _)| *start <= rva && rva < start + size).map(|(start, _, raw)| (raw + rva - start) as usize);
        let mut entries = Vec::new();
        if directory != 0 {
            let begin = offset(directory)?;
            for i in 0..directory_size as usize / 12 {
                entries.push((u32_at(data, begin + 12 * i)?, u32_at(data, begin + 12 * i + 4)?, u32_at(data, begin + 12 * i + 8)?));
            }
        }
        // UNW_FLAG_CHAININFO: the entry continues another function.
        let primary = |start: u32, unwind: u32| -> Option<u32> {
            let mut u = offset(unwind & !1)?;
            let (mut flags, mut codes) = (*data.get(u)? >> 3, *data.get(u + 2)? as usize);
            let mut at = start;
            for _ in 0..32 {
                if flags & 4 == 0 {
                    break;
                }
                let c = u + 4 + 2 * (codes + (codes & 1));
                at = u32_at(data, c)?;
                u = offset(u32_at(data, c + 8)? & !1)?;
                (flags, codes) = (*data.get(u)? >> 3, *data.get(u + 2)? as usize);
            }
            Some(at)
        };
        let mut functions: Vec<_> = entries.iter().map(|&(start, end, unwind)| (start, end, primary(start, unwind).unwrap_or(start))).collect();
        functions.sort();
        Some(Functions {
            starts: functions.iter().map(|f| f.0).collect(),
            ends: functions.iter().map(|f| f.1).collect(),
            primary: functions.iter().map(|f| f.2).collect(),
        })
    }

    /// The RVA of the function `rva` lies in, or none (a leaf function).
    fn of(&self, rva: u32) -> Option<u32> {
        let i = self.starts.partition_point(|&s| s <= rva).checked_sub(1)?;
        (rva < self.ends[i]).then(|| self.primary[i])
    }
}

// ---------- the process and its threads ----------

struct Module {
    base: u64,
    end: u64,
    name: String,
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
    let mut handles: Vec<HMODULE> = vec![std::ptr::null_mut(); 1024];
    let mut needed = 0;
    unsafe { EnumProcessModulesEx(process, handles.as_mut_ptr(), (handles.len() * size_of::<HMODULE>()) as u32, &mut needed, LIST_MODULES_ALL) };
    handles.truncate(needed as usize / size_of::<HMODULE>());
    handles
        .into_iter()
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
            let functions = std::fs::read(String::from_utf16_lossy(&path)).ok().and_then(|d| Functions::read(&d)).unwrap_or_default();
            Module { base, end: base + u64::from(info.SizeOfImage), name: String::from_utf16_lossy(&name[..len]).to_lowercase(), functions }
        })
        .collect()
}

/// The main thread's instruction pointer and return addresses, innermost
/// first.
fn walk(process: HANDLE, thread: HANDLE) -> Vec<u64> {
    // GetThreadContext fails on a context that is not 16-byte aligned.
    #[repr(C, align(16))]
    struct Aligned(CONTEXT);
    let mut aligned = Aligned(unsafe { std::mem::zeroed() });
    let context = &mut aligned.0;
    context.ContextFlags = CONTEXT_FULL_AMD64;
    if unsafe { GetThreadContext(thread, context) } == 0 {
        return Vec::new();
    }
    let mut frame: STACKFRAME64 = unsafe { std::mem::zeroed() };
    // AddrModeFlat.
    (frame.AddrPC.Offset, frame.AddrPC.Mode) = (context.Rip, 3);
    (frame.AddrFrame.Offset, frame.AddrFrame.Mode) = (context.Rbp, 3);
    (frame.AddrStack.Offset, frame.AddrStack.Mode) = (context.Rsp, 3);
    let mut pcs = Vec::new();
    while pcs.len() < 200
        && unsafe {
            StackWalk64(
                u32::from(IMAGE_FILE_MACHINE_AMD64),
                process,
                thread,
                &mut frame,
                (context as *mut CONTEXT).cast(),
                None,
                Some(SymFunctionTableAccess64),
                Some(SymGetModuleBase64),
                None,
            )
        } != 0
    {
        if frame.AddrPC.Offset == 0 {
            break;
        }
        pcs.push(frame.AddrPC.Offset);
    }
    pcs
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

fn sample(out: &str, seconds: f64, hz: f64) -> Result<()> {
    let id = *crate::game::processes().first().ok_or("eu4.exe is not running")?;
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
    let modules = modules_of(process);
    let exe_base = modules.iter().find(|m| m.name == "eu4.exe").ok_or("eu4.exe has no module eu4.exe")?.base;

    let before: HashMap<u32, u64> = threads.iter().map(|&(t, h)| (t, thread_times(h).1)).collect();
    let start_state = world_state(process, exe_base);
    let mut samples = Tally::new();
    let interval = Duration::from_secs_f64(1.0 / hz);
    let (mut suspended, mut count) = (Duration::ZERO, 0u64);
    let began = Instant::now();
    let mut next = began;
    while began.elapsed().as_secs_f64() < seconds {
        let now = Instant::now();
        if now < next {
            std::thread::sleep((next - now).saturating_sub(Duration::from_micros(500)));
            continue;
        }
        next += interval;
        let t0 = Instant::now();
        if unsafe { SuspendThread(main) } == u32::MAX {
            break;
        }
        let pcs = walk(process, main);
        let state = world_state(process, exe_base);
        unsafe { ResumeThread(main) };
        suspended += t0.elapsed();
        count += 1;
        samples.add((tag(state), pcs), 1);
    }
    let wall = began.elapsed().as_secs_f64();
    let end_state = world_state(process, exe_base);
    let after: HashMap<u32, u64> = threads.iter().map(|&(t, h)| (t, thread_times(h).1)).collect();

    let mut names: HashMap<u64, String> = HashMap::new();
    let (mut stacks, mut flags) = (Tally::new(), Tally::new());
    for ((where_, pcs), n) in &samples.counts {
        let mut frames: Vec<String> = pcs
            .iter()
            .enumerate()
            .map(|(i, &pc)| {
                let at = if i == 0 { pc } else { pc - 1 };
                names.entry(at).or_insert_with(|| name(process, &modules, at)).clone()
            })
            .collect();
        frames.push(where_.to_string());
        stacks.add(frames, *n);
        flags.add(*where_, *n);
    }
    for (_, thread) in &threads {
        unsafe { CloseHandle(*thread) };
    }
    unsafe { CloseHandle(process) };

    let folded: String = stacks.most().iter().map(|(frames, n)| format!("{} {n}\n", frames.iter().rev().cloned().collect::<Vec<_>>().join(";"))).collect();
    std::fs::write(format!("{out}.folded"), folded).map_err(|e| format!("cannot write {out}.folded: {e}"))?;

    let (mut inclusive, mut own, mut parts) = (Tally::new(), Tally::new(), HashMap::new());
    for (frames, n) in &stacks.counts {
        own.add(frames[0].clone(), *n);
        let mut seen = std::collections::HashSet::new();
        for frame in frames {
            if seen.insert(frame) {
                inclusive.add(frame.clone(), *n);
            }
        }
        *parts.entry(part(frames)).or_insert(0) += n;
    }
    let framework: u64 = stacks.counts.iter().filter(|(f, _)| f.iter().any(|fr| fr.starts_with("eu4_harmony_core.dll"))).map(|(_, n)| n).sum();

    let total = count.max(1) as f64;
    let share = |n: u64| 100.0 * n as f64 / total;
    let mut lines = vec![format!(
        "{count} samples in {wall:.1} s ({:.0} a second); the main thread (id {main_id}) was suspended {:.1}% of the time, {:.2} ms a sample",
        count as f64 / wall,
        100.0 * suspended.as_secs_f64() / wall,
        1000.0 * suspended.as_secs_f64() / total
    )];
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
    let mut cpu: Vec<(f64, u32)> = threads.iter().map(|(t, _)| ((after[t] - before[t]) as f64 / 1e7 / wall, *t)).collect();
    cpu.sort_by(|a, b| b.partial_cmp(a).expect("CPU times are numbers"));
    for (c, t) in cpu.iter().filter(|(c, _)| *c >= 0.005) {
        lines.push(format!("  {c:5.2}  thread {t}{}", if *t == main_id { " (main)" } else { "" }));
    }
    lines.push(format!("  {:5.2}  in all", cpu.iter().map(|(c, _)| c).sum::<f64>()));
    std::fs::write(format!("{out}.txt"), lines.join("\n") + "\n").map_err(|e| format!("cannot write {out}.txt: {e}"))?;
    println!("{}", lines[..lines.len().min(40)].join("\n"));
    Ok(())
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
    fn ties_keep_the_order_first_met() {
        let mut tally = Tally::new();
        for (k, n) in [("a", 1), ("b", 2), ("c", 2), ("a", 0)] {
            tally.add(k, n);
        }
        assert_eq!(tally.most().iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["b", "c", "a"]);
    }
}
