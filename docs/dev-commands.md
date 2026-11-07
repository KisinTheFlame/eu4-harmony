# Development commands plan

Status: decided (2026-11-04); X1 to X4 built and tried the same day ([prototype-log.md](prototype-log.md), "Development commands"); X3's `perf-run` and `leak-run` were replaced on 2026-11-05 by the `perf`, `leak` and `growth` scenarios of `cargo xtask scenario` ([scenarios.md](scenarios.md)). The tools are now described in [dev-tools.md](dev-tools.md).

## Goal and scope

- **Goal**: the repository's development tools are Rust, built, formatted, tested and linted with the rest of the workspace, instead of PowerShell and Python scripts that nothing checks. What needs the repository becomes a command of `cargo xtask`, a program of the workspace that only development machines run; what is useful without it, to mod authors too, becomes a command of the command line tool, `eu4-harmony`.
- **In scope**: every script under `tools/` (`game-dir.ps1`, `player-files.ps1`, `dev-install.ps1`, `release.ps1`, `player-state.ps1`, `perf-run.ps1`, `leak-run.ps1`, `profile.py`, `crash-dump.py`); entry points for the disassembler and the offline checks that find the game themselves; the game-lock skill, which calls `player-state.ps1`, switched to the command that replaces it.
- **Out of scope**: the core's development tools (`dev-call-trace`, `dev-memory`, `dev-heap-census`, `dev-command-log`, `dev-test-switches`) and `dev.toml`, which run inside the game and stay features of the core; CI; a third program for development, which would only be needed on a machine without the repository.

## Where each goes

