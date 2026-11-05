# Development tools

Tools for trying things in a running game, finding dependency points and looking at crashes. What needs the repository is a command of `cargo xtask` (`xtask/`, `cargo xtask --help`), for development machines only and not among the player's files, which `cargo xtask release` packs; what does not is a command of the command line tool, `eu4-harmony`; what runs inside the game is a feature of the core, built into a developer build. Why each is where it is: [dev-commands.md](dev-commands.md).

| Tool | Use |
| --- | --- |
| `cargo xtask game` | The claim (GLOSSARY): `claim`, `start`, `stop`, `release`, `renew`, `watch`, `status`, one holder of the computer's game at a time, in a queue, the player first; the claim backs up the player's files and the game folder's framework files, `start` starts the game hidden, and the release keeps the run's logs and crash folders and puts everything back ([game-testing.md](game-testing.md), the game-test skill). `claim --lease <minutes>` sets the lease (30 without it), `start --timeout <seconds>` how long a start may take to reach the main menu (300), `watch --warn <minutes>` how long before the lease's end it exits (5) |
| `cargo xtask install` | Within the claim, installs the player's files into the game folder as last built, a player build or a developer build, with the repository's `dev.toml`: `version.dll` in the root, the core and `eu4-harmony.exe` in `eu4-harmony\` (`--uninstall` removes them) |
| `cargo xtask release` | Builds the player build and packs it for players: `target\eu4-harmony\` and `target\eu4-harmony.zip`, whose contents go into the game's root folder |
| `cargo xtask game-dir` | Prints the game folder: `EU4_GAME_DIR`, or else the Steam library that has the game. Every command that needs the game finds it so; `game`, `install` and `scenario` also take `--game-dir` for another |
| `eu4-harmony backup`, `restore` | Backs up the player's files a run of the game may change (`backup <new folder>`: the saves, the framework's folder and every file at the top of the user folder), and afterwards puts them back (`restore <folder>`): lists what the run changed, restores and compares every file, removes the run's crash folders (`--keep-crashes` keeps them) and the backup (`--keep` keeps it) |
| `eu4-harmony start` | Starts the game and waits for its main menu: `--hidden` with nothing of it on the desktop and no sound unless `--sound`, `--timeout <seconds>` for the wait (300). Needs Steam running and refuses a game already running; within the repository, `cargo xtask game start` does the same within the claim ([hidden-start.md](hidden-start.md)) |
| `cargo xtask scenario` | Plays scenarios in the game through remote control, within one claim, and writes one report of how each ended and what it found ([scenarios.md](scenarios.md)): with no name it lists them; it needs Steam running, claims the game unless this checkout holds it (`--wait` waits in the queue), builds and installs each build the scenarios need, starts the game hidden, tells a scenario that passed from one that failed a check, crashed, hung or was cut short by the player taking the game, and releases the claim it made. `--<name>.<parameter> <value>` changes a parameter for a run; `--from-report <dir>` writes `report.md` again |
| `scenario perf` | On the player build, one run of the framework's game for [performance.md](performance.md): the start to the main menu, memory, a new game as Ming, CPU paused and at speed 5, game days a minute; `--perf.events <kinds>` listens to those events while it runs (their cost is in the framework's log, `event costs:`), `--perf.reads true` times `country.get`, `country.budget` and `province.get` |
| `scenario growth`, `scenario leak` | Rounds of sessions (a load, an in-game load, a return, a new game, a return) and what each round kept: `growth` on the build with the memory account, where each round's growth goes part by part and heap by heap; `leak` on the build with the heap census, the leftover sites too, their trend, their function, where blocks of each are let go, after the last round what still points at them and what keeps the heap's free pages committed, with `watch` which owners were let go without them, with `quarantine` what the repairs let go and whether anything was written to it, and what the census does not count and which sizes of it grew ([leak-finding.md](leak-finding.md)) |
| `cargo xtask profile` | Samples the running game's main thread from outside, its call stack at a steady rate with Windows' own dbghelp.dll, and writes where its time went: inside days, in frames inside days and between days, by part and by function, as flame graph stacks too; `--tree` prints a run's stacks as a call tree. Measures vanilla and any build. It uses the running game, so claim it first ([research/main-thread.md](research/main-thread.md)) |
| `cargo xtask crash-dump` | Runs debugger commands on a crash dump with Windows' own dbgeng.dll, with no debugger to install |
| `cargo xtask disasm` | Reads the installed game's code: disassembles, dumps bytes, counts a function's arguments ([below](#disassembly)) |
| `cargo xtask offline-check` | Checks every dependency point's bytes against the game's `eu4.exe`, and that every observe point forwards no fewer arguments than its function reads |
| `dev-call-trace` feature | Logs calls to chosen game functions: the arguments, the text arguments point to, the call site, the return value (`core/src/trace.rs`; [below](#call-tracing)) |
| `dev-memory` feature | Answers remote control's `memory`: what the process holds, split into the game's heap, the other heaps, the heaps' free pages, what lies outside the heaps and the framework's heap, at no cost between two asks (`core/src/memory_account.rs`, [leak-finding.md](leak-finding.md)) |
| `dev-heap-census` feature | Counts the game's live heap blocks by site and generation, logs at every world reset those the earlier sessions left, and answers remote control's `memory.leftovers`, `memory.watch`, `memory.generation`, `memory.quarantine`, `memory.pages` and `memory.uncounted`; builds the memory account in too (`core/src/census.rs`, [leak-finding.md](leak-finding.md)) |
| `dev-command-log` feature | Logs every command the game executes, or those of the types named, in its written form, as the command layer hears them (`core/src/command_log.rs`, `core/src/game/commands.rs`) |
| `dev-write-watch` feature | Logs what a game function writes to memory that was there before it was called: the heap's blocks and `eu4.exe`'s data, by the instruction and the calls it was in (`core/src/write_watch.rs`; [below](#write-watch)) |
| `dev-test-switches` feature | Makes startup fail, shows two test notices, posts commands written in the game's form through the command layer at the first frame of every game, or crashes the game there, to try the failure, the notice window, the game's commands and what follows a crash on a supported game; has every save's countries and provinces written by the game's loops too and compared with the threads' (`compare-saves`), or saves written by the game's own code alone (`game-writes-saves`), to check and measure S8 ([game-speed.md](game-speed.md)) |

To ask the running game things through remote control, such as the session or the player's armies, use the command line tool: `eu4-harmony game session get`, and `eu4-harmony wiki` for the rest. `eu4-harmony game screenshot --scale 50 --out shot.png` saves the frame as the game draws it, framework UI included, wherever its window is; `eu4-harmony game input click 800 450 --scale 50` clicks where that screenshot shows something, and `input key`, `input type`, `input scroll` and `input drag` give the game the rest of a player's mouse and keyboard, with the game in front or behind other windows ([input.md](input.md)).

## Caveats

- **Claim the game first.** The game rewrites a save after loading it, and a run installs into the game folder: `cargo xtask game claim` backs both up and `release` puts them back. A player or mod author without the repository backs up with `eu4-harmony backup <new folder>` and restores with `eu4-harmony restore <that folder>`.
- **Crash dumps** are in the documents folder at `Europa Universalis IV\crashes\eu4_<date>_<time>\minidump.dmp`; the framework's logs are in `eu4-harmony\logs\`. `cargo xtask crash-dump <dump> <log> '!analyze -v' 'kn 30'` writes what the debugger says of one into `<log>`, the core's own frames named from this checkout's release build.

## Disassembly

```
cargo xtask disasm ARG...
```

Arguments (numbers are hex, with or without `0x`; addresses are all RVAs):

- `RVA`: the whole function starting there, its end taken from the exception directory; a leaf function, which has no entry in the directory, runs to the padding after its `ret`, at most 40 instructions;
- `RVA:N`: N instructions from RVA, which need not be the start of a function;
- `bytes:RVA:N`: N bytes in hex, in the same format as dependency points;
- `args:RVA`: how many arguments the function reads, which is how many an observe point on it must forward at least;
- `uses:`, `refs:`, `slots:` and `chain:` find the instructions that use a field, a global or a vtable, as `core/examples/disasm.rs` describes.

It runs `core/examples/disasm.rs` with the core's `analysis` feature. The same analysis runs the offline check, `cargo xtask offline-check`: the core's ignored tests, with `EU4_GAME_DIR` set to the game found.

## Developer builds

Each development tool is a feature of the core, and `dev` turns them all on:

```
cargo build --release --features eu4-harmony-core/dev
cargo build --release --features eu4-harmony-core/dev-command-log,eu4-harmony-core/dev-call-trace
```

A tool built in is on; nothing else turns it on. The heap census observes every allocation of the game's, which slows it down: build only the tools a run needs. The build goes where the player build goes, `target\x86_64-pc-windows-msvc\release`, and `cargo xtask install` installs whichever was built last; the first line of the framework log and of the startup report says "developer build" and names the tools, and the report lists their options as they took effect. `cargo xtask release` builds the player build itself before packing it.

The tools' options are in `dev.toml` at the top of the repository, which is not checked in; `cargo xtask install` copies it into the game folder's `eu4-harmony\`, where the core reads it at startup. [core/dev.toml.example](../core/dev.toml.example) shows every section with its defaults; a section left out takes them, and a file the core cannot read, or an option it does not know, fails startup.

## Call tracing

In `dev.toml`'s `[call-trace]`, `functions` lists the RVAs of the functions to trace, for example `["0x15988e0", "0x159c600"]`. The first 20 calls to each function (`calls` changes this) go into the framework log: the value of each argument, with the text when an argument points to text (a C string or a `std::string`, decoded as game text), the call site, and the return value.

The number of arguments is counted from the function's code. For a leaf function with no entry in the exception directory, write `"RVA:N"` and give the count yourself; give too few and the function reads garbage, and the game may crash.

## Write watch

In `dev.toml`'s `[write-watch]`, `function` is the RVA of a game function, for example `"0x74ff30"`, the world's write of itself into a save. During its first call on the main thread (`calls` changes how many), every other thread of the process is suspended but those that write a save's countries and provinces with the main thread, whose writes are watched too, and every page of the process heap that holds a block and every writable page of `eu4.exe` is read-only. Each write to one of them stops at an exception handler, which records it and lets it through. When the call returns, the pages are as they were, the threads go on, and the framework log lists the writes, grouped by the instruction and the calls it was in, most first:

```
write watch #0: 40539 main static at 0x15bf95 to 0x2458c00 in 0x861c55 < 0xd2f434 < 0x29cde7 < …
write watch #0: 4300 main block at 0x29cd99 to 0x50 bytes +0x40 in 0x2976ac < 0xd2f434 < 0x297568 < …
```

What a write went to is `static` (`eu4.exe`'s data, by its RVA), `block` (a heap block that was there when the call began: its size and the offset written), or `new` (the rest of the heap's pages: blocks made during the call, and the heap's own records). Writes to the stack and to memory outside the heap and the image are not seen.

- Build it alone (`--features eu4-harmony-core/dev-write-watch`): the held threads may hold a lock the watched function then waits for, and the heap census takes one at every allocation.
- The watched call runs many times slower: an ironman save of 55 MB from 1724 took about a minute and wrote 25.8 million times. Setting it up walks the heap, 0.6 s for 8 million blocks.
- A function that waits for another thread never returns while watched.
