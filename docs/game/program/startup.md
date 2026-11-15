# Startup


**Entry.** The CRT entry `0x1a337c8` leads to `0x1a33654`, then to SDL's `WinMain` `0x1a78630`. That turns the command line into UTF-8 arguments, calls `SDL_SetMainReady` and then `SDL_main`, which is the game's `main` `0x8757e0` (`main.cpp`).

**`main`, in order:**

1. Basics:
   - looks for a second instance (`0x14c4470`), by a window class no window has, so it never finds one ([several-games.md](several-games.md));
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
