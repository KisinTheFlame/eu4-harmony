r"""Samples the running game's main thread, its call stack at a steady rate,
with Windows' own dbghelp.dll and no debugger to install (tools/README.md):

    python tools/profile.py OUT [--seconds 60] [--hz 50]
    python tools/profile.py OUT.folded --tree [--root FRAME] [--depth 6] [--min 1]

It finds eu4.exe, suspends its main thread HZ times a second, walks its call
stack, reads the date and the world's flags, and resumes it. It writes:

- OUT.folded: one line per distinct stack, root first, `frame;frame;... count`,
  which flame graph tools read;
- OUT.txt: how many days passed; the share of samples inside a day, inside a
  frame that runs inside a day, and between days, and of the parts of each
  (as docs/research/main-thread.md counts them); the functions with the most
  samples inside them and with the most of their own; each thread's CPU.

Each stack's root is where in the game's time it fell: `[day]`,
`[frame inside a day]` or `[between days]`. --tree prints the stacks of a
.folded file as a tree, top down, or the tree below one frame.

Frames are named by the function they lie in, from the exception directory
of the module's file: in eu4.exe by the function's RVA, elsewhere by module
and export, or module and RVA where the function is not exported.

Suspending the thread for each walk slows the game, about 1.4 ms a sample
(the report gives it): at 50 samples a second the game runs about 7% slower,
at 200 about 30%, and a slower day brings more frames inside days. Measure
shares at 50.
"""
import argparse
import bisect
import collections
import ctypes
import os
import struct
import sys
import time
from ctypes import byref, c_uint64, c_ulong, c_void_p, sizeof, wintypes

parser = argparse.ArgumentParser(description="Sample the game's main thread.")
parser.add_argument("out", help="where the results go, without .folded and .txt; with --tree the .folded file")
parser.add_argument("--seconds", type=float, default=60)
parser.add_argument("--hz", type=float, default=50)
parser.add_argument("--tree", action="store_true", help="print the call tree of a .folded file instead")
parser.add_argument("--root", help="with --tree: the tree below this frame, wherever it is on a stack")
parser.add_argument("--depth", type=int, default=6)
parser.add_argument("--min", type=float, default=1.0, help="with --tree: leave out what has fewer percent")
args = parser.parse_args()


def tree():
    """The call tree of a .folded file, top down, in percent of its samples."""
    total, top = 0, {}
    for line in open(args.out):
        stack, n = line.rsplit(" ", 1)
        frames, n = stack.split(";"), int(n)
        total += n
        if args.root:
            if args.root not in frames:
                continue
            frames = frames[frames.index(args.root):]
        node = top
        for fr in frames:
            node = node.setdefault(fr, [0, {}])
            node[0] += n
            node = node[1]

    def show(children, depth, indent):
        for fr, (n, below) in sorted(children.items(), key=lambda kv: -kv[1][0]):
            if 100 * n / total < args.min:
                continue
            print(f"{indent}{100 * n / total:5.1f}%  {fr}")
            if depth:
                show(below, depth - 1, indent + "  ")

    show(top, args.depth, "")


if args.tree:
    tree()
    sys.exit()

system = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32")
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
psapi = ctypes.WinDLL("psapi", use_last_error=True)
dbghelp = ctypes.WinDLL(os.path.join(system, "dbghelp.dll"), use_last_error=True)

# --- Which function an address lies in. -------------------------------------


