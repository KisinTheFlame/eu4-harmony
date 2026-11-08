# The startup's stretches side by side

Status: decided 2026-11-09, after [research/loading.md](../research/loading.md) ("Which stretches can run side by side"); B1 asked for by the owner the same day, built and tried the same day ([the run](../prototype-log/2026-11-09-mesh-types-beside-the-databases.md)), and taken out again that night, after two starts crashed in the map logic while its thread read a file ([the crashes](../prototype-log/2026-11-09-mesh-types-crash-in-the-map-logic.md)). It follows [loading.md](loading.md), a record, and meets the same bar ([ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/2026-11-04-making-the-game-faster-leaves-what-it-computes-as-it-is.md)): what the game loads stays what it loads, only sooner.

## Goal and scope

- **Goal**: the main menu comes sooner, the startup's stretches that need nothing of each other running at the same time instead of one after another, with the same data loaded.
- **In scope**: the startup load from the gfx stage to the main menu: the LoadingThread's stretches and the main thread's part after them.
- **Out of scope**: the databases' own order, and the events after them, which need the databases they name; work inside a stretch, which [loading.md](loading.md) shortened; a save's load.

## What the game does

The startup load runs on the LoadingThread while the main thread waits for it, then the main thread makes the main menu ([game/program.md](../game/program.md)). After L1 to L5 the LoadingThread's path is about 17.6 s: the gfx stage 2.2 s, the meshes 2.2 s, the databases 6.0 s, the events 3.9 s, the flags 0.6 s and the rest of InitGame 2.4 s; then the main thread's part takes 4.5 s, the map's graphics 2.1 s of it ([research/loading.md](../research/loading.md), "The startup after L5, broken down"). Which of them could run beside which, and on what conditions, was read from the game's code, with the machinery they share and whether it is safe from two threads ([research/loading.md](../research/loading.md), "Which stretches can run side by side").

## Behavior

Nothing the player sees changes but how soon the main menu comes. `setup.log` times each stretch from the LoadingThread, so a stretch moved off it shows as shorter there.

## Parts

### B1 The mesh types loaded beside the databases

At the end of the gfx stage the LoadingThread spends about 2.2 s in the meshes (`0x14dc5d0`), nearly all of it in `0x14dc390`, which loads every mesh type (its `.mesh` file, its vertex and index buffers, its textures, its skeleton) and then makes the particles' shaders. Nothing the databases load reads what it makes; what the two use alike (Direct3D, made multithreaded; PhysFS; the log; the heap; the texture lookups) is safe from two threads.

- At the startup's first call, `0x14dc390` runs on a thread of the framework's, and the LoadingThread goes on with the rest of the meshes (the mesh objects before it, the meshes' shaders and the sprites' vertex declaration after it, which neither need it nor are read by it), the rest of the gfx stage and the databases.
- The LoadingThread waits for that thread as the databases end, at the DLC's metadata (`0x2060f0`), which InitGame calls next: the events, the flags and what follows them, the main menu and every drawing of a mesh or a particle come after it, as they do now. A check of the device (`0x16d8e20`) that finds it lost waits for it too, before the game lets every device object go and makes it again.
- A later call, such as after the device's reset, loads the mesh types on its caller's thread, as the game does.
- The mesh types loaded, in the same order, and every other step stay what they were; only the time changes: `setup.log`'s "Loading Animated Map-Sprites" line counts only what is left on the LoadingThread.
- Shown: three starts each way, the part on and left to the game (`game-paths`): InitGame began 2.21, 2.13 and 1.73 s after the gfx stage did, against 3.83, 3.12 and 3.19 s; the databases took 4.95, 4.91 and 4.56 s against 4.93, 4.20 and 4.62 s, and ended after the mesh types each time. An ironman save loaded each way and written by the game after its load: `gamestate` the same but for the 4 bytes at 510, `meta` but for its UUID and its closing checksum, `ai` the same; the units' models drawn alike ([research/loading.md](../research/loading.md), "The startup with B1").
- Taken out (2026-11-09): two of about twelve starts of a test game on a busy machine crashed on the LoadingThread in the map logic, which the databases run, while the mesh types' thread read a `.mesh` file ([the crashes](../prototype-log/2026-11-09-mesh-types-crash-in-the-map-logic.md)). Until what the two share is known, the mesh types load on the LoadingThread again, as the game does.

### Not planned yet

What [research/loading.md](../research/loading.md) found possible beyond B1, each a part of its own if the owner asks for it: the map's graphics begun on the main thread once the map logic has run (up to 2.1 s, on four conditions: the meshes ended, one thread drawing the loading screen at a time, no dynamic token made while it looks up its tokens, no device reset meanwhile); the flags beside the events (at most 0.6 s, with the dynamic tokens made under the tokens' lock).

## How it fits together

`loading::meshes` in the core was B1, until it was taken out: a speed-up of `crate::speed` (`meshes`, the group `loading`), as [loading.md](loading.md)'s parts are: a developer build's `game-paths` leaves it to the game. It has nothing to check against the game's own: the mesh types are the game's, loaded by the game's code, only on another thread.
