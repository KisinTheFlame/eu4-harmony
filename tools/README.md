# Development tools

Tools for trying things in a running game, finding dependency points and looking at crashes. They are for development machines only and are not among the player's files, which `release.ps1` packs.

| Tool | Use |
| --- | --- |
| `player-files.ps1` | Lists the player's files: the built files that go into the game folder (`version.dll`; the core and `eu4-harmony.exe` in `eu4-harmony\`), and where; the two tools below read it |
| `dev-install.ps1` | Installs them into the game directory as last built, a player build or a developer build, with the repository's `dev.toml` (`-Uninstall` removes them) |
| `release.ps1` | Packs them for players: `target\eu4-harmony\` and `target\eu4-harmony.zip`, whose contents go into the game's root folder |
| `game-dir.ps1` | Prints the game folder: `EU4_GAME_DIR`, or else the Steam library that has the game; the other tools' default `-GameDir` |
| `player-state.ps1` | Backs up the player's files a run of the game may change (`-Backup <new folder>`: the saves, the framework's folder and every file at the top of the user folder), and afterwards puts them back (`-Restore <folder>`): lists what the run changed, restores and compares every file, removes the run's crash folders (`-KeepCrashes` keeps them) and the backup (`-Keep` keeps it) |
| `perf-run.ps1` | Measures one run of the game for [docs/performance.md](../docs/performance.md): launch to main menu, memory, CPU paused and at speed 5, game days per run; its clicks fit a 3840×2160 screen at the game's default interface scale, and it needs reference images taken on the machine once, so read its notes before the first run |
| `leak-run.ps1` | Plays rounds of sessions on a developer build with the heap census through remote control (a load, an in-game load, a return, a new game, a return), and writes a report of what each round kept and where the game's memory goes: the leftover sites, their trend, their function, where blocks of each are let go, and after the last round what still points at them ([docs/leak-finding.md](../docs/leak-finding.md)) |
| `crash-dump.py` | Runs debugger commands on a crash dump with Windows' own dbgeng.dll, with no debugger to install; needs 64-bit Python 3.8 or later |
| `core/examples/disasm.rs` | Reads the installed game's code: disassembles, dumps bytes, counts a function's arguments |
| `dev-call-trace` feature | Logs calls to chosen game functions: the arguments, the text arguments point to, the call site, the return value (`core/src/trace.rs`; [below](#call-tracing)) |
| `dev-heap-census` feature | Counts the game's live heap blocks by site and generation, logs at every world reset those the earlier sessions left, and answers remote control's `memory` group: `eu4-harmony game memory`, `memory.leftovers`, `memory.generation` (`core/src/census.rs`, [docs/leak-finding.md](../docs/leak-finding.md)) |
| `dev-command-log` feature | Logs every command the game executes, or those of the types named, in its written form, as the command layer hears them (`core/src/command_log.rs`, `core/src/game/commands.rs`) |
| `dev-test-switches` feature | Makes startup fail, shows two test notices, or posts commands written in the game's form through the command layer at the first frame of every game, to try the failure, the notice window and the game's commands on a supported game |

To ask the running game things through remote control, such as the session or the player's armies, use the command line tool: `eu4-harmony game session get`, and `eu4-harmony wiki` for the rest. `eu4-harmony game screenshot --scale 50 --out shot.png` saves the frame as the game draws it, framework UI included, wherever its window is; `eu4-harmony game input click 800 450 --scale 50` clicks where that screenshot shows something, and `input key`, `input type`, `input scroll` and `input drag` give the game the rest of a player's mouse and keyboard, with the game in front or behind other windows ([docs/input.md](../docs/input.md)).

`perf-run.ps1` measures vanilla too, where no framework runs, so it clicks, presses keys and reads the screen itself, in physical pixels of the screen.

## Caveats

- **Back up before a test that loads a save.** The game rewrites a save after loading it: run `player-state.ps1 -Backup <new folder>` first and `player-state.ps1 -Restore <that folder>` after the test.
- **Crash dumps** are in the documents folder at `Europa Universalis IV\crashes\eu4_<date>_<time>\minidump.dmp`; the framework's logs are in `eu4-harmony\logs\`.

## Disassembly

```
EU4_GAME_DIR=<install dir> cargo run --release -p eu4-harmony-core --features analysis --example disasm -- ARG...
```

Arguments (numbers are hex, with or without `0x`; addresses are all RVAs):

- `RVA`: the whole function starting there, its end taken from the exception directory; a leaf function, which has no entry in the directory, runs to the padding after its `ret`, at most 40 instructions;
- `RVA:N`: N instructions from RVA, which need not be the start of a function;
- `bytes:RVA:N`: N bytes in hex, in the same format as dependency points;
- `args:RVA`: how many arguments the function reads, which is how many an observe point on it must forward at least.

The same analysis also runs the offline check: `EU4_GAME_DIR=<install dir> cargo test --release -p eu4-harmony-core -- --ignored` checks the bytes of every dependency point, and checks that every observe point forwards no fewer arguments than its function actually reads.

## Developer builds

Each development tool is a feature of the core, and `dev` turns them all on:

```
cargo build --release --features eu4-harmony-core/dev
cargo build --release --features eu4-harmony-core/dev-command-log,eu4-harmony-core/dev-call-trace
```

A tool built in is on; nothing else turns it on. The heap census observes every allocation of the game's, which slows it down: build only the tools a run needs. The build goes where the player build goes, `target\x86_64-pc-windows-msvc\release`, and `dev-install.ps1` installs whichever was built last; the first line of the framework log and of the startup report says "developer build" and names the tools, and the report lists their options as they took effect. `release.ps1` builds the player build itself before packing it.

The tools' options are in `dev.toml` at the top of the repository, which is not checked in; `dev-install.ps1` copies it into the game folder's `eu4-harmony\`, where the core reads it at startup. [dev.toml.example](dev.toml.example) shows every section with its defaults; a section left out takes them, and a file the core cannot read, or an option it does not know, fails startup.

## Call tracing

In `dev.toml`'s `[call-trace]`, `functions` lists the RVAs of the functions to trace, for example `["0x15988e0", "0x159c600"]`. The first 20 calls to each function (`calls` changes this) go into the framework log: the value of each argument, with the text when an argument points to text (a C string or a `std::string`, decoded as game text), the call site, and the return value.

The number of arguments is counted from the function's code. For a leaf function with no entry in the exception directory, write `"RVA:N"` and give the count yourself; give too few and the function reads garbage, and the game may crash.
