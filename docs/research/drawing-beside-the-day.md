# Drawing beside the day (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Asked by the owner on 2026-11-07: whether the game's logic and its drawing can run apart, on different threads, so that the game runs faster. Worked out from what was measured before ([main-thread.md](main-thread.md), [late-game.md](late-game.md)) and from the game's direct calls (an E8 and E9 scan of `.text`, each site mapped to its function through `.pdata`; calls through pointers are not in it). Nothing here was tried in the game. The frame, the day and the threads are in [architecture.md](architecture.md); the bar a change must meet is [ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/making-the-game-faster-leaves-what-it-computes-as-it-is.md).

In short: **a simulation thread and a drawing thread side by side cannot be made from the game's code**: its drawing reads the live game state while the day changes it, with nothing between them. Two narrower ways are open. **A frame drawn while the main thread waits for the AI stage**, which late in a game waits 11 to 22 ms a day doing nothing, could take most of the drawing off the day's path: up to 12 to 18% more days a second late in a game, and smooth frames for nothing. It is new ground, since the game never draws while a parallel stage runs, and it needs a write watch to show the frame writes nothing the AI reads. **The Direct3D calls handed to a thread of their own** is of kind A and safe by construction, but would give a few percent at most; the driver's share of a frame should be measured first.

## Where the main thread's time goes, late in a game

From [late-game.md](late-game.md) ("The whole day, late in a game"), 1780 at speed 5, "Faster time" on, an average day of 64.4 ms of the main thread:

| | ms a day | Can it run beside the day? |
| --- | --- | --- |
| The AI stage | 24.2, of it 11 to 22 the main thread spinning while the workers finish (`0x179e90`) | It already runs on the workers; the spin is idle time |
| Each country's daily update | 11.9 | No: the day itself, serial (kind C) |
| The monthly update, its share | 8.8 (266 ms on the month's first day) | No: the day itself (kind C) |
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
- A game started with `-seed` runs the AI's countries one after another on the main thread instead of on the stage ([ai.md](ai.md), "A day of the AI"), so it has no spin to draw in: this way is for a player's game, and a bench on `-seed` ([game-speed.md](../game-speed.md), S5) cannot measure it.
- The device is made `D3DCREATE_MULTITHREADED` and the frame runs on the main thread as always, so nothing changes for Direct3D.

**What is not known, each to be shown before it is built:**

- **The game never draws while a stage runs.** `0x769ea0` is called from the day's phases (`0x758430`, `0x75bbe0`, `0x75b0d0`, `0x75b2d0`, `0x75b4b0`, `0x75d690`), between stages, never from inside one. A frame in the spin is a frame at a point the game was not written for.
- **The frame runs stages of its own on the same pool.** Three were found by direct calls: the GUI's update `0x14c5810` → `0x169bd80` (also in the main menu's and the nudge editor's frames), the alerts `0xdf5860` → `0xe075b0` → `0xe0fe50`, and the map's markers under `0x10a6d00` → `0x10a7a90` → `0x10a74c0`; each goes through `0x1798a0` and `0x179e90`. Whether the pool takes a second job while one is in flight, and what the main thread does when it helps with both, was not read. The main thread takes its job's parts itself, so the frame's stage should finish without the workers, but that is not shown.
- **Whether the frame writes what an AI task reads.** The GUI's views and the alerts may work values out and keep them in the countries' or provinces' objects (a modifier set made again, a cache of diplomacy, `0x93fe30`), which an AI task on another thread reads at the same time. If so, the AI's decisions could change, or its read could tear. Shown by the developer build's write watch over a frame inside a day, its writes against what the AI stage reads; the frame's work depends on the windows open, so on several.
- **The shared lock** `[0x2357ba0]` the AI's agents take, and the allocator's locks: a frame that takes them waits for the AI, which costs time but does not deadlock unless the frame holds something an AI task waits for.
- **The bar.** A frame inside a day is allowed by the ADR because it executes no commands and draws no random numbers. A frame beside the AI's tasks must also write nothing they read while they run; that is a proof of a new kind, by the write watch and the code, and whether it meets the bar is the owner's to decide.

## The Direct3D calls on a thread of their own

The game draws through a table of 81 function pointers (`0x1fd10f8`–`0x1fd1378`, [architecture.md](architecture.md), "Presentation"). Each slot could be replaced by one that records the call into a queue, which a thread of the framework's plays to the device, so that the driver's work on the CPU leaves the main thread. The frame's code, which reads the game state, stays on the main thread: this is of kind A, outside the game state.

- **What it could give**: the driver's share of the frame. Paused, `DrawIndexedPrimitive` (`0x16d4f00`) was 4.8% of the main thread, nearly all inside `d3d9.dll`; at speed 5 the driver's share of the 4.9 ms a frame was not taken. If it is a third, about 2.5 ms of a 64 ms day late in a game: about 4%.
- **What it needs**: every slot that returns something (a texture or buffer made, a lock of a dynamic buffer, a lost device, a query) answered on the main thread or made to wait for the queue; the locked buffers' bytes copied into the queue; the device's reset; the framework's own drawing (the framework UI in the Present observer, the font pages it writes, screenshots) through the same queue. The device is `D3DCREATE_MULTITHREADED` and the loading screen is already drawn from another thread ([prototype-log.md](../prototype-log.md), "Two graphics driver failures"), so the device can be used from the framework's thread.
- **Not known**: whether the driver already does its work on threads of its own (NVIDIA's threaded optimization), which would leave less to take.

## What to do next

1. **Measure**, without building anything: `cargo xtask profile --threads --waiting-in 0x179e90` on the 1700 and 1780 saves ([late-game.md](late-game.md), "Saves to test with") already gives the AI stage's wait; what is left is the frame's time split into the game's code and `d3d9.dll` and the driver, and the wait set beside the frame's length, day by day. This says how much each way could give.
2. **Show the frame in the stage is safe**: the write watch over frames inside a day in several windows, against the AI stage's reads; the pool's code read for a second job in flight.
3. Only then a prototype of the frame in the spin, behind a test switch of the developer build, measured as [game-speed.md](../game-speed.md) measures its parts.

## Open questions

- What `0x169bd80`, `0xe0fe50` and `0x10a74c0` divide among the pool's threads.
- How the pool (`0x1798a0`, `0x179e90`) behaves with two jobs in flight.
- What the frame between days must do at speed 5 and what it may leave out.
- The driver's share of a frame at speed 5, and whether the driver threads its own work.
