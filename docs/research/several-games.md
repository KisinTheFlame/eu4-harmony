# Several games at once

Whether more than one game can run on this computer at the same time, for development runs side by side: what in the game, in Steam, in the framework and in the development tools keeps it to one, and what each would take. Baseline 1.37.5 (`9ad3efe1…`); all addresses are RVAs. Read from `eu4.exe` with `cargo xtask disasm` and its import table on 2026-11-07, and tried in the game the same day ([prototype-log.md](../prototype-log.md), "Two games at once"). What is marked _inferred_ was concluded, not seen. It answers [headless.md](headless.md), "What it would take", item 4.

## Why

Every session on the computer that tries something in the game waits for the one claim ([game-testing.md](../game-testing.md)): while one plays a scenario of twenty minutes, the others queue; the run below waited for one itself. The computer has room for more than one game: 61 GB of memory, a graphics card of 16 GB, 16 threads, and a game uses about 4 GB, 1.4 GB and one busy main thread (below).

## In short

Two games ran at once, both hidden, both with the framework, each answering remote control on a pipe of its own, each in a game of 1700 at speed 5. The game does not stop a second process; Steam let both run. What keeps it to one is the framework and the development tools:

| What | Shared now | What a second game needs |
| --- | --- | --- |
| The game's instance check | Never finds anything (below) | Nothing |
| Steam | One game of app 236850 at a time is all it has seen | Nothing: two ran. Each started from the game's own folder, or it loses DLCs (below) |
| The game folder | One, with the framework installed in it | The same folder for every game; a core of each game's own, which the proxy loads from elsewhere |
| The user folder | The player's | One per game, with `-userdir`; the framework reading `-userdir` |
| Remote control's pipe | `\\.\pipe\eu4-harmony` | A name per game, and the command line tool told which |
| The claim | One holder | A claim per game, and the whole computer for what measures |

## The game's instance check never finds anything

`main` (`0x8757e0`) calls `0x14c4470` with `"Europa Universalis IV"` (`0x1c45be0`, at `0x875809`). That calls `FindWindowExA` (import slot `0x1b66d78`) at `0x14c4496` with the text in `r8`, the **class** name, and `NULL` in `r9`, the title. When a window is found it logs "An instance of this game is already running on this computer! Exiting.", shows a message box unless `-suppresserrormessages` (`MessageBoxA`, `0x1b66dc8`) and calls `FatalExit` (`0x1b66868`).

The game does register a window class of that name: `0x14c3dd0` fills a `WNDCLASSEXA` (size `0x50`, style `0x48`) with the name it is given and calls `RegisterClassExA` (`0x1b66d70`, at `0x14c3e59`). But its window is SDL's, of class `SDL_app`, titled `Europa Universalis IV`; no window of the class `Europa Universalis IV` is ever made (_inferred_ from the processes' windows, which are only `SDL_app` and `IME`). So the check finds nothing, and a second game starts. Tried: a second `eu4.exe` started while the first was at its main menu ran on, and its `system.log` has no line of the check. ([headless.md](headless.md) took the text for the title; [architecture.md](architecture.md) says the game refuses a second instance.)

## Steam, and why the game must start from its own folder

Both games ran with Steam signed in, which showed nothing. Started from another folder, the game loses DLCs:

- **A folder of junctions**: a folder holding a junction to each of the game folder's folders, copies of its files, a `userdir.txt` of its own and `steam_appid.txt` (`236850`), so that it could hold a framework of its own. The game started and played, but 5 of its 31 DLCs were disabled (`game.log`, "Setting up DLC …: Disabled": Muslim Advisor Portraits, National Monuments, National Monuments II, Dharma - Indian Sultanate Pack, Dharma - Hindu Unit Pack), and `error.log` had 17 lines "Could not find item in store backend" that a game from the game folder does not have. The same with the variable `SteamAppId` instead of `steam_appid.txt`, and with the player's `pdx_login_cache.txt` copied into its user folder.
- **The game folder, with `-userdir`**: a third game from the game folder itself, with a user folder of its own and no login cache, had all 31 enabled and no such line, while the first ran.

Why Steam leaves those 5 out for a process outside the game folder was not looked into; each has a `steam_id` in its `.dlc` file, as the others do. A game with DLCs missing is another game: saves differ, and a run on it does not test what the player plays. So every game starts from the game folder.

## The game folder, the user folder and the framework

