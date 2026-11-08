# Drawing beside the day (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Asked by the owner on 2026-11-07: whether the game's logic and its drawing can run apart, on different threads, so that the game runs faster. Worked out from what was measured before ([main-thread.md](main-thread.md), [late-game.md](late-game.md)) and from the game's direct calls (an E8 and E9 scan of `.text`, each site mapped to its function through `.pdata`; calls through pointers are not in it), on 2026-11-08 from one run of the write watch over a frame's drawing ("The drawing on a thread of its own while the next day runs"), and on 2026-11-09 from the game's direct call graph followed from the day and from the drawing, for the plan of a drawing thread ([drawing-thread.md](../plans/drawing-thread.md); "Where the day reaches the interface and the graphics", "What the drawing reaches of the game state"). The frame, the day and the threads are in [architecture.md](architecture.md). This note was written against [ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/2026-11-04-making-the-game-faster-leaves-what-it-computes-as-it-is.md); since 2026-11-08 [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md) supersedes it, so the day's own work ("kind C" below) may now be changed too when each difference has its reason, and the AI's armies' searches already run on idle threads ([game-speed.md](../plans/game-speed.md), S9). Drawing apart from the day is about the frames, not what the day computes, so what holds it back is unchanged: what the drawing reads while the day writes.

In short: **a simulation thread and a drawing thread side by side cannot be made from the game's code**: its drawing reads the live game state while the day changes it, with nothing between them; the map's drawing reads every province each frame. Three narrower ways are open. **A frame drawn while the main thread waits for the AI stage**, which late in a game waits 11 to 22 ms a day doing nothing, could take most of the drawing off the day's path: up to 12 to 18% more days a second late in a game, and smooth frames for nothing. It is new ground, since the game never draws while a parallel stage runs, and it needs a write watch to show the frame writes nothing the AI reads. **The Direct3D calls handed to a thread of their own** is of kind A and safe by construction, but would give 5% at most late in a game, measured: 3.4 ms of a 69 ms day, of it 2.0 ms a wait in the driver that may be the graphics card's. **The drawing on a thread of its own while the next day runs**, the GUI's update left on the main thread, would keep the frames and take the drawing's 7.5 ms a day off the day's path, about 12%, if the drawing read only what the GUI's update and the map's own arrays prepared; it does not quite: the map's drawing reads three fields of every province each frame, which the next day did not write on the two days watched, and the day calls Direct3D itself, so the device would have to pass between the two threads. This last way was set aside on 2026-11-08 with what it would still need ("Where this research stops"); part of it cannot be found with the write watch. Taken up again on 2026-11-09: the day changes the interface's and the graphics' own state in at least 48 places reached by direct calls (popups and messages, the map mode, the minimap, the units' models, a combat result drawn into a texture), and the drawing reads little of the game state once the map mode's colors and the debug text are left out of it; so the drawing can run beside the stretches of the day that reach none of those places, if the main thread waits for it before each that does; and the game itself lets its whole frame in between two countries' daily updates, every 32 countries, so the interface can be updated often inside a day without a copy of the world ([drawing-thread.md](../plans/drawing-thread.md)).

## Where the main thread's time goes, late in a game

From [late-game.md](late-game.md) ("The whole day, late in a game"), 1780 at speed 5, "Faster time" on, an average day of 64.4 ms of the main thread:

| | ms a day | Can it run beside the day? |
| --- | --- | --- |
| The AI stage | 24.2, of it 11 to 22 the main thread spinning while the workers finish (`0x179e90`) | It already runs on the workers; the spin is idle time |
| Each country's daily update | 11.9 | No: the day itself, serial; changing it is a question of its own, under the new ADR |
| The monthly update, its share | 8.8 (266 ms on the month's first day) | No: the day itself, as above |
| Drawing, 1.53 frames a day | 7.5 | This note |
| The rest between days: commands, the GUI's update, the map's markers, the alerts | 5.8 | The commands no; the rest this note |
| The provinces, the rest of the day's body | 6.3 | No: the day itself |

So the most that drawing apart could give, late in a game, is the frames' 13.3 ms of the day's 64.4: about 25% more days a second. Early in a game the frames are a smaller share at "Faster time" (19 ms of a 62 ms day around 1450, [late-game.md](late-game.md)), and the AI's spin is shorter, 5.5 ms a day.

At the game's own pace ("Smooth map"), the frames inside a day are up to a third of the main thread ([main-thread.md](main-thread.md), 1624); "Faster time" takes them away today, at the cost of a choppier map. Drawing beside the day would give both: the smooth map and the faster time.

## A simulation thread and a drawing thread

What it would be: the day on one thread, the frames on another, each at its own pace, the drawing showing the last day finished.

Why the game's code does not allow it:

- **The frame reads the live game state.** The map's colors, the units on it, the outliner, the alerts, the top bar and every open window read the world's objects through the same globals the day writes (`[0x233fe78]`, `[0x233d8d0]` and the rest). There is no copy for the drawing and no lock between the two: the game keeps them apart by running them on one thread, and the only frames inside a day it draws are at points between the day's phases (`0x769ea0`), on that thread.
- **The day changes containers, not only values.** Units, armies and wars are made and deleted, lists grow and are moved; a frame reading them meanwhile reads freed memory. The game puts off some deletions to a point in the day ("Objects waiting to be destroyed", "the units waiting to be deleted"), which is for the frames between phases, not for a thread beside it.
- **A copy for the drawing** would have to hold every object the frame reads, taken once a day, and every place in the frame that reads the world would have to read the copy instead. The world is many hundreds of megabytes of objects linked by pointers; copying it each day would cost more than drawing, and redirecting the frame is a rewrite of the game's renderer and interface.
- **Locking** the state while either runs would put them back in turn, which is what the game does now.

So this is not a change the framework can make to the game as it is.

**A renderer and interface of the framework's own**, drawing from a copy it takes between days, was asked about too (the owner, 2026-11-07). Weighed:

- **What the screen shows is mostly worked out, not stored.** The 85 map modes' colors (`0x10a8c30`, [map-modes.md](map-modes.md)), a tooltip's breakdown of a country's income or modifiers, the ledger: the game's code works each out from the live state as it draws. A renderer of the framework's would have to work them out again from its copy, the same as the game does, which is writing much of the game's rules a second time, with no way to check them but against the game's screens.
- **Its size.** The game alone (no DLC or mod) has 114 `.gui` files, about 90,000 lines with 857 windows, 35 shader files and 127,000 English localisation keys; behind each window a view of the game's own code binds its widgets to the state ([architecture.md](architecture.md), "Presentation"). Besides them the units' models and animations and the map's layers.
- **Mods.** Mods change the interface and the map through the game's own `.gui`, `.gfx` and shader files, as the player's playset does (the Chinese language mod, Graphical Map Improvements). A renderer of the framework's would have to read every such file as the game does, a second Clausewitz interface engine, or those mods would no longer work, against designing for any mod.
- **What it could give** is still the most any drawing apart could give: the frames' 13.3 ms of a 64 ms day late in a game, about 25%; the day itself, 51 ms on one thread, stays.

So a renderer of the framework's would cost about as much as the game's presentation, and give no more than the frame drawn in the AI stage's wait (below) could give most of.

## A frame drawn while the AI stage is waited for

The AI stage (`0x77d8a0`) runs one task per AI country on the workers and the main thread; once no task is left to take, the main thread spins in `0x179e90` until the last one is done. Late in a game that was 22 ms a game day by the spin's own samples from 1775 to 1820 ([late-game.md](late-game.md), "The AI stage"), and 11.3 ms of a 15.3 ms stage when the wait was shared out among the threads running at it, from the 1780.7.1 save at 16.3 days a second ([late-game.md](late-game.md), "What the main thread waits on in the AI stage"). The frame's code could run in that time instead, as the game runs it between phases: `0x14c25a0(app, 0)` with `world+0x1e18` and the random-forbidden flag set, which executes no commands ("Update within execution. Skipping.").

**What it could give:**

- At "Faster time": the frame between days could leave out its drawing when a frame was drawn in the stage since the last one, so that the drawing leaves the day's path. Late in a game the spin (11 to 22 ms) is longer than a frame (4.9 ms of drawing and part of the 5.8 ms besides), so up to 7.5 to 11 ms of a 61 to 64 ms day, the spin bounding it where it is shorter than the frames' 13.3 ms: 12 to 18% more days a second. Which of the frame between days' work can be left out (the GUI's update, the alerts) and which cannot (input, the commands, the time step that posts the day) is to be read.
- At the game's pace: the frames inside a day drawn in the spin instead of between phases, the map as smooth as now and the day not waiting for it.
- Early in a game the spin is 5.5 ms a day, shorter than a frame: little gain there.

**Why the game's code does not rule it out:**

- The write watch saw an army's think write no army, province or country, only its agent, blocks made in the call, the world's random generator at `world+0x1550`, the pool's and the allocator's data ([late-game.md](late-game.md), "What an army's think writes"). What the AI tasks read the frame does not write by design: a frame inside a day executes no commands and posts the player's input as commands for later.
- The random-forbidden flag, a global, is set by the stage and by a frame inside a day alike; the AI's agents draw from the world's generator, not from the global one the flag guards.
- A game started with `-seed` runs the AI's countries one after another on the main thread instead of on the stage ([ai.md](ai.md), "A day of the AI"), so it has no spin to draw in: this way is for a player's game, and a bench on `-seed` ([game-speed.md](../plans/game-speed.md), S5) cannot measure it.
- The device is made `D3DCREATE_MULTITHREADED` and the frame runs on the main thread as always, so nothing changes for Direct3D.

**What is not known, each to be shown before it is built:**

- **The game never draws while a stage runs.** `0x769ea0` is called from the day's phases (`0x758430`, `0x75bbe0`, `0x75b0d0`, `0x75b2d0`, `0x75b4b0`, `0x75d690`), between stages, never from inside one. A frame in the spin is a frame at a point the game was not written for.
- **The frame runs stages of its own on the same pool.** Three were found by direct calls: the GUI's update `0x14c5810` → `0x169bd80` (also in the main menu's and the nudge editor's frames), the alerts `0xdf5860` → `0xe075b0` → `0xe0fe50`, and the map's markers under `0x10a6d00` → `0x10a7a90` → `0x10a74c0`; each goes through `0x1798a0` and `0x179e90`. Whether the pool takes a second job while one is in flight, and what the main thread does when it helps with both, was not read. The main thread takes its job's parts itself, so the frame's stage should finish without the workers, but that is not shown.
- **Whether the frame writes what an AI task reads.** The GUI's views and the alerts may work values out and keep them in the countries' or provinces' objects (a modifier set made again, a cache of diplomacy, `0x93fe30`), which an AI task on another thread reads at the same time. If so, the AI's decisions could change, or its read could tear. Shown by the developer build's write watch over a frame inside a day, its writes against what the AI stage reads; the frame's work depends on the windows open, so on several.
- **The shared lock** `[0x2357ba0]` the AI's agents take, and the allocator's locks: a frame that takes them waits for the AI, which costs time but does not deadlock unless the frame holds something an AI task waits for.
- **The bar.** A frame inside a day is allowed by the ADR because it executes no commands and draws no random numbers. A frame beside the AI's tasks must also write nothing they read while they run; that is a proof of a new kind, by the write watch and the code, and whether it meets the bar is the owner's to decide.

## The Direct3D calls on a thread of their own

The game draws through a table of 81 function pointers (`0x1fd10f8`–`0x1fd1378`, [architecture.md](architecture.md), "Presentation"). Each slot could be replaced by one that records the call into a queue, which a thread of the framework's plays to the device, so that the driver's work on the CPU leaves the main thread. The frame's code, which reads the game state, stays on the main thread: this is of kind A, outside the game state.

- **What it could give**: the driver's share of the frame. Paused, `DrawIndexedPrimitive` (`0x16d4f00`) was 4.8% of the main thread, nearly all inside `d3d9.dll`; at speed 5 late in a game, measured below.
- **What it needs**: every slot that returns something (a texture or buffer made, a lock of a dynamic buffer, a lost device, a query) answered on the main thread or made to wait for the queue; the locked buffers' bytes copied into the queue; the device's reset; the framework's own drawing (the framework UI in the Present observer, the font pages it writes, screenshots) through the same queue. The device is `D3DCREATE_MULTITHREADED` and the loading screen is already drawn from another thread ([prototype-log/2026-10-27-unicode-module-framework-ui.md](../prototype-log/2026-10-27-unicode-module-framework-ui.md), "Two graphics driver failures"), so the device can be used from the framework's thread.
- **Measured** on 2026-11-08 ([a late frame's drawing and the driver](../prototype-log/2026-11-08-a-late-frames-drawing-and-the-driver.md)): the 1780.7.1 save in observe mode in the Ottomans' view at speed 5 with "Faster time", a hidden test game, 180 s, 14.5 days a second, 69 ms a day. The drawing (`0x8183b0`) was 7.9% of the main thread, 5.5 ms a game day:

  | What ran innermost in the drawing | Share | ms a game day |
  | --- | --- | --- |
  | The main thread waiting in the kernel, under the driver | 37% | 2.0 |
  | The game's code | 34% | 1.9 |
  | `d3d9.dll` | 15% | 0.8 |
  | The driver (`nvd3dumx.dll`) | 9% | 0.5 |
  | Other (`ntdll.dll`, the C runtime) | 4% | 0.2 |

  By the drawing's parts: the map (`0x10a59a0`) 2.5 ms, of it 20% waiting; the GUI (`0x14dd010`) 1.4 ms, 29% waiting; the frame's end (`0x14ddc40`, through the framework's Present observer to `0x16d4db0`) 1.1 ms, 97% waiting; `0x10a51e0` 0.4 ms. The driver already works on a thread of its own: one thread of it ran on 6.6% of the ticks, against the main thread's 0.5 ms a day in it.
- **So it could give at most about 3.4 ms of a 69 ms day, 5%**: the 1.3 ms the Direct3D calls run on the main thread, and the 2.0 ms it waits in them, if that wait is for the driver's queue and not for the graphics card, which a queue of the framework's would then wait for as well; which of the two it is was not found. A hidden test game draws to no window on the desktop, so the wait at the frame's end may differ in a game the player sees, with the player's vertical sync.

## The drawing on a thread of its own while the next day runs

What it would be: the frame as now on the main thread up to its drawing (input, the commands, the time step, the GUI's update), then the drawing (`0x8183b0`) handed to a thread of the framework's while the main thread goes on to the next frame and its day; before the day reaches what changes the interface (the interface's daily refresh, the in-game idler's `+0x2a8`, and its monthly one), and before the next frame's GUI update, the main thread waits for the drawing. Engines built for it work so. It needs the drawing to read only what the GUI's update and the map's own data prepared, and the day to write none of that while the drawing runs.

**What the map's drawing reads**, watched on 2026-11-08 ([what a frame's drawing reads](../prototype-log/2026-11-08-what-a-frames-drawing-reads.md), [what the drawing reads and the next day writes](../prototype-log/2026-11-08-what-the-drawing-reads-and-the-next-day-writes.md)): one call of the map's drawing (`0x10a59a0`) in the 1780.7.1 save at speed 5, in observe mode in the Ottomans' view, with the write watch's `reads-before`, every page of the process heap and `eu4.exe`'s data inaccessible during the call.

- 3.07 million reads and writes in 29,626 groups, of 8,481 blocks of the heap and 263 places of `eu4.exe`'s data. Most of them were `d3d9.dll`'s and the driver's (`nvd3dumx.dll`) own state, reached from the game's draw calls.
- **The map's own arrays.** The map renderer (`0x12dd5f6` and the code from `0x12dd9c2` to `0x12de4f6`, among `pdxmap*.cpp`'s functions) reads per-province arrays the map manager keeps (`[map manager+0xc0]`, a byte a province, and `[+0xd8]`, an i32 a province, compared between neighbors), not the provinces themselves: presentation data, which the map mode's refresh and the GUI's update fill.
- **Every province, each frame.** The same code walks the world's province array (`[world+0x1ca8]`, one block of 0x3793907 bytes, 4,942 provinces of 0x2e10), reads each province's ID (`+0x20`) and a byte at `+0x18`, and copies that byte into bit 0 of the map manager's byte for the province (`0x12dda90`..`0x12ddac2`). What the byte at `+0x18` is was not found; it is in the province object, which the day writes.
- **The world itself**: its province array (`+0x1ca8`) and its map manager (`+0x1e00`), 1,870 times each, in the same code.
- Other blocks the game's code read, unnamed: 0x7d0 bytes (`0xfd7c00`, 41,545 reads, among `eu4\source\graphics\`), 0x25a8 bytes (`0xfd7be2`), 0x16e8 and 0x1627 bytes (`0x12e6d70`), 0x5850f and 0xb09f7 bytes (in the map renderer).

- **Also from the provinces, for the borders**: `0x12ddb75`..`0x12ddba6` reads two neighbors' 16 bits at `+0x46c` (the high half of the eight bytes at `+0x468`, through `[world+0x1ca8]`) and compares them.

**The GUI's drawing** (`0x14dd010`) stops while watched: the main thread waits in the graphics driver (`ntdll` < `KERNELBASE` < `nvd3dumx.dll+0x18767d` < … < `d3d9.dll+0x11086` < the game's back end `0x16d66be` < `0x159b113` < `0x15f41b9` < `0x14dd4ee`), for the driver's own thread, which the watch holds. The first run waited 16 minutes, the game ended by the release; a second run let the driver's threads go on (`let-run`), and the map's drawing then never ended, its watch making no progress the watchdog of that time could tell from a loop's; a third gave the watch up after 20 s of nothing new and went on, twice the same way. What the GUI's drawing read up to the wait: 14,021 blocks and 4,315 places of `eu4.exe`'s data with the map's (5,540 blocks and 4,052 places more than the map's alone), mostly blocks of 0x218, 0x288 and 0x2e0 bytes, unnamed.