def functions(path):
    """The starts of a module's functions, sorted, with their ends, from its
    exception directory; a chained entry (a function's later part) counts as
    the function it continues."""
    data = open(path, "rb").read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    sections = struct.unpack_from("<H", data, pe + 6)[0]
    optional = struct.unpack_from("<H", data, pe + 20)[0]
    directory = struct.unpack_from("<II", data, pe + 24 + 112 + 3 * 8)
    table = []
    for i in range(sections):
        s = pe + 24 + optional + 40 * i
        size, rva, raw_size, raw = struct.unpack_from("<IIII", data, s + 8)
        table.append((rva, max(size, raw_size), raw))

    def offset(rva):
        for start, size, raw in table:
            if start <= rva < start + size:
                return raw + rva - start
        raise ValueError(hex(rva))

    entries = []
    if directory[0]:
        begin = offset(directory[0])
        for i in range(directory[1] // 12):
            entries.append(struct.unpack_from("<III", data, begin + 12 * i))
    primary = {}
    for start, end, unwind in entries:
        try:
            u = offset(unwind & ~1)
            flags, codes = data[u] >> 3, data[u + 2]
            at, seen = start, 0
            while flags & 4 and seen < 32:  # UNW_FLAG_CHAININFO
                c = u + 4 + 2 * (codes + (codes & 1))
                at, _, unwind2 = struct.unpack_from("<III", data, c)
                u = offset(unwind2 & ~1)
                flags, codes = data[u] >> 3, data[u + 2]
                seen += 1
        except (ValueError, IndexError, struct.error):
            at = start
        primary[start] = at
    entries.sort()
    return [e[0] for e in entries], [e[1] for e in entries], [primary[e[0]] for e in entries]


tables = {}


def function_of(module, rva):
    """The RVA of the function `rva` lies in, or None (a leaf function)."""
    if module[3] not in tables:
        try:
            tables[module[3]] = functions(module[3])
        except OSError:
            tables[module[3]] = ([], [], [])
    starts, ends, primary = tables[module[3]]
    i = bisect.bisect_right(starts, rva) - 1
    if i >= 0 and rva < ends[i]:
        return primary[i]
    return None


# --- The process and its threads. -------------------------------------------


class PROCESSENTRY32W(ctypes.Structure):
    _fields_ = [("dwSize", c_ulong), ("cntUsage", c_ulong), ("th32ProcessID", c_ulong), ("th32DefaultHeapID", c_void_p),
                ("th32ModuleID", c_ulong), ("cntThreads", c_ulong), ("th32ParentProcessID", c_ulong),
                ("pcPriClassBase", ctypes.c_long), ("dwFlags", c_ulong), ("szExeFile", ctypes.c_wchar * 260)]


class THREADENTRY32(ctypes.Structure):
    _fields_ = [("dwSize", c_ulong), ("cntUsage", c_ulong), ("th32ThreadID", c_ulong), ("th32OwnerProcessID", c_ulong),
                ("tpBasePri", ctypes.c_long), ("tpDeltaPri", ctypes.c_long), ("dwFlags", c_ulong)]


kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.OpenThread.restype = wintypes.HANDLE


def game_pid():
    snap = kernel32.CreateToolhelp32Snapshot(2, 0)
    entry = PROCESSENTRY32W(dwSize=sizeof(PROCESSENTRY32W))
    ok = kernel32.Process32FirstW(snap, byref(entry))
    while ok:
        if entry.szExeFile.lower() == "eu4.exe":
            kernel32.CloseHandle(snap)
            return entry.th32ProcessID
        ok = kernel32.Process32NextW(snap, byref(entry))
    sys.exit("eu4.exe is not running")


def thread_ids(pid):
    snap = kernel32.CreateToolhelp32Snapshot(4, 0)
    entry = THREADENTRY32(dwSize=sizeof(THREADENTRY32))
    ids = []
    ok = kernel32.Thread32First(snap, byref(entry))
    while ok:
        if entry.th32OwnerProcessID == pid:
            ids.append(entry.th32ThreadID)
        ok = kernel32.Thread32Next(snap, byref(entry))
    kernel32.CloseHandle(snap)
    return ids


def thread_times(handle):
    """Creation time and CPU time (user + kernel), in 100 ns."""
    c, e, k, u = (wintypes.FILETIME() for _ in range(4))
    kernel32.GetThreadTimes(handle, byref(c), byref(e), byref(k), byref(u))

    def v(f):
        return (f.dwHighDateTime << 32) | f.dwLowDateTime

    return v(c), v(k) + v(u)


pid = game_pid()
process = kernel32.OpenProcess(0x0400 | 0x0010, False, pid)  # QUERY_INFORMATION, VM_READ
threads = {}
for tid in thread_ids(pid):
    h = kernel32.OpenThread(0x0002 | 0x0008 | 0x0040, False, tid)  # SUSPEND_RESUME, GET_CONTEXT, QUERY_INFORMATION
    if h:
        threads[tid] = h
main_tid = min(threads, key=lambda t: thread_times(threads[t])[0])
main = threads[main_tid]

# Modules, for naming frames outside the exe.
modules = []
handles = (c_void_p * 1024)()
needed = c_ulong()
psapi.EnumProcessModulesEx(process, handles, sizeof(handles), byref(needed), 3)


class MODULEINFO(ctypes.Structure):
    _fields_ = [("base", c_void_p), ("size", c_ulong), ("entry", c_void_p)]


for i in range(needed.value // sizeof(c_void_p)):
    info = MODULEINFO()
    psapi.GetModuleInformation(process, c_void_p(handles[i]), byref(info), sizeof(info))
    name = ctypes.create_unicode_buffer(260)
    psapi.GetModuleBaseNameW(process, c_void_p(handles[i]), name, 260)
    path = ctypes.create_unicode_buffer(520)
    psapi.GetModuleFileNameExW(process, c_void_p(handles[i]), path, 520)
    modules.append((info.base, info.base + info.size, name.value.lower(), path.value))
exe = next(m for m in modules if m[2] == "eu4.exe")
exe_base = exe[0]

# --- Walking a stack. -------------------------------------------------------


class ADDRESS64(ctypes.Structure):
    _fields_ = [("Offset", c_uint64), ("Segment", ctypes.c_ushort), ("Mode", c_ulong)]


class STACKFRAME64(ctypes.Structure):
    _fields_ = [("AddrPC", ADDRESS64), ("AddrReturn", ADDRESS64), ("AddrFrame", ADDRESS64), ("AddrStack", ADDRESS64),
                ("AddrBStore", ADDRESS64), ("FuncTableEntry", c_void_p), ("Params", c_uint64 * 4), ("Far", wintypes.BOOL),
                ("Virtual", wintypes.BOOL), ("Reserved", c_uint64 * 3), ("KdHelp", ctypes.c_byte * 256)]


dbghelp.SymSetOptions(0x00000004 | 0x00000002)  # DEFERRED_LOADS, UNDNAME
dbghelp.SymInitializeW.argtypes = [wintypes.HANDLE, ctypes.c_wchar_p, wintypes.BOOL]
if not dbghelp.SymInitializeW(process, None, True):
    sys.exit(f"SymInitialize failed: {ctypes.get_last_error()}")
dbghelp.StackWalk64.argtypes = [c_ulong, wintypes.HANDLE, wintypes.HANDLE, c_void_p, c_void_p, c_void_p, c_void_p, c_void_p, c_void_p]
dbghelp.SymLoadModuleExW.argtypes = [wintypes.HANDLE, wintypes.HANDLE, ctypes.c_wchar_p, ctypes.c_wchar_p, c_uint64, c_ulong, c_void_p, c_ulong]
dbghelp.SymLoadModuleExW.restype = c_uint64
for base, end, _, path in modules:
    dbghelp.SymLoadModuleExW(process, None, path, None, base, end - base, None, 0)
function_table = ctypes.cast(dbghelp.SymFunctionTableAccess64, c_void_p)
module_base = ctypes.cast(dbghelp.SymGetModuleBase64, c_void_p)

context_buffer = ctypes.create_string_buffer(1232 + 16)
context_at = (ctypes.addressof(context_buffer) + 15) & ~15
CONTEXT_FULL = 0x10000B


def walk():
    """The main thread's instruction pointer and return addresses,
    innermost first."""
    ctypes.memset(context_at, 0, 1232)
    struct.pack_into("<I", (ctypes.c_char * 1232).from_address(context_at), 0x30, CONTEXT_FULL)
    if not kernel32.GetThreadContext(main, c_void_p(context_at)):
        return []
    raw = (ctypes.c_char * 1232).from_address(context_at)
    rsp, rbp = struct.unpack_from("<QQ", raw, 0x98)
    rip = struct.unpack_from("<Q", raw, 0xF8)[0]
    frame = STACKFRAME64()
    frame.AddrPC.Offset, frame.AddrPC.Mode = rip, 3
    frame.AddrFrame.Offset, frame.AddrFrame.Mode = rbp, 3
    frame.AddrStack.Offset, frame.AddrStack.Mode = rsp, 3
    pcs = []
    while len(pcs) < 200 and dbghelp.StackWalk64(0x8664, process, main, byref(frame), c_void_p(context_at), None,
                                                    function_table, module_base, None):
        if frame.AddrPC.Offset == 0:
            break
        pcs.append(frame.AddrPC.Offset)
    return pcs


def read(address, size):
    buf = ctypes.create_string_buffer(size)
    got = ctypes.c_size_t()
    if not kernel32.ReadProcessMemory(process, c_void_p(address), buf, size, byref(got)):
        return None
    return buf.raw


def world_state():
    """The date (hours), and the flags `world+0x1e10` (a day runs) and
    `world+0x1e18` (a frame runs inside it)."""
    p = read(exe_base + 0x233FE78, 8)
    if not p:
        return None
    world = struct.unpack("<Q", p)[0]
    if not world:
        return None
    d = read(world + 0x1DD0, 4)
    f = read(world + 0x1E10, 9)
    if not d or not f:
        return None
    return struct.unpack("<i", d)[0], f[0], f[8]


# --- Naming frames. ----------------------------------------------------------

names = {}


class SYMBOL_INFOW(ctypes.Structure):
    _fields_ = [("SizeOfStruct", c_ulong), ("TypeIndex", c_ulong), ("Reserved", c_uint64 * 2), ("Index", c_ulong),
                ("Size", c_ulong), ("ModBase", c_uint64), ("Flags", c_ulong), ("Value", c_uint64), ("Address", c_uint64),
                ("Register", c_ulong), ("Scope", c_ulong), ("Tag", c_ulong), ("NameLen", c_ulong), ("MaxNameLen", c_ulong),
                ("Name", ctypes.c_wchar * 256)]


def name(address, innermost):
    """`0x758430`, the RVA of a function of the exe; elsewhere the module and
    the function's export, or the module and the function's RVA. A return
    address is looked up one byte back, in the call."""
    at = address if innermost else address - 1
    if at in names:
        return names[at]
    module = next((m for m in modules if m[0] <= at < m[1]), None)
    if not module:
        label = "?"
    else:
        f = function_of(module, at - module[0])
        if module is exe:
            label = f"0x{f:x}" if f is not None else f"eu4.exe+0x{at - exe_base:x}"
        else:
            start = module[0] + f if f is not None else at
            label = f"{module[2]}+0x{start - module[0]:x}"
            symbol = SYMBOL_INFOW(SizeOfStruct=88, MaxNameLen=255)
            displacement = c_uint64()
            if dbghelp.SymFromAddrW(process, c_uint64(start), byref(displacement), byref(symbol)) and displacement.value == 0:
                label = f"{module[2]}!{symbol.Name}"
    names[at] = label
    return label


def tag(state):
    """Where in the game's time the sample fell, by the world's flags."""
    if not state:
        return "[no world]"
    return "[frame inside a day]" if state[2] else "[day]" if state[1] else "[between days]"


# --- Sampling. ----------------------------------------------------------------

before = {tid: thread_times(h)[1] for tid, h in threads.items()}
start_state = world_state()
samples = collections.Counter()
interval = 1.0 / args.hz
suspended = 0.0
count = 0
began = time.perf_counter()
next_at = began
while time.perf_counter() - began < args.seconds:
    now = time.perf_counter()
    if now < next_at:
        time.sleep(max(0.0, next_at - now - 0.0005))
        continue
    next_at += interval
    t0 = time.perf_counter()
    if kernel32.SuspendThread(main) == 0xFFFFFFFF:
        break
    try:
        pcs = walk()
        state = world_state()
    finally:
        kernel32.ResumeThread(main)
    suspended += time.perf_counter() - t0
    count += 1
    samples[(tag(state), tuple(pcs))] += 1
wall = time.perf_counter() - began
end_state = world_state()
after = {tid: thread_times(h)[1] for tid, h in threads.items()}

# --- The report. ---------------------------------------------------------------

stacks, flags = collections.Counter(), collections.Counter()
for (where, pcs), n in samples.items():
    stacks[tuple(name(pc, i == 0) for i, pc in enumerate(pcs)) + (where,)] += n
    flags[where] += n

with open(args.out + ".folded", "w") as f:
    for frames, n in stacks.most_common():
        f.write(";".join(reversed(frames)) + f" {n}\n")

inclusive, own = collections.Counter(), collections.Counter()
for frames, n in stacks.items():
    if frames:
        own[frames[0]] += n
    for fr in set(frames):
        inclusive[fr] += n

# The parts of the day and of a frame (docs/research/architecture.md), as
# docs/research/main-thread.md counts them: the first that is on the stack.
DAY = [("each country's daily update", "0x2ee280"), ("the monthly update", "0x75d690"), ("the AI", "0x75b4b0"),
       ("the provinces' passes", "0x75b2d0"), ("the yearly update", "0x75e260")]
FRAME = [("the autosave", "0x5cbb00"), ("POPS", "pops_api.dll"), ("drawing", "0x8183b0"), ("the GUI update", "0x14c5810"),
         ("commands", "0x20f810"), ("input", "0x830fd0"), ("the in-game frame, other", "0x8137c0")]


def part(frames):
    """`[where]: part`, for a stack innermost first with its tag last."""
    where, frames = frames[-1], list(reversed(frames[:-1]))
    if where == "[frame inside a day]" and "0x769ea0" in frames:
        frames = frames[len(frames) - 1 - frames[::-1].index("0x769ea0"):]
    for label, f in DAY if where == "[day]" else FRAME:
        if any(fr == f or fr.startswith(f) for fr in frames):
            return f"{where}: {label}"
    return f"{where}: other"


parts = collections.Counter()
for frames, n in stacks.items():
    parts[part(frames)] += n
framework = sum(n for frames, n in stacks.items() if any(fr.startswith("eu4_harmony_core.dll") for fr in frames))

lines = []
total = max(count, 1)
lines.append(f"{count} samples in {wall:.1f} s ({count / wall:.0f} a second); the main thread (id {main_tid}) "
             f"was suspended {100 * suspended / wall:.1f}% of the time, {1000 * suspended / total:.2f} ms a sample")
if start_state and end_state:
    days = (end_state[0] - start_state[0]) / 24
    lines.append(f"{days:.0f} days passed, {days / wall:.2f} a second")
lines.append("")
lines.append("Samples by where in the game's time (the world's flags):")
for k, n in flags.most_common():
    lines.append(f"  {k}: {100 * n / total:.1f}%")
lines.append("")
lines.append("Samples by part:")
for label, n in sorted(parts.items()):
    lines.append(f"  {100 * n / total:5.1f}%  {label}")
lines.append(f"  ({100 * framework / total:.1f}% of samples have the framework's core on the stack)")
lines.append("")
lines.append("Most samples inside (inclusive):")
for fr, n in inclusive.most_common(60):
    lines.append(f"  {100 * n / total:5.1f}%  {fr}")
lines.append("")
lines.append("Most samples of their own (self):")
for fr, n in own.most_common(60):
    lines.append(f"  {100 * n / total:5.1f}%  {fr}")
lines.append("")
lines.append("CPU of each thread over the run, in cores (busiest first):")
cpu = sorted(((after[t] - before[t]) / 1e7 / wall, t) for t in threads if t in after)
for c, t in reversed(cpu):
    if c >= 0.005:
        lines.append(f"  {c:5.2f}  thread {t}{' (main)' if t == main_tid else ''}")
lines.append(f"  {sum(c for c, _ in cpu):5.2f}  in all")
open(args.out + ".txt", "w").write("\n".join(lines) + "\n")
print("\n".join(lines[:40]))