- **The core.** The proxy loads `eu4-harmony\eu4_harmony_core.dll` beside `eu4.exe` (`proxy/src/lib.rs`), so every game from the game folder runs the same core, the one installed last. Builds of two checkouts side by side need the proxy to load a core from elsewhere, named on the command line, as `-harmony_hidden` is read: the proxy may read `GetCommandLineW` in `DllMain` (_inferred_: it touches no other module). The developer build's options, `eu4-harmony\dev.toml` in the game folder, go with the core.
- **The user folder.** The game takes `-userdir=<folder>` (`main`, step 3 in [architecture.md](architecture.md)); a game started so wrote its logs, settings, shader cache and flag files there. The framework does not read the option: it takes `userdir.txt` ([user-folder.md](user-folder.md)), so the third game above wrote its `core.log` into the player's user folder and moved the first game's to `core.prev.log`. Whether the game itself writes anything into the player's user folder when started with `-userdir` was not checked: the release compared the folder, but the first game wrote to it too.
- **What a user folder needs** to play as the player's: `settings.txt`, `dlc_load.json` (the playset) and the `mod` folder its descriptors are in (a junction to the player's served); the framework's `eu4-harmony\settings.txt` for the language; the saves a run loads. The login cache is not needed for the DLCs.
- **The game folder's own files.** The game writes `console_history.txt` there when the console is used, the one file two games would share (_inferred_; not tried).

## Remote control

The pipe's name is fixed (`eu4_harmony_remote::PIPE`). A second game's core waits for it every second while the first has it ([`pipe.rs`](../../core/src/pipe.rs), `listen`), so it starts and plays but cannot be reached until the first ends. A core and command line tool built with another name (`\\.\pipe\eu4-harmony-b`) served the second game beside the first: the client checks only that the process serving the pipe is `eu4.exe`.

## What it costs

Two games side by side, each loaded from `observer-1700.1.1.eu4` and run at speed 5 for 45 to 60 seconds, alone (the other paused in its game) and together; the window 3840×2160 borderless with 8× multisampling, as the player's settings have it:

| | Private memory | Dedicated graphics memory |
| --- | --- | --- |
| At the main menu | 3.5 to 3.7 GB | 1.23 GB |
| In 1700 | 4.0 to 4.3 GB | 1.32 to 1.38 GB |

| Days a second | First game | Second game |
| --- | --- | --- |
| Round 1, alone | 12.9 | 10.8 |
| Round 1, together | 10.6 | 9.5 |
| Round 2, alone | 11.3 | 9.3 |
| Round 2, together | 10.4 | 8.9 |

- The days get slower as the game goes on, which is most of what changes from one measurement to the next; between neighbors, a game ran 4 to 8% slower with the other running beside it. Two games together made 1.7 to 1.9 times the days of one.
- The second game ran 15 to 20% slower than the first throughout, alone too. It was the one from the folder of junctions with 5 DLCs missing; why it is slower was not looked into.
- A startup took 33 to 39 s with the other game at its main menu or in a game, as one alone does ([headless.md](headless.md)).
- So a game beside another changes what a measurement measures: runs that measure (the perf scenario, `cargo xtask profile`) need the computer to themselves.

## What it would take

1. **The core**: the proxy loads a core named on the command line instead of the installed one; the developer build's options beside it.
2. **The user folder**: the framework reads `-userdir` as the game does, before `userdir.txt`; the command line tool's `--user-dir` already names one.
3. **The pipe**: a name per game, taken from the command line, and the command line tool told which game to ask.
4. **The claim** ([game-testing.md](../game-testing.md)): a game of a holder's own, with a user folder made for it from the player's and a core copied from its checkout's build, started from the game folder and removed at the release; the player's files are then not touched, so nothing of them is backed up or put back. Installing into the game folder, the perf scenario and the player's own game keep the computer to one holder.

Nothing in the game needs to change: the instance check needs no dependency point.

Built on 2026-11-08 as test games ([test-games.md](../test-games.md)), with the pipe a test game's own in a developer build only.

## Open questions

- Whether Steam lets the player start the game, from Steam or the Paradox launcher, while a development game runs; Steam marks the game running (_inferred_).
- Why the folder of junctions loses 5 DLCs and runs slower.
- Whether a game started with `-userdir` writes anything into the player's user folder.
- How many games fit: memory in the late game ([late-game.md](late-game.md)), and the graphics driver's memory while the window is hidden ([headless.md](headless.md)), each counted once per game; a smaller window for development games, which would take less graphics memory, was not tried.