| Now | Becomes | Why there |
| --- | --- | --- |
| `game-dir.ps1` | the game folder lookup that every command of both programs uses | |
| `player-files.ps1` | a list in `cargo xtask` | only `install` and `release` read it |
| `dev-install.ps1` | `cargo xtask install [--uninstall]` | it installs the build last made in `target\`, with the repository's `dev.toml` |
| `release.ps1` | `cargo xtask release [--out <dir>]` | it builds the player build; `eu4-harmony` could not, since it would build itself while it runs, and Windows keeps a running program's file from being written |
| `player-state.ps1` | `eu4-harmony backup <new dir>`, `eu4-harmony restore <dir>` | a mod author's test run rewrites saves as ours do, and the backup needs nothing of the repository |
| `perf-run.ps1` | `cargo xtask perf-run`, now the `perf` scenario | a measurement for [performance.md](performance.md); it also measures vanilla, so it clicks and reads the screen itself, at fixed places of a 3840x2160 screen |
| `leak-run.ps1` | `cargo xtask leak-run`, now the `leak` and `growth` scenarios | it needs a developer build with the memory account, and writes for [leak-finding.md](leak-finding.md) |
| `profile.py` | `cargo xtask profile` | it samples the game for [game-speed.md](game-speed.md), and names the framework's frames by the core's exports |
| `crash-dump.py` | `cargo xtask crash-dump` | the core's own frames need the symbols of the build in `target\` |
| `cargo run … --example disasm` | `cargo xtask disasm ARG...` | the same example, with the game found |
| `EU4_GAME_DIR=… cargo test … -- --ignored` | `cargo xtask offline-check` | the same tests, with the game found |

## Behavior

### The game folder

Both programs find it the same way: the command's `--game-dir`; else `EU4_GAME_DIR`; else, for `eu4-harmony`, the folder it is installed in (`<game>\eu4-harmony\eu4-harmony.exe`), as now; else the Steam library that holds the game, from `SteamPath` under `HKCU\Software\Valve\Steam` and the libraries `steamapps\libraryfolders.vdf` lists. `cargo xtask game-dir` prints it. The lookup lived in the manifest crate at first; it is now the `folders` crate's, with the user folder and the names of what it holds, and an empty `EU4_GAME_DIR` counts as unset for both programs ([deepening.md](deepening.md), D14).

### `cargo xtask`

- An alias in `.cargo/config.toml`, `xtask = "run --release --quiet --package xtask --"`, runs the crate `xtask/`. It is a member of the workspace, so `cargo build`, the tests and clippy cover it, CI included; it is not among the player's files and `release` never packs it.
- `install` and `release` do what their scripts do, with the same messages: a foreign `version.dll` moved to `eu4-harmony\backup\` and put back on `--uninstall`; `dev.toml` installed, or one installed before removed; the package folder and its zip made anew. `release` builds only the packages of the player's files (`-p eu4-harmony-proxy -p eu4-harmony-core -p eu4-harmony-cli`), so it never rebuilds `xtask` while it runs, and writes the zip with Windows' own `tar`.
- `perf-run` and `leak-run` take the scripts' parameters as options (`--out`, `--name`, `--reference`, `--intro-reference`, `--menu-only`; `--rounds`, `--country`, `--days`, `--sites`, `--watch`, `--watch-after`, `--keep-game`, `--from-report`, the two saves by name) and write the same files: the references a machine already took (`menu-reference.bin`, `intro-corner.png`) go on working, and so do reports read before. The scenarios that replaced them take their parameters as `--<scenario>.<parameter>` instead ([scenarios.md](scenarios.md)), and the perf run no longer measures vanilla.
- `crash-dump <dump> <log> [--symbols <dir>] <command>...` runs the commands with Windows' `dbgeng.dll`, with this checkout's release build and Microsoft's symbol server on the symbol path, as the script does.
- `disasm` and `offline-check` run the existing example and ignored tests with `EU4_GAME_DIR` set to the game found.

### `eu4-harmony backup` and `restore`

- `backup <new dir>` and `restore <dir> [--keep] [--keep-crashes]`, with `--game-dir` and `--user-dir`, do what `player-state.ps1 -Backup` and `-Restore` do: the same files (the saves, the framework's folder and every file at the top of the user folder), the same lines (`added:`, `changed:`, `deleted:`, `NOT RESTORED:`, the crash folders), a failure when a file could not be put back, the backup removed once all match.
- The backup's folder is laid out as the script's (`files\`, `items.txt`, `crashes.txt`, `user-folder.txt`), so a backup the script made is restored by the command; `copied.txt` beside them lists every file copied.
- A restore removes nothing the backup has no whole copy of: a backup missing a file `copied.txt` lists is refused before anything changes, and each copy is made in `.eu4-harmony-restoring\` in the user folder and compared before it takes the place of what is there.
- Both refuse while `eu4.exe` runs, and when the list of processes cannot be read to tell. `eu4-harmony skills` says when a mod author needs them.

### No `tools/` folder

Once the scripts are gone, `tools/` goes too:

- `tools/README.md`, the guide to the development tools (the core's features, `dev.toml`, call tracing, the disassembler and the commands above), becomes `docs/dev-tools.md`, with the other documents;
- `tools/dev.toml.example` becomes `core/dev.toml.example`, next to the code that reads `dev.toml` (`core/src/dev.rs`), whose test reads the example.

AGENTS.md, README.md, README.zh-CN.md and the plans that name a script or `tools/` name the command or the new place instead; earlier records in [prototype-log.md](prototype-log.md) keep naming the scripts they used.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`;
- what it delivers was tried, in the game where it uses the game, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- the scripts it replaces are removed, and every document that named them names the commands; the last part removes `tools/` itself.

### X1 `cargo xtask`, installing and packing

The crate and its alias; the game folder lookup in the manifest crate, with `eu4-harmony` taking Steam's library as its last place too; `game-dir`, `install`, `release`, `disasm`, `offline-check`. Tried: installing a player build and a developer build into the game and uninstalling, with a foreign `version.dll` in the root; the package's files the same as `release.ps1` packed; the offline checks passing.

### X2 Backing up the player's files

`eu4-harmony backup` and `restore`; the game-lock skill calling `restore` and `backup`, and the script while a checkout still has it. Tried: a backup and a restore around a run of the game that loads a save, every file back; a backup the script made restored by the command.

### X3 Measuring

`perf-run`, `leak-run` and `profile`. Tried: one run of each, with numbers of the same order as in performance.md and leak-finding.md, and the references taken before still recognized. `perf-run` and `leak-run` are gone since, replaced by `cargo xtask scenario perf`, `leak` and `growth`; `profile` stays.

### X4 Crash dumps

`crash-dump`, and `tools/crash-dump.py` removed; `tools/README.md` and `tools/dev.toml.example` moved, and `tools/` gone. Tried: `!analyze -v` and `kn 30` on a dump of the game's with the core's frames named.
