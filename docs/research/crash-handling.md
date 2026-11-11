# The game's crash handling

What the game does when it crashes, from the exception to its crash folder, and where the framework's crash record (`core/src/crash.rs`) joins it: a line in `core.log` with what crashed, where, on which thread and at which date, and the calls it was in, so that a player's logs say where the game crashed when the crash folder is not sent with them. Baseline 1.37.5; all addresses are RVAs. Read from `eu4.exe` on 2026-11-11 with `core/examples/disasm.rs` and a script over the PE file (imports, direct calls, RIP-relative references). It began with a player's logs that ended in the middle of a game with no line of the framework's after them, and no crash folder sent with them.

## Installing it

The crash reporter (`pdx_crashreport2`), made in `main` (`0x874b40`, [architecture.md](architecture.md), "Startup"), installs the handlers once (`0x1654c00`, guarded by the byte `[0x242bf7d]`):

- the unhandled exception filter `0x1654730`, through `SetUnhandledExceptionFilter` (`0x1654c19`);
- `0x1654b50`, for `abort`, through `0x1a4b36c` (_inferred_: the C runtime's `signal`, from what it handles);
- `0x1654ac0`, for a pure virtual function's call, through `0x1a35140` (_inferred_: `_set_purecall_handler`).

## The three ways in

Each first asks the handler object `[0x23496b8]` through `0x1650580` whether to go on, which lets only one crash of a process through (_inferred_ from its use; not read to its end). Then:

- **An unhandled exception** (`0x1654730`, given the `EXCEPTION_POINTERS`): a stack overflow (`0xc00000fd`) is handled on a thread of its own, since its own stack has no room: the filter starts it with `0x1654250` and the text "Unhandled Exception" (`0x1654799`) and waits for it. Any other exception calls `0x1654250` itself (`0x1654a43`) with "Unhandled exception". The filter then ends the process: `TerminateProcess(GetCurrentProcess(), 1)` (`0x1654a89`).
- **A pure virtual function's call** (`0x1654ac0`): `0x1654250` with "Pure Virtual Function Call" and no exception (`0x1654aeb`), then `TerminateProcess(…, 1)`.
- **`abort`** (`0x1654b50`): `0x1654250` with "Abnormal Program Termination" and no exception (`0x1654b7b`), then the same.

`0x1654c50` is the game's own test of it: it writes to address 0 (`0x1654c5f`) inside a guarded block whose filter calls `0x1654730` directly (`0x1654cc6`).

What ends the process without the filter comes to none of this: a corrupted heap's end (`0xc0000374`) and other fail-fast ends go to Windows' error reporting, and the game leaves no crash folder ([late-game.md](late-game.md) met one).

## Writing the crash's files

`0x1654250`(text, the exception's pointers or null, the thread as a handle) builds the crash folder's path (`0x164ea00`, `0x164ebd0`) and writes, as the handler object's flags say:

- `exception.txt` (`+0x2a4`), through `0x1653d10`, with the text, the exception and its context;
- `minidump.dmp` (`+0x2a2`), through `0x1655290`, with the thread's id from `GetThreadId` (`0x165447a`);
- `screenshot.bmp` (`+0x2a5`), from SDL's window;
- `meta.yml` and the rest (`+0x2a6`, `0x164f450`);
- then (`+0x2a7`, and a setting on) it minimizes and destroys the window and starts `CrashReporter.exe`, waiting for it ([headless.md](headless.md), "The crash reporter").

The thread handle is `GetCurrentThread()`'s, a pseudo handle: for a stack overflow, read on the thread `0x1654250` runs on, it names that thread, not the one that overflowed (_inferred_ from the handle's meaning).

## Where the crash record joins

The record observes `0x1654250`'s entry, before it writes anything: every way in reaches it once, with the text, and with the exception where there is one. It reads the exception record and the context through reads that cannot fault, walks the stack from the context, and writes two lines to the log, waiting at most a second for the log's lock, which the crashed thread may hold. Without an exception it walks from where it is, through the observer's own calls to the way in. Before and not after, since after comes the crash reporter, which waits for the player, and a player who ends the game there would leave no record.

What it cannot give:

- **A stack overflow's calls**: the context is the overflowed thread's, and the walk reads only the stack of the thread it runs on, so it gives the address alone; the thread it names is the one the game made.
- **Anything of a crash that never reaches the filter** (above).