**What the next day wrote of it**, the third run, two rounds, each the map's and the GUI's drawing then the next daily tick (`0xb79520`): the day also stops while watched, in `d3dx9_43.dll+0x11aa4b` → `d3d9.dll` → the driver, the same wait: **the day calls Direct3D**, through D3DX. Up to it, the first day wrote 2.63 million times, 123,412 of them to what the drawing had read or written, in 15,772 groups; the second 2.57 million and 87,060. Of the first day's 123,412, the game's own code wrote 121,906:

| What | Writes | Groups | The first writer |
| --- | --- | --- | --- |
| The province array (0x3793907 bytes) | 78,840 | 6,799 | `0x780836` |
| A block of 0x34612e0 bytes, unnamed | 37,122 | 8,667 | `0x2d0ae3`, `0x9569f`, `0x94398` |
| The world, at `+0xb90` only (its random generator's state) | 5,893 | 136 | `0x14c723c` |
| A block of 0x1b70 bytes, unnamed | 26 | 23 | `0xacfef2` |
| The map manager (0x5bc8 bytes), at `+0x338` | 25 | 5 | `0xa0b1f3` |

- **Field by field, the provinces do not meet**: the day wrote 994 different fields of the provinces (by each group's first write; `+0x120` the most), and none of the three the map's drawing reads, `+0x18`, `+0x20` and `+0x46c`. A day on which a province changes owner, area or the like may write `+0x46c`; two days of an observed peace are not all days.
- **The world** meets only as a block: the day wrote its random generator's state, the drawing read its province array and map manager.
- **The block of 0x34612e0 bytes is most likely the countries**, inferred from the offsets and not checked in the game: every offset written, less 0x20, falls into the country object's size, 0x9b78 bytes (its deleting destructor `0x286510`, [main-menu.md](main-menu.md)), at fields its writers' code uses: `0x2d0ae3` appends to a vector at `+0x2338`..`+0x2348` of the country at index 8 (`+0x4ff00` = 0x20 + 8 × 0x9b78 + 0x2340), after comparing provinces' `+0x46c` (`0x2d0a3f`); `0x9406e6` in `0x93fe30`, the cache of diplomacy, counts into a country's array of 16-bit values at `+0x10c6`. So the countries lie one after another in one block, as the provinces do, and the day's writes there are its countries' daily updates (`0x75b210` < `0x75c0ca`) and the AI stage's (`0x77f8fd`, `0x77f44d`). What the drawing read of them is not known: the watch keeps what a reading call read by its block, not its field, and lists only the 300 largest of its groups.
- **What the day draws through D3DX** was not found. `d3dx9_43.dll+0x11aa4b` is no export of the DLL. The game imports eight functions of D3DX (`D3DXLoadSurfaceFromMemory`, `D3DXCreateTexture`, `D3DXLoadSurfaceFromSurface`, `D3DXCompileShader`, `D3DXSaveTextureToFileInMemory`, `D3DXSaveSurfaceToFileInMemory`, `D3DXCreateLine`, `D3DXCreateCubeTexture`), all called from the graphics back end (`0x16d50e0`, `0x16d5430`, `0x16d7aa0`, `0x16d7e70`, `0x16d8120`, `0x16d9750`, `0x16da460`), which the game reaches through its table of function pointers, so no direct call leads to them from the day. The call trace's `times` on those functions and on the daily tick would show which of them run inside a day, and from where.

So the frame's drawing and the next day touch the same objects but, in the map's part and on these two days, not the same fields of the provinces; and both call Direct3D, the day through D3DX, so a drawing thread would have to own the device while the main thread's day waits for it at each of its Direct3D calls, or have them handed to it. The GUI's drawing was watched only to its first wait in the driver, and the day only to its first Direct3D call.

**Where this research stops** (2026-11-08): it was set aside here, since the drawing on a thread of its own is not first among the ways to a faster game ([game-speed.md](../plans/game-speed.md), "Which part comes next"). What it would still need, and whether it can be found:

- **What of the countries each side touches**: can be found, by a watch that keeps what was read by its eight bytes instead of its block, and the countries' block checked against the country list in the same run.
- **What the day draws through D3DX**: can be found, with the call trace (above).
- **The day past its first Direct3D call**: can be found in parts, by watching the day's phases one at a time; only the phase that calls Direct3D cannot be watched past the call.
- **The GUI's drawing past its first wait in the driver**: cannot be found with the write watch. It holds every other thread, so the driver's own thread does not run and the drawing's call waits for ever; with the driver's threads running, the watch did not end (the second run); and the GUI's drawing calls Direct3D throughout, so it cannot be cut into parts that do not.

## The in-game frame's steps, in order (read 2026-11-09)

The direct calls of the in-game frame (`0x8137c0`), by where they lie in it, for which of its steps read or change the interface:

1. Before the commands: the dialog machinery `0x10a03f0` (at `0x813ac5`, given the world's map manager), GUI calls `0x15f42c0` and `0x152b3f0` (twice), the map's `0x6ecae0`, the timers `0x1711d40`, `0x1711eb0`, `0x1711f80`, the date's text (`0x6e4c20`, `EU3_DATE_STRING`), the chat's check (`0x812e20`). Which of these run in every frame and which only when something waits was not read.
2. The commands (`0x20f810`, at `0x813fb5`), with the random-forbidden flag cleared around them; the daily tick among them runs the day.
3. The outliner (`0x128cd30`), input (`0x830fd0`), the time step (`0x757ad0`), the alerts (`0xdf5860`), the map's markers (`0x10a6d00`), the GUI's update (`0x14c5810`).
4. **The drawing (`0x8183b0`), last**, right after the GUI's update (`0x81591c`).

So the frame between days has the interface's steps on both sides of its commands: a drawing handed away at the frame's end would meet the next frame's first steps before that frame's day begins, unless those steps wait for it or touch nothing it draws.

The drawing's own steps, by its calls: the map mode's colors (`0x10a8c30`, at `0x818652`, when they changed), the device check (`0x14df050`), the map's off-screen passes (`0x10a51e0`), BeginScene (`[0x1fd1290]`), the 3D map (`0x10a59a0`), the sky and the rest (`0x10a5f30`, `0xde7480` four times), the GUI (`0x14dd010`, `0x14dd540`), `0x6ebd10` (the GUI manager's, `vic_18`), and from `0x818aff` the debug text, its strings `AI Budget`, `Current Money`, `GUI Objects`, `Rendered Objects`, `2D Render Time` and the rest, drawn when `[0x2340040]` is set (also written by the console's `0xe85d70`).

## Where the game lets a frame in, inside the day (read 2026-11-09)

The direct calls of `0x769ea0`, the check for a frame inside a day, by the function they lie in: 6 in the daily update (`0x758430`), 11 in the day's body (`0x75bbe0`), 14 in the monthly update (`0x75d690`), one in the provinces' phase (`0x75b2d0`) and one in the AI's (`0x75b4b0`), before the AI stage (`0x77d8a0`) and not inside it. In the countries' phase (`0x75b0d0`) it is **inside the loop over the countries**: after each country's daily update (`0x2ee280`, at `0x75b20b`), when the country's index is a multiple of 32 (`and eax, 8000001Fh`, at `0x75b212`), it checks for a frame (`0x75b22f`). So the game itself holds that its whole frame, the interface's update included, may run between two countries' daily updates. The loop walks the list at `world+0x1d08`, its count at `+0x1d14` (_inferred_: the active countries, `+0x1d00` in [architecture.md](architecture.md), "The game state"); how many points that gives a late day was not counted.

## The camera in a frame (read 2026-11-09)

The camera is updated in the in-game frame's input (`0x830fd0`): it is offered the frame's events (`0x109c9e0`, at `0x83117b`), then updated (`0x109bee0`, at `0x8311b3`): it glides towards its goal (`0x109f240` → `0x109f3e0`, `0x109cd80`), and is kept on the map (`0x109ec30` → `0x109d0e0`) ([camera.md](camera.md)). The main menu's frame (`0x10dc4f0`) and the nudge editor's (`0x1253d80`) call the same two. Later in the in-game frame, between the time step and the map's markers, `0x109c020` is called (at `0x815711`); what it does was not read. So the camera moves once a frame, from the events of that frame, on the main thread; nothing found moves it from the drawing.

## Where the day reaches the interface and the graphics (read 2026-11-09)

How it was read: every function of `.pdata`, its direct calls (`E8`) and its jumps into other functions, followed breadth-first from the daily update `0x758430`; each function reached is kept with the call it was reached by, so each finding below has a path from the day. Calls through pointers are not followed, so what it finds is what the day reaches at least, and a path says the code can go there, not that a day does. The day reaches 6,005 functions so. Of them:

- **48 lie among `eu4\source\graphics\`** (`0xde6000`–`0x1415000`) and are entered from code outside it: the places where the day's code goes into the interface's.
- **17 use the graphics back end's table** (`0x1fd10f8`–`0x1fd1378`): textures made (`0x1fd1320`) and destroyed (`0x1fd1330`), a render target set (`0x1fd1360`), and others.
- **16 use the GUI manager** (`[0x23494f0]`), **9 the graphics object** (`[0x23494b8]`).
- None calls `d3d9.dll` or `d3dx9_43.dll` itself: the day's Direct3D goes through the back end's table, whose functions call D3DX (as the 2026-11-08 watch found the day waiting in `d3dx9_43.dll`).

What they are, by the strings they load and the source files their asserts name:

| What the day does | Its path from the day | What it changes of the interface or the graphics |
| --- | --- | --- |
| A message, and its popup | a country's month (`0x2f36c0`) → the message handler (`0x8c07b0`, `messagehandler.cpp`) → `0x8c1430` → `0x8c1f80` | Popups by name (`DefaultPopup`, `EndOfLandCombatPopup`, `EndOfNavalCombatPopup`, `NewColonialCountryPopup`, `NewClientVassalPopup`, `NameHeirDialog`, …) through the dialog machinery (`0x10a03f0`, `0x10a06c0`: `AgreeButton`, `DeclineButton`, `popup`); the combat result (`0x8c7790`) → `0xde6300` (`3dcombatresult.cpp`), which sets a render target and makes and destroys textures; the message list's items (`0x123f800`, `MessageInterfaceListItem`) |
| A dialog to the player from a country's day | each country's daily update (`0x2ee280`) → `0xe428b0` (`Title`, `Description`) → `0x10a03f0` → … → the window class's constructor (`0x1543f80`) | A window made, and the textures it makes; the loans' dialog (`0x33d6c0` → `0x11bc630`, `EXTENDLOAN`, `REPAYLOAN`); a new client state or colonial nation (`0x481f90`, `dynamiccountries.cpp` → `0xe2baa0`, `CS_NEW_STATE_DESC`, `CustomizeColony`) |
| A dialog from the battles' step | `0x75bbe0` → `0x2485b0` (`combatmanager.cpp`) → … → `0xd429e0` | `MOVE_AUTO_TRANSPORT_DIALOG`, and exploring (`0x10c22c0`, `CONFIRM_EXPLORE_COAST`) |
| The map mode set again | a province's construction (`0xa1a010`) → the capital moved (`0x3041b0`, `CAPITALMOVED`) → the in-game idler's map mode (`0x821ff0`, its slot `+0xc8`) | The map mode's colors (`0x10a8c30`: a texture made), the map's names (`0x10a9be0`: their meshes), and others (`0x10af270`, `0x12dd0c0`) |
| A province's owner changed | the provinces' month (`0x9f35c0`) → `0xa00270` (`SetOwner`) → `0x139bff0` → `0x1347a50` | The map's markers (`strait_indicator`, `river_indicator`, `province_terrain`), through the GUI manager |
| A center of trade updated | `0xa00270` → … → `0x474d80` (`UpdateCenterOfTrade`) → `0x13238b0` | A port's model (`westerngfx`, `_trade_node_port_0`) |
| The minimap | a country's year (`0x2f7e90`) → a diplomatic action executed (`0x4e7c50` → `0x4ea820`) → `0x10b6c50` (`eu3minimap.cpp`, `map/minimap_base.bmp`) | Sprite types (`0x156e0a0`, `spritetype.cpp`) and their textures made and destroyed |
| A unit made | a native uprising (`0x75f630`) and others → the unit's constructor (`0xd3acd0`, `unit.cpp`) → `0x1448880` | `0x1094e30`, `0x1095870`, `0x1326c60` (_inferred_: the unit's model on the map) |
| A government reform | `0x2ee280` → `0x7b0670` (`government_reform_stack.cpp`) → `0x7b1e10` → `0x1323560` | `idle` (_inferred_: an animation's state) |
| A new age | the month (`0x75d690`) → `0x771000` (`new_age`) | Its window (`0xded0f0`, `new_age_objectives_entry`) |
| The frames inside a day | `0x758430` → `0x769ea0` → `0x14c25a0` | A whole frame, its drawing included |

Through pointers, and so not in the graph, the day also calls the in-game idler's daily refresh (slot `+0x2a8`, `0x8339f0`, which calls the minimap `0x10b6c50` and the date's text among 17 functions) and its monthly refresh (`+0x2b0`, `0x81e280`) ([architecture.md](architecture.md), "Time"); and the commands, executed in the same frame as the day, reach the interface as the player's own actions do.

So the day does not leave the interface and the graphics alone while it runs: it makes and destroys windows, textures and models, sets the map mode and draws a combat result into a texture of its own, in the middle of its phases, from a country's, a province's or a battle's own code. A drawing on another thread meanwhile would read the GUI manager's windows, the textures and the map's scene as they are changed. Which of these places a day of a late game reaches, how often and how far into the day, was not measured.

## What the drawing reaches of the game state (read 2026-11-09)

The same graph followed from the drawing (`0x8183b0`) reaches 1,936 functions, 126 of them referring to the world (`[0x233fe78]`) and 187 to the country manager (`[0x233d8d0]`): nearly all through its first step, the map mode's colors (`0x10a8c30` → `0x12fd9b0` → `0x12f0b40`), which works each province's color out from the live game state ([map-modes.md](map-modes.md)). Followed without that step, it reaches 950 functions:

- **The random generator** (`0x2325220`), its count and the random-forbidden flag (`0x1fd10c4`): no function refers to them.
- **The world**, 16 functions, **the country manager**, 11, **the map** (`[0x233fed8]`), 9, in these places:
  - the map's names (`0x10a9be0`: `ADJECTIVE`, `AREANAME`, `ENCLAVE_NAME_FORMAT`), made again from the 3D map's drawing (`0x10a59a0`) when they are asked for, and from the map mode's set (`0x821ff0`) in the day: they read the countries and the provinces, and make their meshes through the back end (`0xfd7140` → `0xfd58b0`);
  - the map's off-screen passes (`0x10a51e0` → `0x12dab90`, `0x12efe40`, `0x13bd3a0`), reading the world and the map;
  - the map's views (`0x13f9520`, `0x12db0c0`, `0x13c36b0`), reading the world;
  - `0x6e7a50`, called by the drawing itself, reading the world and the map (_inferred_ from its neighbor `0x6e80ee` and the debug text's `sea volume:` and `plains volume:`: the ambient sound by the terrain under the camera);
  - the debug text, from `0x818aff`: the country `[0x2340040]` names, its money, its advisors' costs (`0x1ac5e0` → `0x1ac650`), its modifiers (`0x8e8ca0`) worked out through the country's cached data (`0x93d750`, `country_cached_data.cpp`, `CCountryModifiers::EState::CALCULATING`), which works cached values out and keeps them: the one place found where the drawing writes game state, and only with the debug text on.
- **The GUI's drawing** (`0x14dd010`, through `0x14dd350`) reaches nothing by direct calls: it calls each of the frame's 2D objects through its vtable, its `+0xb8`, its `+0x98` and its `+0x58`, which draws. Of every vtable in `.rdata` (runs of pointers to functions' starts), four have a `+0x58` that reaches the world or the country manager by direct calls, and none of the four is a GUI object's: the world's own key reader (`0x751cd0`, vtable `0x1caffe8`) and three others (`0x970f60`, `0xb459d0`, `0x92e420`). So the GUI's objects draw from what they hold, as far as direct calls show; the views set it at their update ([architecture.md](architecture.md), "Presentation"). This agrees with what the write watch saw the GUI's drawing read up to its first wait in the driver, mostly blocks of 0x218, 0x288 and 0x2e0 bytes, the GUI's own objects ("The drawing on a thread of its own while the next day runs").

So, the map mode's colors and the debug text set apart, what the drawing reads of the game state is the map's: its names when made again, its passes and views, and the three fields of every province the 2026-11-08 watch found. What it reads through pointers beyond the map, and the GUI's drawing past the driver's first wait, are still to be watched: the watch held the driver's thread there, which a drawing whose Direct3D calls go nowhere would not need ([drawing-thread.md](../plans/drawing-thread.md), D2).

## What to do next

1. **Measure**, without building anything: `cargo xtask profile --threads --waiting-in 0x179e90` on the 1700 and 1780 saves ([late-game.md](late-game.md), "Saves to test with") already gives the AI stage's wait; the frame's time split into the game's code and `d3d9.dll` and the driver was measured on 2026-11-08 ("The Direct3D calls on a thread of their own"); what is left is the wait set beside the frame's length, day by day.
2. **Show the frame in the stage is safe**: the write watch over frames inside a day in several windows, against the AI stage's reads; the pool's code read for a second job in flight.
3. Only then a prototype of the frame in the spin, behind a test switch of the developer build, measured as [game-speed.md](../plans/game-speed.md) measures its parts.
4. The drawing on a thread of its own last, if ever: set aside, with what it would still need, above ("Where this research stops"). Since 2026-11-09 it has a plan of its own, which takes up what it would still need as its first parts ([drawing-thread.md](../plans/drawing-thread.md)).

## Open questions

- What `0x169bd80`, `0xe0fe50` and `0x10a74c0` divide among the pool's threads.
- How the pool (`0x1798a0`, `0x179e90`) behaves with two jobs in flight.
- What the frame between days must do at speed 5 and what it may leave out.
- Whether the main thread's wait in the driver is for the driver's queue or for the graphics card, and how it is in a game the player sees.
- Which of the places where the day reaches the interface a late day reaches, how often, and how many milliseconds into the day the first of them comes; and which the calls through pointers add.
- Which of the in-game frame's steps before its commands run in every frame, and what each changes.
- Whether the day moves the units' models (their positions on the map) as it moves the units, or the drawing reads the units' positions itself.
- What `0x109c020` does to the camera in the in-game frame, and what besides the drawing reads the camera (the map's markers, picking, the ambient sound, the views placed on the map).
- How long the interface's update takes at each of the game's points for a frame inside a day, and whether the game's state there is whole for every view (the game draws its own frames there, but only once its interval has passed).
- What the drawing reads through pointers outside the map, and the GUI's drawing past its first Direct3D call.
