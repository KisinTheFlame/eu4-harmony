# The game program's architecture

Baseline 1.37.5 (`9ad3efe1…`). All addresses are RVAs. Read from `eu4.exe` on 2026-11-02, with `core/examples/disasm.rs` and scripts over the PE file (its sections, imports and exports, `.pdata`, strings, RIP-relative references and direct calls). The game was not run for this note; the notes it links to say what was tested. What is not marked otherwise was read in the code or the data; what is marked _inferred_ was concluded from names, strings or the shape of the code.

This note is a map: where each part of the program lies, what it does and how the parts meet. The notes it links to go deeper.

## The build

| | |
| --- | --- |
| File | `eu4.exe`, 38,462,504 bytes, x64, a Windows GUI program, image base `0x140000000` |
| Build | 2024-10-03 (the PE timestamp; `main` logs `Oct  3 2024 : 09:26:40`), branch `release_1.37.5`, revision `835bfdf8…` (`eu4_rev.txt`; `clausewitz_rev.txt` names the same revision) |
| Build root | `C:\mnt\gsg\eu4\eu4\`, with the engine under `clausewitz\` and the game under `eu4\source\`; the PDB, `outputdir_windows\eu4.pdb`, is not shipped |
| Sections | `.text` 27.4 MiB (`0x1000`–`0x1b65d78`), `.rdata` 4.3 MiB, `.data` 4.7 MiB in memory (3.5 MiB in the file), `.pdata` (79,914 functions with unwind information), `.rsrc`, `.reloc` |
| RTTI | 507 type descriptors, nearly all lambdas, `std`, `boost` and a few exception classes. Class names come from elsewhere: the source paths in asserts and logs (396 files), log lines naming `CClass::Method`, and the token table |
| Exports | SDL2's and PhysFS's functions, which are linked in, and `NvOptimusEnablement` |

## Layers

The program has three layers: third-party libraries, the Clausewitz engine and the game. The game calls the engine, and both call the libraries.

**Third-party libraries.**

- Linked in:
  - SDL 2.0.4: the window, input, threads, and audio over DirectSound or WinMM;
  - PhysFS 3.0.1: the virtual file system, including zip and 7z archives;
  - Lua 5.1.1 with luabind: defines;
  - boost 1.67;
  - FreeImage, with libpng, libjpeg and zlib 1.2.8;
  - libcurl 7.63.0 with OpenSSL 1.1.1a.
- Imported:
  - `d3d9.dll` and `d3dx9_43.dll`, and `OPENGL32.dll`;
  - `tbb.dll`;
  - `steam_api64.dll`, `EOSSDK-Win64-Shipping.dll` (Epic), `nakama-cpp.dll` (multiplayer matchmaking) and `pops_api.dll` (Paradox accounts, telemetry and cloud files).

**The engine** (`clausewitz\`), by library:

| Library | What it holds |
| --- | --- |
| `clausewitzlib` | The application and idlers, the session, server and command factory (`session.cpp`, `server.cpp`, `order.cpp`), the random generator, localisation files, DLCs and mods, save helpers, input and hotkeys, the GUI types and the graphics object (`graphics\`) |
| `pdx_core` | Lexer and parser, the token table, persistent objects, the virtual file system over PhysFS, the command line, version and system information |
| `pdx_gfx` | The DX9 and OpenGL back ends, the shader builders and the texture manager |
| `pdx_audio`, `pdx_anim`, `pdx_particle` | Sound over SDL, mesh animation, particles |
| `pdx_localize`, `pdx_lua` | The localisation file parser and table; the Lua state |
| Platform | `pdx_net` (over Steam or Nakama), `pdx_nakama_wrapper`, `pdx_pops_wrapper`, `pdx_social`, `pdx_egs` (Epic), `pdx_achievements` and `pdx_cloudstorage` (Steam), `pdx_crashreport2`, `pdx_speech` (Windows SAPI), `pdx_applicationutils` (the tweaker console, online utilities) |

**The game** (`eu4\source\`) has 223 files named in strings:

- the game state (`gamestate.cpp`) and its objects: countries, provinces, units, wars, trade, religions, estates, the HRE;
- 18 `*commands.cpp` files (commands);
- the AI (`ai*.cpp`);
- the script engine (`trigger*.cpp`, `effect*.cpp`, the `triggers\` and `effects\` folders, `modifier.cpp`, `event*.cpp`, `onaction.cpp`);
- the map data and random new world (`map*.cpp`);
- multiplayer (`multiplayerhandler.cpp`, `oos_handler.cpp`, `ingamelobby.cpp`);
- the interface: 45 files in `graphics\`, with the views, the game console's commands and the map renderer.

**Where the code lies.** The functions naming each source file were found through the strings they load. The game's files lie in the order of their names, and each engine library's lie together. So an address in the game part tells roughly which source file a function comes from: `0x758430` lies among `gamestate.cpp`'s functions, `0x8137c0` among `ingameidler.cpp`'s.

| `.text` range | What |
| --- | --- |
| `0x1000`–`~0xde6000` | `eu4\source\*.cpp` from `achievements.cpp` to `warhistory.cpp`, then `triggers\` and `effects\` (`~0xdc8000`–`0xde5000`). The script engine is the bulk: `triggerimplementation.cpp` spans `~0xbbf000`–`0xd2b000`, `effectimplementation.cpp` `~0x5f0000`–`0x6d3000` |
| `~0xde6000`–`~0x1415000` | `eu4\source\graphics\`: views, the game console's commands (`ConsoleCmdImpl.cpp`, `~0xe45000`–`0xea2000`), the map renderer (`pdxmap*.cpp`, `~0x12da000`–`0x1301000`) |
| `~0x14a6000`–`~0x172c000` | The engine libraries |
| `~0x172c000`–`0x1b65000` | Third-party code, with SDL's exports from `0x1731da0`, PhysFS's at `0x19f6690`–`0x1a0bda0` and the CRT (entry `0x1a337c8`), and a few out-of-line pieces of game code |

## Startup

**Entry.** The CRT entry `0x1a337c8` leads to `0x1a33654`, then to SDL's `WinMain` `0x1a78630`. That turns the command line into UTF-8 arguments, calls `SDL_SetMainReady` and then `SDL_main`, which is the game's `main` `0x8757e0` (`main.cpp`).

**`main`, in order:**

1. Basics:
   - refuses a second instance (`0x14c4470`);
   - parses the command line (`0x1719120`);
   - registers the thread as "Main Thread" and raises its priority;
   - calls `SDL_Init(SDL_INIT_VIDEO)`.
2. Makes the process-wide objects:
   - the settings `[0x242bde8]`;
   - the clock `[0x23494a8]`;
   - the input context `[0x23494e8]`;
   - the SDL event pump `[0x2349580]`.
3. Handles its own options: `-userdir`, `-continue`, `-threads`, `-seed`, `-debug`, `-checksum` and others.
4. Steam and Epic (`0x875600`).
5. Starts two worker pools (`0x1712f50`): "Short Task" `[0x242b8e0]` and "Long Task" `[0x242b950]`, each with max(cores − 1, 1) threads.
6. Reads `settings.txt` (`0x155a6a0`) and makes the window (`0x14c3dd0`, `SDL_CreateWindow`).
7. Makes the "context owner" (`0x1564830`), which holds the platform services: audio, Steam's callbacks, POPS, social and Nakama.
8. Files:
   - the virtual file system: PhysFS with the game folder, and the user folder named by `userdir.txt` or `-userdir`;
   - the DLC and mod scan (`0x1506d30`, `dlc_load.json`);
   - the search path is rebuilt with them (`0x873be0`);
   - 33 directory listings are registered in advance (`0x872bd0`);
   - the crash reporter (`0x874b40`).
9. Localisation (`0x873fc0` → `0x14b9370`).
10. "Creating application...": `CEU4Application` (`0x1fe8f0`), then "Initializing application...": `0x200af0`, which calls the engine's Init `0x14c02b0` (graphics, sound, cursors).
11. The startup load (below).
12. "Running application...": the run `0x14c2d00`, until the player quits or the game restarts; then shutdown.

**The startup load.**

- `main` starts **LoadingThread** (`SDL_CreateThread` at `0x878043`, entry `0x874010` → `0x200810`). Until it ends, `main` keeps draining SDL's events, ticks Epic and sleeps 10 ms at a time.
- When the player has not chosen a language yet, the load runs on the main thread instead (`0x2003d0`), after the language idler.
- The load runs two steps:
  1. `0x202850`: `interface\*.gfx`, the map (`map\default.map`, `0x8798e0`), `shortcuts.txt`, `interface\*.gui`.
  2. `0x202490` → InitGame `0x2043a0`:
     - the databases (`0x208620`, about 90 loaders);
     - the DLCs' metadata;
     - the game's events, on_actions and disasters;
     - flags, trade nodes, the data checksum, bookmarks;
     - checking every effect.

     It then makes the main menu and queues the switch to it, which the main thread makes at its first frame.

**The virtual file system** (`pdx_core`, over PhysFS).

- Every mount is put in front (`0x1714580`, `PHYSFS_mount(…, 0)`). `0x873be0` mounts, in order: the game folder, `eu4.pdx`, each enabled DLC and mod (`0x1505dc0`), then the user folder, which is also where files are written.
- So the user folder comes first, then the mods and DLCs (the last mounted first), then the game folder (inferred: the first in the search path wins).
- `0x1718300` is the only place that opens a game file.
- Listings go through `0x170ffe0`, which uses the listings registered in advance. Each mod's `replace_path` entries are kept in a vector at `0x242b9c8`, which the recursive lister reads.
- The data checksum is made from `checksum_manifest.txt` (`0x14c2a50` → `0x171cdf0`) and compared with a value built into the code (`0x205974`). The answer goes to `[0x233d890]+2` (inferred: whether achievements are allowed).

**Defines.**

- `0x48c940` runs `common\defines.lua`, `common\defines\*.lua` and the difficulty files in Lua (`0x14a6bb0` makes the state).
- It then reads **2414 values** by name (`NDefines.<table>.<KEY>`), each into its own global in `.data`, between `0x1fc2328` (the end date) and `0x2357d68`. The types are fixed point ×1000 (1380 values), i32 (779), float (239), and a few ×32768, strings and dates.
- Reload paths call it too: 7 callers in all, among them the save reader.

**The databases.**

- **Shape.** Most databases are singletons made on first use: 0x88 bytes with 511 hash buckets, a name looked up by `0x8e9f0` (hash `(h + c) × 61`).
- **Loaders.** A loader lists `common\<folder>\*.txt` and reads each file with a reader over the lexer.
- **Order.** `0x208620` loads them in this order, which the game's `setup.log` confirms ("<Name> Initialized").

| Folder | Loader | Global |
| --- | --- | --- |
| modifier types (built into the code, about 632) | `0x8d84a0` | vector `0x2358080` |
| `static_modifiers`, `event_modifiers` | `0x8ed090` | `[0x242bd08]` |
| `country_tags`, then `countries` | `0x281410`, `0x2830a0` | the country manager `[0x233d8d0]` |
| `scripted_triggers`, `scripted_effects` | `0xace660`, `0xacb950` | `[0x242bde0]`, `[0x242bdd0]` |
| `cultures`, `religions` | `0x27b580`, `0xa79000` | `[0x242ba98]`, `[0x242bd98]` |
| `tradegoods`, `buildings` | `0xd330a0`, `0x1e5500` | `[0x242be70]`, `[0x242ba88]` |
| `governments`, `government_reforms` | `0x798fc0`, `0x7a7520` | `[0x242bc70]`, `[0x242bc80]` |
| `ideas` | `0x8508f0` | `[0x242bce0]` |
| map logic ("INIT_MAP_LOGIC") | `0x87b9e0` | the map `[0x233fed8]` |
| `tradenodes` | `0xb65d30` | `[0x242be48]` |
| `cb_types`, `wargoal_types`, `diplomatic_actions` | `0x1f9340`, `0xda4950`, `0x5b5bb0` | `[0x242ba90]`, `[0x242bed0]`, `[0x242bba0]` |
| `missions`, `decisions` | `0x8d5bd0`, `0x4882c0` | `[0x242bd00]`, `[0x242bb18]` |
| `history\` (provinces, countries, advisors, diplomacy, wars) | `0x7ea2c0` | `[0x233fea0]` |
| `events\`, then `on_actions`, `disasters` (in InitGame) | `0x6f3250`, `0x945f70`, `0x47d180` | the event manager `[0x242bbf0]`, `[0x242bd68]`, `[0x242bb10]` |

## The application, its frame and the idlers

**The application.**

- `CEU4Application` (`eu4application.cpp`) is 0x6a8 bytes, with constructor `0x1fe8f0` and vtable `0x1c48bf8`. It derives from the engine's `CApplication` (`application.cpp`; constructor `0x14bfd80`, vtable `0x1d8c248`).
- The global `[0x242bf50]` holds it, and every idler keeps it at `+0x330`.
- Its vtable: Init `+0x40` (`0x14c02b0`); before and after the loop `+0x48` and `+0x50`; SDL's application events `+0x58`–`+0x70`.

| Field | What |
| --- | --- |
| `+0x40` | The current idler; at first an engine idler that does nothing (vtable `0x1d924a8`) |
| `+0x48`, `+0x70`, `+0x78`, `+0x80` | The idler switch: pending, the next idler, the parked old one, keep the old one ([notice-window.md](notice-window.md)) |
| `+0x83`, `+0x84`, `+0x88` | Quit, restart, the restart's extra arguments ([main-menu.md](main-menu.md)) |
| `+0x2f8` | Quit asked by the event pump |
| `+0x308`–`+0x320` | The frame's start time, the time after the idler's frame, the frame's length in seconds, the frame count |
| `+0x348` | The context owner: audio `+0`, POPS `+0x60`, the gamepad `+0x188` (partly inferred) |
| `+0x350` | The graphics object (also `[0x23494b8]`) |
| `+0x360` | The graphical map |
| `+0x370` | The session |
| `+0x648` | The random seed: `-seed`, or the low half of `QueryPerformanceCounter` |

**The run** `0x14c2d00`:

1. `UpdateWindow`.
2. Pins the main thread to one CPU (`GetProcessAffinityMask`, `SetThreadAffinityMask`).
3. Runs the frame `0x14c25a0(app, 1)` until `+0x83` or `+0x2f8` is set.
4. Calls the after-loop method.
5. Restarts `eu4.exe` when `+0x84` is set (`0x14c3a10`; [time.md](time.md)).

**The frame `0x14c25a0(app, normal)`, in order:**

1. The platform services: `SteamAPI_RunCallbacks`; Epic's tick `0x169a2f0`; POPS `0x14ada90`; Nakama `0x168dff0`.
2. **Input.** The event pump `[0x2349580]`'s `+8`, `0x1569c70`, polls SDL's events. It hands each key, mouse and text event to the current idler's `+0x18`, and sends SDL's window and application events to the window and to the application.
3. Quits on `SDL_QUIT`.
4. `SDL_Delay(0)`, or 100 ms when the last Present failed. There is no frame cap here (inferred: vsync paces the frames).
5. Timing; volumes pushed to the audio.
6. **The idler's frame**, its `+0x20`. Everything else happens inside it: commands, time, the interface, drawing and Present.
7. The idler switch, when one is pending. The new idler's first frame is the next application frame.
8. Speech; hotkey chords that timed out (`[0x23494d8]`); the frame count.

**Idlers** are the game's top-level states. Each has its own frame, input handling and device objects.

| Idler | Vtable | Constructor (size) | Frame | Source |
| --- | --- | --- | --- | --- |
| Null (engine) | `0x1d924a8` | inline in `0x14bfd80` (8) | does nothing | |
| EU4 base | `0x1caaab8` | `0x6e8a90` | `0x6ea380` | `eu3idler.cpp` (inferred) |
| Language selection | `0x1d631e0` | `0x1179130` (0xcc8) | `0x1179500` | `languageidler.cpp` |
| Main menu | `0x1d5a8c8` | `0x10d23c0` (0xb48) | `0x10d2e80` | `frontend.cpp` ([main-menu.md](main-menu.md)) |
| In a game ("map manager") | `0x1cb8460` | `0x803270` (0x5bc8) | `0x8137c0` | `ingameidler.cpp` |
| Nudge editor | `0x1d6d138` | `0x1250d80` (0x2360) | `0x1253d80` | `interface\nudge.gui` |

- **Slots.** `+0x18` input event; `+0x20` frame; `+0x28` input devices; `+0x30` and `+0x38` restore and invalidate device objects; `+0x40` after activation; `+0x48` and `+0x50` activate and deactivate (which set the current idler `[0x2349570]`).
- **Transitions:**
  - InitGame switches to the main menu;
  - the main menu's launch (`0x10da520`) switches to a new in-game idler;
  - the nudge editor parks the in-game idler and returns to it.
  - **Leaving a game is not a switch.** The in-game frame asks for a restart with `-skiplauncher` (`0x14c2500`), and a new process starts with the main menu ([safe-points.md](safe-points.md)).

**Frames inside a day.**

- The daily update calls `0x769ea0` between its phases, from `0x758430` and the phase functions `0x75b0d0`, `0x75b2d0` and `0x75b4b0`.
- When the day has run past its budget, `0x769ea0` runs a whole application frame, `0x14c25a0(app, 0)`. The budget is the float at `0x245b070` times a factor by speed, at `0x1c33e18`.
- That frame includes input, the in-game frame, Present and possibly an idler switch.
- While it runs, `world+0x1e18` and the random-forbidden flag are set. The session does not execute commands inside it ("Update within execution. Skipping.").
- It is on by default (`[0x1fc23d8]` = 1, toggled by a console command `0xe84320`). This is why the map manager's frame can run inside `daily_tick`, as [time.md](time.md) records.

## Threads

No thread name reaches Windows (no `SetThreadDescription`, no `0x406D1388` exception). The names below are the game's own, kept in a registry at `0x23268b0` that the crash reporter reads.

| Thread | Made at | What it does |
| --- | --- | --- |
| Main | the process | Everything in the frame: input, commands, the game state, the interface, drawing; above normal priority, pinned to one CPU |
| LoadingThread | `0x878043`, entry `0x874010` | The startup load; it registers itself as "Main Thread" too |
| "Short Task" ×max(cores − 1, 1) | `0x1712f50`, entry `0x1712d40` | The engine's parallel work: the daily update's parallel stages (`0x1798a0`), database loading |
| "Long Task" ×max(cores − 1, 1) | the same | Longer jobs at below normal priority: parts of InitGame, out-of-sync saves |
| AudioUpdateThread | `0x16a6a8a` | Refills the sounds' buffers |
| SDL's audio device thread | inside SDL | Mixes (the callback is `0x16a6c20`) |
| SaveSessionUpdateThread, RNWSessionUpdateThread | `0x776b90` and nearby; `0xd84490` | Keep the session's network polled every 50 ms while a save is written or read, or a random new world is made (inferred: so multiplayer peers do not time out) |
| TBB's workers | `tbb.dll` | `tbb::parallel_for` in about ten places (history loading `0x7ea2c0`, map and graphics code); the engine's work sets are used instead unless two switches allow it (`[0x2357b34]`, `[0x1fd10f2]`; inferred) |
| libcurl's resolver | `_beginthreadex` | One short thread per name lookup |

Which thread draws the loading screen while a save loads from the main menu ([safe-points.md](safe-points.md)) was not found.

## The game state

**The world.**

- `[0x233fe78]` is `CGameState` (`gamestate.cpp`), 0x7518 bytes, made by `0x74c080` (vtable `0x1cb00c8`, base constructor `0x74c8b0`).
- It is a persistent object like the commands are: `+0x10` writes the save's game state (`0x74ff30`), `+0x20` reads one key (`0x751cd0`).
- Most of its fields were matched to the save's keys through that writer.

| Field | What |
| --- | --- |
| `+0x1ca8`, `+0x1cd0` | The provinces by ID (0x2e10 bytes each); the province array the parallel stages walk |
| `+0x1ce8`, `+0x1d00` | All countries; the active countries, rebuilt every day (`0x75ab30`) |
| `+0x1dd0`–`+0x1df0` | The date and its parts ([dates.md](dates.md)) |
| `+0x1e00` | The in-game idler ("map manager") |
| `+0x1e08`, `+0x1e0c` | The speed; the lag level, from how long the last day took |
| `+0x1e10`, `+0x1e18` | Set while the daily update runs; set while a frame runs inside it |
| `+0x1e30`, `+0x1e40` | Wars going on; wars that ended ([countries.md](countries.md)) |
| `+0x1ea0` | The game's events waiting ([questions-and-offers.md](questions-and-offers.md)) |
| `+0x15d0`, `+0x1b40` | The HRE and the celestial empire, in the world itself |
| `+0x1bb8`, `+0x1c48` | Diplomacy; the combat manager (its battles at `+0x28`) |
| `+0x2198` | Trade |
| `+0x25a8`, `+0x25b0`, `+0x25b8` | Religions' data (the papacy), great projects, the unit manager |
| `+0x7484` | Whether the AI runs |

**Other process-wide objects:**

| Global | What |
| --- | --- |
| `[0x233d8d0]` | The country manager: countries by tag index ([countries.md](countries.md)) |
| `[0x233fed8]` | The map: provinces' map data, terrains, adjacencies ([provinces.md](provinces.md)) |
| `[0x242bbf0]` | The event manager (`eventmanager.cpp`) |
| `[0x242bcf8]` | The message handler (`messagehandler.cpp`): the messages the simulation builds (`0x8d4660`, 639 call sites) |
| `0x2325220` | The random generator, one global MT19937; its seed `0x23494b4` and draw count `0x23494b0` |
| `0x23587b0` | The command registry, by type token |
| `0x1fd8660` | The token table: 6648 entries of 0x208 bytes, ids `0xb`–`0x3eef`. The name→id tree `0x2326720` and the id→name vector `[0x242b888]` are built from it by `0x170d250` |
| `[0x242bde8]` | The settings (`settings.txt`) |
| `0x2357b58` | The game console, an object of 0x48 bytes ([game-console.md](game-console.md)) |
| `[0x242be58]` | The tick profiler ("Daily/Monthly/Yearly Avg. Time") |

## Time

- A day is a command. The in-game frame `0x8137c0` does two things in this order:
  1. updates the session, which executes the commands queued since the last frame (`0x813fb5`);
  2. calls the time step `0x757ad0` (`0x815312`), which posts a `daily_tick` once the frame times add up to a day ([time.md](time.md)).
- In multiplayer only the host posts days. It also waits until every player has reported the day before.
- `daily_tick`'s execute `0xb79520` calls the daily update `0x758430`, which:
  1. moves the date on;
  2. posts `client_ping` (the date and lag level a player reports);
  3. runs the day's body `0x75bbe0`;
  4. runs the monthly update `0x75d690` when the month changed, then the interface's monthly refresh (the in-game idler's `+0x2b0`);
  5. runs the yearly update `0x75e260` when the year changed;
  6. deletes the units waiting to be deleted.

**The day's body, in order:**

1. Objects waiting to be destroyed are destroyed.
2. Wars that ended (`0x759640`); pirates (`0x760250`); the active countries are rebuilt.
3. The papacy, great projects, the celestial empire.
4. The game's events due today (inferred: the event manager finds today in two lists sorted by date).
5. Two parallel passes over the countries.
6. Rebels; units (a parallel gather, then deletions).
7. **Each country's daily update `0x2ee280`, one after another**, which runs each unit's daily update (`0xd3e110`).
8. The HRE.
9. Provinces: two parallel passes, then a serial one (`0x9f52e0`: constructions, spies).
10. Battles.
11. The AI (`0x75b4b0`).
12. Military power; each war's daily update (`0xd91a10`).
13. The interface's daily refresh (the in-game idler's `+0x2a8`, `0x8339f0`).
14. Army missions; unit templates.

**Parallel work** is about ten stages inside the day and the month, over countries, provinces, units and AI countries.

- Each stage goes through `0x1798a0`, which divides the range by the number of cores and hands the parts to the worker pool.
- A stage sets the random-forbidden flag `0x1fd10c4`: drawing a random number in it logs "Calling random inside forbidden area!" and gives 0. The randomness a stage needs comes as 1000 numbers drawn beforehand on the main thread (`0x14c74c0`).
- Results are merged under a lock and applied one after another.
- Everything else is serial on the main thread: commands, the date, wars, battles, every country's and unit's daily update, the game's events and the interface.

## Commands and multiplayer

**Commands.**

- **The player's interface and the AI change the game through commands**, with the few exceptions below.
- The base vtable is `0x1c3a710`, 21 slots:
  - `+0x08`–`+0x20` write and read, as a persistent object;
  - `+0x40` the check;
  - `+0x48` execute;
  - `+0x50` the type token;
  - `+0x60` clone;
  - `+0x68` and `+0x70` before and after executing;
  - three flags: a lobby or system command; allowed during a hot join; may be posted outside the frame.
- Fields: `+0x38` the sender, `+0x44` the type, `+0x48` a stamp, `+0x4c` the ID, the command's own data from `+0x50`.
- There are **339 command classes**. `0x249120` registers 337 prototypes and `0x7422d0` one more, through `0x14c6930` into `0x23587b0`. The factory `0x14c6500(type)` clones a prototype.
- Examples:
  - `daily_tick` (`0x1cea280`);
  - `buildunit` (`0x1c4dff8`, [provinces.md](provinces.md));
  - `diplomaticactioncommand` (`0x1c814b0`; its execute `0x4e7c50` is the framework's "diplomatic action carried out");
  - `move_order` (`0x1d1d3f8`);
  - `select_event_option`.

**The life of a command:**

1. The interface, the AI (`0xbfe50`, 184 call sites) or the simulation itself makes the command.
2. It is posted with `0x14ca2d0` or `0x14ca210`, stamped, and handed to the session's handler.
3. Even in single player, the local handler (`0x1580370`) writes the command out and makes the command that runs from that copy, as a network command would be. So the check runs on "a command object of its own" ([recruiting.md](recruiting.md)).
4. In the next in-game frame, `CSession::Update` `0x14ca360` executes the queued commands (`0x14ca870`, the main thread only): it asks the check again and drops a command that fails it ("Discarding invalid command!"), then calls before, execute and after.

Commands' written form, the factory and the one place every executed command passes: [commands.md](commands.md).

**What is not a command.** The game console's handlers change the state directly; the console is refused in multiplayer and ironman ([game-console.md](game-console.md)), and the checksum records whether it was used. In single player the pause key toggles the pause directly ([time.md](time.md)).

**Session kinds** (`server.cpp`), by the session's configuration (`0x14c83e0`):

| Kind | Handler | Vtable | Notes |
| --- | --- | --- | --- |
| 0 client | `0x157b5b0` | `0x1d96268` | |
| 1 host | `0x15764d0` | `0x1d96088` | Sends commands in batches per tick |
| 2 local (single player) | `0x157ffe0` | `0x1d96178` | No network |

The transport is `pdx_net`, over Steam or Nakama.

**Lockstep and out-of-sync checks.**

- Every machine executes the same commands in the same order.
- The host posts the days and waits for the slowest player; `client_ping` can pause the game ("LAG_PAUSE").
- After a `daily_tick` executes, the host fills a checksum into it (`+0x70` → `0x743aa0`, `checksums.h`). It is a record of values, not a hash of memory: the random seed and draw count, whether the console was used, and many values per province, country and unit.
- Each client compares its own state with it (`0x742420`, `oos_handler.cpp`).
- `setrandom` reseeds the generator and advances it by a count, so the seed and the count reproduce its state; both are saved.
- A hot join sends a save and holds the commands back until the new player has it.

## The AI

- The AI runs inside the day (`0x75b4b0`), only on the host or in single player and when `world+0x7484` allows it.
- Each AI country's update (`0xb0fa0`) runs in parallel.
- A country's AI is `CAIKing` (`ai_king.cpp`, at `country+0x3d0`), with agents in `ai_army`, `ai_admiral`, `ai_economy`, `ai_foreignminister`, `ai_strategy` and others.
- It acts only by posting ordinary commands, as the player's interface does. So only the commands, not the AI itself, have to be deterministic across machines (inferred).

## Script

**Reading** (`pdx_core`):

- `0x170cd10` reads a file whole.
- A reader (`0x1709570`, vtable `0x1dbf7d0`) takes statements, key, operator and value, from a tokenizer: text (`0x1dbf958`, next token `0x170e700`) or binary (`0x1dbf988`).
- Keys are token ids: every key, trigger, effect and modifier name the game knows is in the token table. A name it does not know becomes an id made at run time.
- `0x1709280(writer, id)` writes a key in either format.

**Persistent objects** (`persistent.cpp`) share one vtable layout. Commands, the world, effects, triggers, scopes and the application all use it.

| Slot | What |
| --- | --- |
| `+0x08` | Write as a block (`0xd2f410`) |
| `+0x10` | Write the fields |
| `+0x18` | Read (`0x17068b0`), calling `+0x20` for each key |
| `+0x20` | Read one key |
| `+0x30` | Fix up after reading |
| `+0x38` | Report an error with file and line |

**Triggers.**

- `0xb8ba90(out, token)` makes one by its token, a switch of 899 cases.
- When the switch has nothing, `0xb89f60` tries idea groups, religions, trade goods, institutions, areas and scripted triggers.
- A trigger's evaluate is `+0x30` and its tooltip `+0x48` (vtable of `is_rival`: `0x1cf7d10`, [countries.md](countries.md)).

**Effects.**

- `0x5dec50(container, reader, token)`, also the effect block's `+0x20`, makes one of about 617 kinds (counted by the objects it allocates).
- Execute is `+0x78`, the tooltip's key `+0x68`.
- Every effect is listed at `[0x233fe48]` and checked at the end of InitGame.

**The rest of the script engine.**

- **Scopes.** A scope is a persistent object of 0xd0 bytes (`0x1c38158`): the province `+0x28`, the country `+0x2c`.
- **Modifier types** are built into the code (`0x8d84a0`, records of 0x60 bytes at `0x2358080`); `static_modifiers` and the like give them values.
- **Scripted triggers and effects.** Scripted triggers, effects and functions are databases of their own. Parameterized script is compiled per use (`metascript.cpp`, `0xdb0990`; inferred from its log lines).
- **Scripted GUI.** It binds `common\custom_gui` to `.gui` elements by name (`scriptable_gui.cpp`).

**Localisation.**

- `pdx_localize` parses the `.yml` files and keeps the hash table.
- `localize.cpp` loads `localisation\languages.yml`, then `localisation\**\*.yml`, then `localisation\replace`. The table is `[0x2357b20]`.
- The lookup `0x14b34a0` fills in `$NAME$`. Where `[Root.GetName]` is evaluated was not found.
- How the framework changes this: [unicode-text.md](unicode-text.md).

**Saves.** One reader and one writer serve text and binary, and a zip holds the parts ([saves.md](saves.md)). The world's writer `0x74ff30` writes the game state.

## Presentation

**The graphics back end** (`pdx_gfx`).

- **The function table.** The back end is not a class: it is a table of 81 function pointers at `0x1fd10f8`–`0x1fd1378`. Its contents in `.data` are the DX9 functions. `0x16c1480` rewrites every slot when the command line names a renderer, and only `-opengl` gives OpenGL.
- **The device.** It is made with `Direct3DCreate9Ex` and `CreateDeviceEx` (`0x16d8ed0`), or plain Direct3D 9.
- **The interface over it** is in the style of Direct3D 10 (inferred from what its slots set): blend, depth and rasterizer state objects, input layouts, and shader constants kept in memory.
- **Shaders.** They are the `gfx\FX\*.shader` files, built through HLSL (`0x16fa460`, compiled with `D3DXCompileShader` and cached in `shadercache\dx9\`) or GLSL.
- **Present** is the slot at `0x1fd1300`, `0x16d4db0`.

| Slot | DX9 function | What |
| --- | --- | --- |
| `0x1fd12d8` | `0x16d8ed0` | Make the device |
| `0x1fd12e0` | `0x16d8e20` | Is the device lost |
| `0x1fd1308`, `0x1fd12f8`, `0x1fd12f0` | `0x16d4bb0`, `0x16d4a40`, `0x16d4b20` | Before, during and after a device reset |
| `0x1fd1290`, `0x1fd1298` | `0x16d4e00`, `0x16d4e50` | BeginScene, EndScene |
| `0x1fd1168`, `0x1fd1208` | `0x16d4f00`, `0x16d4e90` | Draw indexed, draw |
| `0x1fd11f0` | `0x16d8560` | Bind a texture ([font-textures.md](font-textures.md)) |
| `0x1fd1320`, `0x1fd1330` | `0x16d7de0`, `0x16d80f0` | Make and destroy a texture |
| `0x1fd1360` | `0x16d8ca0` | Set the render target |
| `0x1fd1300` | `0x16d4db0` | Present |

**The graphics object** `[0x23494b8]` (`graphics.cpp`, made by `0x14d9a20`).

- It holds:
  - the renderer `+0x88`;
  - the sprite and font types `+0xe8`;
  - the 2D scene;
  - the texture manager `+0x480`.
- Every frame begins with its device check `0x14df050`. A lost device skips the frame. A device that needs a reset is reset between invalidating (`0x14ddc70`) and restoring (`0x14ddd50`) every sprite type and listener.

**The in-game frame.** `0x8137c0` runs:

1. input (`0x830fd0`);
2. the commands and time (above);
3. the engine's services and the GUI's update (`0x14c5810`);
4. drawing (`0x8183b0`), in this order:
   1. the map mode's colors, when they changed;
   2. the device check;
   3. the map's off-screen passes;
   4. BeginScene and Clear;
   5. **the 3D map** (`0x10a59a0`): the map's layers, the map names and the 3D scene, once for each copy of the camera where the map wraps;
   6. the sky and post-processing (inferred: bloom);
   7. **the GUI** (`0x14dd010`, `0x14dd540`);
   8. debug and status text;
   9. EndScene;
   10. Present.

The main menu's frame draws the same way, without units and overlays. The mouse cursor is a Windows cursor (`gfx\cursors\`, `SetCursor`; inferred: it is not drawn into the frame). The framework UI is drawn in the Present hook, after the game's EndScene ([safe-points.md](safe-points.md)).

**The map.** The graphical map (`eu3graphicalmap.cpp`) is the application's `+0x360` (the in-game frame reads `[[idler+0x330]+0x360]`), so the main menu shows the same map behind its pages. Its renderer, the "pdxmap", is at its `+0x58`.

- **The pdxmap's layers**, each found by the effects it loads:

  | Offset | Layer |
  | --- | --- |
  | `+0x18` | Terrain and water |
  | `+0xe8` | Trees |
  | `+0x1160`, `+0x11e8` | Rivers (two of the same class) |
  | `+0x1270` | Borders |
  | `+0x1398` | Straits |
  | `+0x13e8` | Lakes |
  | `+0x1440` | Sky |

- **Map modes.** There are 85 (0–0x54).
  - A change goes through the idler's `+0xc8` (`0x821ff0`) to `0x10a8c30`.
  - That function fills a two-layer province color texture: the colors (`0x12f0b40`, through `0x12fd9b0`) and the stripes over them (`0x12fdcb0`) ([map-modes.md](map-modes.md)).
- **Picking** (`0x833360`): the screen point becomes a map position (`0x10a3e50`), and the province ID grid gives the province. The camera is in [camera.md](camera.md).

**The GUI** (`clausewitzlib\graphics`).

- **Types and windows.** A `.gui` file is read into types: `windowType`, `guiButtonType`, `instantTextBoxType` and so on, made by the factory `0x14eac80` by token. A `.gfx` file is read into sprite and font types. A window is made from a type by name ([ui-injection.md](ui-injection.md)).
- **The GUI manager** `[0x23494f0]` (vtable `0x1d58108`) holds the types, makes windows and sends input to them (`0x14e9a70`, by the event's kind). A handled event is marked so that later handlers skip it.
- **Draw and click order.** Visible windows are drawn from one ordered table, which hit testing walks too (inferred: so what is drawn on top is clicked first).
- **Views.** A view is a C++ class in `eu4\source\graphics`:
  - it makes its root window;
  - it finds its children by name;
  - it binds callbacks to them;
  - it refreshes them every frame ([notice-window.md](notice-window.md), [interface-bindings.md](interface-bindings.md)).

  The fourteen country-view tabs are made by `0x81ecf0`.

**Input.**

- SDL's events are queued on the current idler (in a game at its `+0x1208`).
- `0x830fd0` offers each event, in order, to: a modal window, the GUI manager, the graphical map, the idler itself, the camera, then the hotkey listeners.
- The hotkeys are `[0x23494d8]` (`hotkeymanager.cpp`), with the player's bindings in `shortcuts.txt`.
- Keyboard focus and text: [text-input.md](text-input.md).

**Sound.**

- SDL's audio: its own thread mixes, and AudioUpdateThread refills the buffers.
- Sounds and music are declared in `.asset` files; sounds are `.wav` and music is `.ogg`.
- `eu4sound.cpp` and `musicmanager.cpp` are the game's side.

## Online and platform services

All of these are started in `main` and served at the top of every application frame.

| Service | Started | Every frame | Notes |
| --- | --- | --- | --- |
| Steam | `0x875600` | `SteamAPI_RunCallbacks` | Achievements (`pdx_achievements_steam`); cloud saves (`pdx_cloudstorage_steam`, used by `remotefile.cpp`) |
| Epic (`pdx_egs`) | `0x1699500` | `0x169a2f0` | Login and invites |
| POPS | `0x14ab0a0` (login) | `0x14ada90` | Wrapper `0x2340148`; Paradox account, telemetry, cloud files; `pdx_social` sits on it |
| Nakama | in the context owner | `0x168dff0` | Wrapper `[0x2433e88]`; matches for multiplayer through `pdx_net` |
| Crash reporter (`pdx_crashreport2`) | `0x874b40` | | Installs an unhandled exception filter (`0x1654c00`); writes `meta.yml` and `minidump.dmp` |

## Where the framework attaches

The framework's dependency points sit in each layer:

| Layer | Where | The framework's use |
| --- | --- | --- |
| Engine: files | PhysFS's `PHYSFS_init`, `PHYSFS_openRead` and the rest | `physfs.rs`: splices elements into `.gui` and `.gfx` files as they are read ([ADR inject-ui-at-runtime](../adr/inject-ui-at-runtime.md)) |
| Engine: script | The text tokenizer's next token `0x170e700` | `unicode/scripts.rs` |
| Engine: localisation | `pdx_localize`'s import `0x16fd650` | `unicode/localisation.rs` |
| Engine: input | SDL's input method messages `0x1764940` | `unicode/input.rs` |
| Engine: graphics | Present `0x16d4db0`, before reset `0x16d4bb0`; texture creation and destruction; font loading | `framework_ui/hook.rs`; `unicode/fill.rs`, `unicode/fonts.rs` |
| Application | The restart `0x14c3a10` | `game/time.rs` |
| Idlers | The in-game frame `0x8137c0`; the main menu's frame `0x10d2e80` | `game/events.rs` (safe points); `ui.rs` |
| Time | The daily update `0x758430` | `game/events.rs`, `game/time.rs` |
| Commands | `diplomaticactioncommand`'s execute `0x4e7c50`; army move and stop | `game/diplomacy.rs`, `game/events.rs` |
| Saves | The save loader | `saves.rs` |
| Map | The map mode's colors `0x10a8c30`, `0x12fd9b0` | `map.rs` |
| GUI | The army panel, buttons, the edit box | `ui.rs`, `unicode/` |

The framework calls extensions and remote control from the in-game frame, after the session has executed the frame's commands ([safe-points.md](safe-points.md)). Where a capability changes the game, it does what the player's interface does: it posts the command the interface would post (recruiting), or calls the setter or toggle the interface calls (the speed, the pause) ([remote-control.md](../remote-control.md)).

## Open questions

- Which thread draws the loading screen while a save loads from the main menu, and how it shares the device with the main thread.
- What a frame inside a day skips beyond commands (the time step, view updates at `0x814e7e`).
- When defines are read relative to LoadingThread: their loader is reached through the vtable `0x1c48ba0`'s `+0x10` (`0x1fe740`), whose caller was not found.
- Whether TBB's `parallel_for` is ever taken by default, and which settings set `[0x2357b34]` and `[0x1fd10f2]`.
- The session's modes (`+0x50`), and whether `0x157b1a0` sends executed commands to the other players or records them for a replay.
- Several steps of the day's body (`world+0x21b0`, the map's `+0x6a8`, `0xacfae0`) and what the parallel country passes do.
- Where a scope keeps its root and from, where `[Root.GetName]` in localisation is evaluated, where variables are kept, and where a country's modifiers are summed and cached.
- How an army's 3D entity is made and moved on the map; which OGG decoder plays the music.
