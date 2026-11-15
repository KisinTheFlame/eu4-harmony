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

See [Startup](startup.md).

## The application, its frame and the idlers

See [The application, its frame and the idlers](../day/frame-and-idlers.md).

## Threads

See [Threads](../day/threads.md).

## The game state

**The world.**

- `[0x233fe78]` is `CGameState` (`gamestate.cpp`), 0x7518 bytes, made by `0x74c080` (vtable `0x1cb00c8`, base constructor `0x74c8b0`).
- It is a persistent object like the commands are: `+0x10` writes the save's game state (`0x74ff30`), `+0x20` reads one key (`0x751cd0`).
- Most of its fields were matched to the save's keys through that writer.

| Field | What |
| --- | --- |
| `+0x1ca8`, `+0x1cd0` | The provinces by ID (0x2e10 bytes each); the province array the parallel stages walk |
| `+0x1ce8`, `+0x1d00` | All countries; the active countries, rebuilt every day (`0x75ab30`) |
| `+0x1dd0`–`+0x1df0` | The date and its parts ([dates.md](../day/dates.md)) |
| `+0x1e00` | The in-game idler ("map manager") |
| `+0x1e08`, `+0x1e0c` | The speed; the lag level, from how long the last day took |
| `+0x1e10`, `+0x1e18` | Set while the daily update runs; set while a frame runs inside it |
| `+0x1e30`, `+0x1e40` | Wars going on; wars that ended ([countries.md](../countries/countries.md)) |
| `+0x1ea0` | The game's events waiting ([questions-and-offers.md](../diplomacy/questions-and-offers.md)) |
| `+0x15d0`, `+0x1b40` | The HRE and the celestial empire, in the world itself |
| `+0x1bb8`, `+0x1c48` | Diplomacy; the combat manager (its battles at `+0x28`) |
| `+0x2198` | Trade |
| `+0x25a8`, `+0x25b0`, `+0x25b8` | Religions' data (the papacy), great projects, the unit manager |
| `+0x7484` | Whether the AI runs |

**Other process-wide objects:**

| Global | What |
| --- | --- |
| `[0x233d8d0]` | The country manager: countries by tag index ([countries.md](../countries/countries.md)) |
| `[0x233fed8]` | The map: provinces' map data, terrains, adjacencies ([provinces.md](../provinces/provinces.md)) |
| `[0x242bbf0]` | The event manager (`eventmanager.cpp`) |
| `[0x242bcf8]` | The message handler (`messagehandler.cpp`): the messages the simulation builds (`0x8d4660`, 639 call sites) |
| `0x2325220` | The random generator, one global MT19937; its seed `0x23494b4` and draw count `0x23494b0` |
| `0x23587b0` | The command registry, by type token |
| `0x1fd8660` | The token table: 6648 entries of 0x208 bytes, ids `0xb`–`0x3eef`. The name→id tree `0x2326720` and the id→name vector `[0x242b888]` are built from it by `0x170d250` |
| `[0x242bde8]` | The settings (`settings.txt`) |
| `0x2357b58` | The game console, an object of 0x48 bytes ([game-console.md](../scripts/game-console.md)) |
| `[0x242be58]` | The tick profiler ("Daily/Monthly/Yearly Avg. Time") |


## Time

See [Time](../day/time-architecture.md).

## Commands and multiplayer

See [Commands and multiplayer](../scripts/command-lifecycle.md).

## The AI

See [The AI](../ai/update-architecture.md).

## Script

See [Script](../scripts/runtime.md).

## Presentation

See [Presentation](../interface/rendering-architecture.md).

## Online and platform services

All of these are started in `main` and served at the top of every application frame.

| Service | Started | Every frame | Notes |
| --- | --- | --- | --- |
| Steam | `0x875600` | `SteamAPI_RunCallbacks` | Achievements (`pdx_achievements_steam`); cloud saves (`pdx_cloudstorage_steam`, used by `remotefile.cpp`) |
| Epic (`pdx_egs`) | `0x1699500` | `0x169a2f0` | Login and invites |
| POPS | `0x14ab0a0` (login) | `0x14ada90` | Wrapper `0x2340148`; Paradox account, telemetry, cloud files; `pdx_social` sits on it |
| Nakama | in the context owner | `0x168dff0` | Wrapper `[0x2433e88]`; matches for multiplayer through `pdx_net` |
| Crash reporter (`pdx_crashreport2`) | `0x874b40` | | Installs an unhandled exception filter (`0x1654c00`); writes `meta.yml` and `minidump.dmp` ([crash-handling.md](crash-handling.md)) |


## Open questions

- Which thread draws the loading screen while a save loads from the main menu, and how it shares the device with the main thread.
- What a frame inside a day skips beyond commands (the time step, view updates at `0x814e7e`).
- When defines are read relative to LoadingThread: their loader is reached through the vtable `0x1c48ba0`'s `+0x10` (`0x1fe740`), whose caller was not found.
- Whether TBB's `parallel_for` is ever taken by default, and which settings set `[0x2357b34]` and `[0x1fd10f2]`.
- The session's modes (`+0x50`), and whether `0x157b1a0` sends executed commands to the other players or records them for a replay.
- Several steps of the day's body (`world+0x21b0`, the map's `+0x6a8`, `0xacfae0`) and what the parallel country passes do.
- Where a scope keeps its root and from, where `[Root.GetName]` in localisation is evaluated, and where a country's modifiers are summed and cached. Where flags and variables are kept is [scripts.md](../scripts/scripts.md).
- How an army's 3D entity is made and moved on the map; which OGG decoder plays the music.
