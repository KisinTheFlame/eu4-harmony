r"""Runs debugger commands on a crash dump with Windows' own dbgeng.dll, no
debugger install needed (tools/README.md):

    python tools/crash-dump.py DUMP LOG [--symbols DIR] COMMAND...

Output goes to LOG. Symbols come from this checkout's release build (for the
core's own frames) and Microsoft's symbol server, cached in --symbols
(default C:\symbols). Useful commands: `!analyze -v`, `.ecxr`, `kn 30`,
`r`, `u @rip-0x20 L20`, `lmvm eu4_harmony_core`.

The game writes a folder per crash under Documents\Paradox Interactive\
Europa Universalis IV\crashes (eu4_<date>_<time>), with `minidump.dmp` and
`exception.txt`; the framework's logs are in eu4-harmony\logs there.
"""
import argparse
import ctypes
import os
import uuid
from ctypes import POINTER, byref, c_char_p, c_ulong, c_void_p, wintypes

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUILD = os.path.join(REPO, "target", "x86_64-pc-windows-msvc", "release")

parser = argparse.ArgumentParser(description="Run dbgeng commands on a crash dump.")
parser.add_argument("dump")
parser.add_argument("log")
parser.add_argument("--symbols", default=r"C:\symbols")
parser.add_argument("commands", nargs="+")
args = parser.parse_args()

system = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32")
os.add_dll_directory(system)
dbgeng = ctypes.WinDLL(os.path.join(system, "dbgeng.dll"))


class GUID(ctypes.Structure):
    _fields_ = [("a", c_ulong), ("b", ctypes.c_ushort), ("c", ctypes.c_ushort), ("d", ctypes.c_ubyte * 8)]


def guid(text):
    g = GUID()
    ctypes.memmove(byref(g), uuid.UUID(text).bytes_le, 16)
    return g


def method(obj, index, *argtypes):
    """The COM method at vtable `index` of `obj`."""
    vtable = ctypes.cast(obj, POINTER(POINTER(c_void_p))).contents
    return ctypes.WINFUNCTYPE(ctypes.c_long, c_void_p, *argtypes)(vtable[index])


IID_IDEBUGCLIENT = "27fe5639-8407-4f47-8364-ee118fb08ac8"
IID_IDEBUGCONTROL = "5182e668-105e-416e-ad92-24ef800424ba"
# Vtable indexes: IDebugClient QueryInterface 0, OpenDumpFile 19;
# IDebugControl OpenLogFile 8, CloseLogFile 9, Execute 66, WaitForEvent 93.
client, control = c_void_p(), c_void_p()
assert dbgeng.DebugCreate(byref(guid(IID_IDEBUGCLIENT)), byref(client)) == 0
assert method(client, 0, POINTER(GUID), POINTER(c_void_p))(client, byref(guid(IID_IDEBUGCONTROL)), byref(control)) == 0
assert method(control, 8, c_char_p, wintypes.BOOL)(control, args.log.encode(), False) == 0
execute = method(control, 66, c_ulong, c_char_p, c_ulong)
execute(control, 0, f".sympath {BUILD};srv*{args.symbols}*https://msdl.microsoft.com/download/symbols".encode(), 0)
assert method(client, 19, c_char_p)(client, args.dump.encode()) == 0
method(control, 93, c_ulong, c_ulong)(control, 0, 0xFFFFFFFF)
for command in args.commands:
    execute(control, 0, command.encode(), 0)
method(control, 9)(control)
