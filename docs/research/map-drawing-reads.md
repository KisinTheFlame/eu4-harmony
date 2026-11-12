# What the map's drawing reads, and who writes it (read 2026-11-11)

What the in-game drawing (`0x8183b0`) reads at draw time outside the GUI (the map's off-screen passes `0x10a51e0`, the 3D map `0x10a59a0`, the selection box `0x116e7c0`, the sky and the rest `0x10a5f30`, the combat results' views `0xde7480`, the debug text), who writes each of those and when, and how the drawing thread can read each safely while the main thread runs a day. T3b of [drawing-thread.md](../plans/drawing-thread.md) uses it for the map's half: "the map's data copied at the update", so that the drawing reads nothing live (rule 1). Read statically from the 1.37.5 `eu4.exe`, by the disassembler and the direct call graph; runtime observations are identified in their sections and linked run logs, and what follows only from names, strings or shapes is marked _inferred_. The GUI's half (`0x14dd010`, `0x14dd540`) is another note's.

## The drawing proper, in order

`0x8183b0(idler)`, `r15` the in-game idler, after its two first steps (which the framework does on the main thread, `core/src/drawing/prepare.rs`):

| Where | Call | Its arguments | What |
| --- | --- | --- | --- |
| `0x8187da` | `0x14df050` | | the device check |
| `0x818804`–`0x818831` | back end `[0x1fd1200]`, `[0x1fd1350]`, `[0x1fd1348]` | the graphics object `[idler + 0x11c0]`, its `+0x88`, `+0xc8`, `+0x98` | the renderer's context and two states |
| `0x818837`–`0x818866` | | | the graphics object's `+0x544`–`+0x54c` written from the defines at `0x233eaa0`, `0x233ea9c`, `0x233eab8` |
| `0x8188b5` | `0x10a51e0` | the graphical map `[[idler + 0x330] + 0x360]`, the camera `[idler + 0x1200]`, pointers to the idler's bytes `+0x338`, `+0x339`, `+0x33a`, `+0x33c`, `+0x33d` | **the off-screen passes** |
| `0x8188d6`, `0x8188e3` | BeginScene, `0x14ddc10` | the graphics object | |
| `0x81890e` | `0x10a59a0` | the graphical map, the context, the camera, the frame's length `[idler + 0x1658]`, 0 | **the 3D map** |
| `0x81891a` | `0x116e7c0` | `[idler + 0x1b80]` | the selection box: a rectangle from its own floats (`+0x10`–`+0x20`), written by the input |
| `0x818968` | `0x10a5f30` | the graphical map, the camera, a flag from `[idler + 0x12f0] + 0xf5` and `[[idler + 0x1458] + 8] + 0xf5` | the sky and the map's overlay |
| `0x8189bd`–`0x818a20` | `0xde7480` | `[idler + 0x3e8]`, `+0x3f0`, `+0x3f8`, `+0x400`, each while its `+0x160` is set, with `idler + 0x1b88` + 0x230·i | **the combat results' 3D views** |
| `0x818a39`, `0x818abc` | `0x14dd010`, `0x14dd540` | the graphics object | the GUI (the other note) |
| `0x818af3` | `0x6ebd10` | | the GUI manager's (`vic_18`) |
| `0x818aff`– | | `[0x2340040]` | the debug text, only with the console's debug country set; the framework draws such a frame on the main thread (`must_draw_here`) |

(The in-game idler's `+0x11c0` is the graphics object, not the graphical map: the drawing takes the renderer from its `+0x88` and passes it to the GUI's drawing, as `[0x23494b8]` is passed elsewhere; _inferred_ that it is the same object.)

## 1. What each reads, and what it writes

### The off-screen passes (`0x10a51e0` → `0x12dab90`)

`0x10a51e0(map, camera, &idler+0x338, &+0x339, &+0x33a, &+0x33c, &+0x33d)` hands the pdxmap (`[map + 0x58]`) the five bytes, the second only while the world's `+0x1e10` is clear and the map's fade `+0x104` has reached 1, and clears them after (`0x10a52ac`–`0x10a52dd`); when a pass ran it sets the map's names due (`+0x25a0`) and its fade `+0x104` to 0. `0x12dab90` then, by the bytes:

- **The provinces' texture made again** (`0x12efe40(renderer, pdxmap + 0x14a0, flag)` → `0x12ef5d0`): it reads the world (`[0x233fe78]`, `0x74c190`) and the map's database (`[0x233fed8]`), writes four buffers of its own (`0x2458fc8`–`0x2458fe0`), and fills a texture through the back end (`[0x1fd1220]`). **The provinces to make again are a list the world's interface keeps, `[world + 0x1e00] + 0x340`**, a vector of pointers into the provinces (each the province's `+0x154`), which `0x12efe40` reads and empties; when it is longer than a set share of the map's provinces it is replaced by one entry for the whole map (`0x12efe96`–`0x12efefe`).
- the trees' layer (`[pdxmap + 0xe8]`, its slot `+0x30`), made again with it;
- `0x13bd3a0(pdxmap + 0x13c8, pdxmap + 0x13d0)` on the last byte (_inferred_: the lakes or the straits, by the layers' offsets in [architecture.md](architecture.md), "The map");
- then, each frame and by the settings' bytes (`[0x242bde8] + 0x13b`, `+0x138`, `+0x139`), what follows the camera: the trees for the camera (`0x12e8c00`), the terrain for the camera from the map's database (`0x12e5d30(pdxmap + 0x18, [0x233fed8], camera)`), and `0x12dac90`.

**Who writes the bytes and the list.** The in-game frame sets `+0x33a`, `+0x33b` and others at once (`0x814f0a`, a dword store), `+0x33c`, `+0x339` and `+0x33d` by the bits of an event (`0x814f17`–`0x814f2f`); the in-game idler's `0x793ae0` sets `+0x339` and `+0x33c` (after its map mode slot `+0xc8`); and **the day** sets `+0x339` in the provinces' phase (`0x75b2d0` → `0x9f52e0`, at `0x9f5ee7`, after `0x9d53a0` on the province's `+0x154`) and in `0x9f7a12` (no direct caller: a part of a function), which also **appends the province to the list `+0x340`** (`0x9f7a34`–`0x9f7a4d`, a vector's `push_back`, `0xcc220` when it grows). The province's `+0x2d8` there is the world's interface (it is where `+0x339` and `+0x340` live), _inferred_ from the offsets.

So with the drawing thread on, **the drawing thread empties a vector the day appends to, with no lock between them**: the off-screen passes run in the drawing proper, which the drawing thread runs at its pace whatever the day does, and no crossing is caught there (neither `0x9f52e0` nor `0x9f7a12` is among the crossings, and neither enters the graphics' code). A `push_back` that grows the vector while `0x12efe40` reads it frees what the drawing thread reads; one between the drawing thread's read and its emptying is lost. The texture made again also reads the provinces while the day changes them. This is a fault of the drawing thread as built, not only something T3b must take care of. Counted in a hidden game at speed 5 on 2026-11-11, the preparation found a rebuild asked for (`+0x339`) about 870 times in two game years, so nearly every day. Since then the main thread runs the passes at the frame's preparation and the drawing thread's are given bytes that ask for nothing (step 1 below, `core/src/drawing/prepare.rs`; [2026-11-11-passes-at-the-preparation.md](../prototype-log/2026-11-11-passes-at-the-preparation.md)).

### The 3D map (`0x10a59a0`)

`0x10a59a0(map, context, camera, frame length, flag)`:

- **Writes the map's fades** (`+0xf8` a clock's time, `+0xfc`, `+0x100`, `+0x104`, `+0x63c`, `+0x640`) by the frame's length and the camera's height `[camera + 0x680]` against the defines `0x233e9e4`, `0x233e9e8`, `0x233e9ec`; the markers' update reads them ([drawing-beside-the-day.md](drawing-beside-the-day.md), "What the map's drawing writes for the next update").
- Makes the map's names when due (`+0x25a0`, `0x10a9be0`): the framework now makes them at the frame's preparation, so the drawing thread finds them not due.
- `0x10a55c0(map, context, camera, [map + 0x60])`: the shadow pass (_inferred_: `[map + 0x60]` has a render target at `+0x1c8` and matrices for three views at `+8`, `+0x48`, `+0x88`; it sets a render target and clears it): the scene's frustum worked out (`0x15a4fc0`, arithmetic on the camera), then the scene's meshes drawn for each view (`0x1540c70`), then `0x12e9b00`, `0x12db4d0`, `0x15a7020`.
- Unless the world's interface's slot `+0x140` answers true (_inferred_: a full screen window hides the map), **the pdxmap's drawing** `0x12db0c0([map + 0x58], context, camera, fades, [map + 0x60], flag)`: it reads the map mode `[[world + 0x1e00] + 0x2c]` (an int the map mode's set writes) and the settings, draws the borders (`0x12dd3b0` on `pdxmap + 0x1270`), the terrain, the water, the rivers, the straits and the lakes for the camera and its two other views (`camera + 0x248`, `camera + 0x460`, while `0x8023b0` and `0x8023d0` allow), and **writes which provinces are in view** into the world's interface's bytes `+0xc0` and its `+0x308` (`0x12dd5f6`), which the update and, through a window made in a day, the day read.
- **The map's names** (`0xfd7880`, twice, on `map + 0x4e8` and `map + 0x590`, while the settings' `+0x138` or `+0x139` is set): the names' meshes and their font, which `0x10a9be0` makes.
- **The scene** (`[map + 0x18]`, the engine's 3D scene, with its renderer at `+0x88` and its effects at `+0x580` in entries of 0x38 bytes): `0x14dcde0(scene, view, i)` for the camera's view (`camera + 0x30`) and the two others, `0x13f9520(scene, context, view, i)` for each, and `0x14dcd30(scene, view, i)` for each. `0x14dcde0` runs the scene's passes (`0x1540610`, `0x15aa4d0`, `0x15411a0`, `0x1541630`, `0x15422f0`, `0x1541d30`), which draw the entries on the lists at `0x23588d0`, `0x2358840` and `0x2358960`; an entry (the part, the object, its matrix, 0x50 bytes) is put there by a mesh object's slot `+0x78` (`0x15400a0`, the only pointer to it is the mesh object's vtable `0x1d92408` + 0x78), which chooses the part by the camera's distance and copies the object's matrix (`+0x10`–`+0x4f`). Who calls the slot each frame was not found by direct calls (it is called through the vtable); since the drawing thread draws a frame prepared once many times and the units stay drawn, the entries are put there by the drawing itself (_inferred_). So the lists are the drawing's own; what it reads live is **the mesh objects** (their matrices, `+0x50`, `+0x60`, `+0xa0`, their mesh at `+8`) and **whatever holds them**: the map's markers' models (the units' places' `+0x70`, the ports, the cities; made, moved and animated by the markers' update `0x10a6d00` → `0x10a7a90` → `0x1321880` → `0x1326fd0`), the trees.
- **The objects the GUI puts on the map** (`0x13f9520`): a list of three vectors at `0x2358730`, by pass, of objects of 0x130 bytes that the GUI's factory makes from a type (`0x10b2a00`, the token `0x2c7d` + `0x127`, its vtable entry `0x1d580d0`; constructed by `0x13ff1a0`, which puts them on the list; destroyed by `0x13f9c80`, the vtable `0x1d824c8`'s slot, which takes them off). Each holds three textures found by name (`+0x90`, `+0xb0`, `+0xd0`, through the texture manager `[[0x23494b8] + 0x480]`, `0x16c40b0`, which takes the critical section at `[0x242b818]`) and a vector of items (`+8`–`+0x10`), each item drawn while its `+0x50` bit 0 is set, from its `+0x58`, `+0x60` (a buffer drawn by `0x15bd180`), `+0x68` (its `+0x100`), `+0x70`, `+0x74`, `+0x88`–`+0x90`. They are made and let go with the windows that hold them (_inferred_: by the factory being the GUI's).
- A debug drawing when `0x2458f8c` is set (`0x15a6bd0`).

### The sky and the rest (`0x10a5f30`)

It reads the map, the camera and the settings, and draws through the back end with its own effects (`0x14df7b0` the camera's matrices); no world or GUI object was found read.

### The combat results' views (`0xde7480`)

`0xde7480(view, idler + 0x1b88 + 0x230·i)` for each of the in-game idler's four views at `+0x3e8`–`+0x400` that is shown (`+0x160`): it sets a render target, draws the view's own scene (`0x14dcde0`, `0x14dcd30` on the view's scene, which put entries on the same global lists `0x23588d0` and the others as the map's scene does), and sets the target back. The views and their textures are made and changed by `0xde6300` (`3dcombatresult.cpp`), which the day reaches from a message (`0x8c7790`), a crossing.

### Through the drawing, of the world

What the map's drawing reads of the world itself: the map mode (`[world + 0x1e00] + 0x2c`), `world + 0x1e10`, the pointer chain `[[[world + 0x1e00] + 0x330] + 0x360]` to the graphical map (`0x13f9587`), and, in the provinces' texture made again, the provinces. The map's database `[0x233fed8]` (the provinces' shapes, made at the load) is read by the terrain and the provinces' texture.

### The borders, and a core's highlight (read 2026-11-11)

Looked into because with the drawing thread a core's flag in the province view, hovered, no longer outlines that country's cores in red on the map, as the game does without it ([the first run](../prototype-log/2026-11-11-core-highlight-lost.md)). The producer, clearing and reader were traced on 2026-11-12; see "The frame's temporary highlights" below.

- **The pdxmap's border sets**: at `+0x18`, `+0xe8`, `+0x1160`, `+0x11e8`, `+0x1270`, `+0x1398`, `+0x13e8` and `+0x1440`, each an object with a vtable whose slot `+0x20` builds its geometry, `+0x28` lets it go, `+0x30` does both (`+0x1270`'s, vtable `0x1d749d8`: `0x12dceb0`, `0x12dcf40`, `0x12dd0a0`). `0x12db850` lets them all go and clears their `+0x20`, as the map's graphics load (`LOAD_GFX_PDXMAP_BORDERS` and the rest). The drawing draws `+0x1160` (unless the map mode is `0x1a`) and `+0x11e8` with `0x12e0e40`, which draws its list `+0x68`–`+0x70` while its `+0x20` is set; and `+0x1270` with `0x12dd3b0` then `0x12ddda0`.
- **`+0x1270`, the provinces' borders.** Each draw (`0x12dd3b0`), when its list `+0xc8` is empty, `0x12dd0c0` works out every segment's type again from the two provinces it parts (`0x12dbfc0`, the game's provinces at `[0x233fed8] + 0x6f0`, 0x2e10 bytes each, reading their `+0x28` entries' `+0x228` and `+0x22a`); then `0x12dd610` sorts the segments into nine lists at `+0xe0` (0x18 bytes each) for the camera and its two other views (`+0x248`, `+0x460` of the object `0x8023b0` and `0x8023d0` read, while its `+0x778` and `+0x678` or `+0x679` are set; those two are written by `0x109e040`, under the camera's update `0x109c020`). Traced while a core was hovered, `+0x1270` was built once, at the load, and not again.
- **The border colors.** `0x12dca80` (the slot `+0x18` of `+0x1270`'s vtable) writes a table of colors at `0x2358340`, sixteen bytes each, from the `NGraphics` defines `BORDER_COLOR_*` (read by `0x48c940`): the custom highlight at `0x233e964`–`0x233e970`, the custom temporary highlight at `0x233e9b4`–`0x233e9c0` (the sixth entry, `0x2358390`).
- **`+0x1160` and `+0x11e8` are built by `0x12f0b40`**, which also reads the custom country highlight's color (`0x233e958`–`0x233e960`), from the map's color fill `0x12fd9b0`, under the map mode's colors `0x10a8c30` (the drawing's first steps, `0x818652`; the map mode set, `0x6ec450`). Traced for two seconds of a core hovered, with the drawing thread and without, neither `0x12f0b40` nor `0x6ec450` was called: the core's outline is not drawn through these.
- **The core's tooltip** is `0x13626b0` (`COREINFO`, `CORE_HAS_CONTROL` and the rest), called from the province view's tooltips (`0x137d9bc`).
- **The first experiment suggested drawing consumed the highlight**, because moving the tooltip update back after drawing restored it. That inference did not survive the next test: drawing a prepared frame only once still lost the outline. The clearing is the frame's, outside the drawing, as traced below.

### The frame's temporary highlights (read and tested 2026-11-12)

**Producer.** The core or claim tooltip (`0x13626b0`) first calls the temporary country highlight (`0xe1acc0(country reference, true)`, at `0x13626e7`), before building its text. It resolves the country through `0x233d8d0`, takes the world's interface from `[0x233fe78] + 0x1e00`, and:

- For a country with owned provinces (`+0x1344` nonzero), or the special country kind `+0x26 == 7`, sets the country's bit in the interface's bitset at `+0xf0`, growing it through `0xa2ec80` if the index exceeds `+0x300`.
- With the second argument true, walks the country's core list at `+0x1378`. For each core absent from its owned-province array (`+0x1338`, count `+0x1344`), ORs mask 4 into the province byte at `[interface + 0xc0] + province id` (`0xe1adac`–`0xe1adb3`).
- Returns without drawing. Other tooltip callers use the same producer; this work does not claim to establish the behavior of every caller.

The bitset resize (`0xa2ec80`) computes its word count from the requested bits, allocates a larger word array if needed, and calls the word-array replacement (`0xcf9f0`) to copy existing words, invoke the old array's release slot and install the new pointer and capacity. It zeroes newly added words, clears unused bits in the former last word, stores the new word count at `+0x14` and bit limit at `+0x218` of the bitset object. That limit is the interface's `+0x300`. The reader copies both the words and this limit; array identity checks reject a replacement after preparation.

**Clearing.** The in-game frame (`0x8137c0`) calls `0xe1ac80(idler + 0xb8)` at `0x815928`, immediately after drawing returns. Unless the marks object's `+0x256` is set, it calls `0xe1b030(marks, 4)`, which clears just mask 4 in every byte of its `+8` array (signed count `+0x14`), then tail-calls `memset` (`0x1a35e00`) to zero its `+0x38` array (signed word count `+0x44`). Relative to the idler these are the province bytes at `+0xc0`/`+0xcc` and country words at `+0xf0`/`+0xfc`: the same arrays the producer and border reader use. Two other direct callers clear their own marks in other paths (`0xf80d03`, `0x10d3d5b`); none is changed.

**Reader, read to its end.** The border classifier (`0x12dd610`, one direct call at `0x12dd5f1`) first works out the visible border segments for the camera's three views. Its visibility loop (`0x12dda90`–`0x12ddac2`) writes bit 0 of the real province bytes and the selected province at `+0x308`. From `0x12ddac7` to its return it only reads the interface through `r11`; there are no calls in that suffix. The fields it reads are `+0x30c/d` (display flags), `+0xc0` (province flags), `+0xd8` and `+0x308` (the other border grouping), `+0x300` and `+0xf0` (temporary country highlights). It writes the classified border output through separate pointers, not through `r11` or those input arrays. The only late callee in the whole function is the CRT static initialization path before `0x12dd7c0`, before this suffix.

- At `0x12ddbfb`/`0x12ddc05`, either province byte carrying mask 4 selects border color index 6 (`0x12ddc97`), the custom temporary highlight.
- Otherwise the owners of the two provinces are tested against the temporary country bitset (`0x12ddbb6`, `0x12ddbd9`), bounded by `+0x300`. A marked owner selects index 7. The province and country markings together implement the core tooltip's owned and unowned cores.

**Causal experiment on master 0.1.4.** With the drawing thread, eight hover screenshots had red-pixel counts `[6, 6, 6, 6, 6, 6, 6, 6]`. Rebuilding with only the clear call at `0x815928` replaced by a no-op, then starting that completed build, gave `[6, 6, 50, 704, 846, 389, 10, 6]` and visible red outlines. The outline pulses, so its absence in one screenshot proves nothing. Skipping clearing is an experiment only: it leaves old targets marked.

**Prepared inputs, without changing a shared pointer.** `core/src/drawing/marks.rs` captures the province bytes, country words and country limit after preparation and before publishing `PREPARED_SEQ`. The main thread still clears the real arrays at the original call. At `0x12ddac7` on the drawing thread only, a stub replaces the classifier's local `r11` with a thread-local input prefix. Its province array combines the live flags, including the visibility bit just written, with only mask 4 from the prepared frame; its country array and country limit are that frame's. Other prefix fields retain their current values. The classifier suffix has no call or write through these inputs, so it cannot expose the copied pointers to another subsystem.

The presentation lock excludes the next preparation while the drawing consumes a frame. Each preparation replaces the kept inputs, including an empty highlight; the classifier copies them into its own reader, whose vectors and prefix stay put until that call returns. Array identities, counts and idler identity must still match; otherwise it uses the original input. Preparation reuses the kept arrays' capacity through `resize` and `Memory::read_into`; a validity flag is cleared before any capture read and set only after both arrays and their limit are read successfully. A failed or partial capture leaves the storage allocated but unavailable to the classifier, so no previous frame's marks are reused. A new idler cannot use the previous idler's marks, and after an in-game load the next preparation replaces the inputs before publishing its frame. Main-thread classification keeps the original `r11`. The day and the game during Present always see the game's original arrays: the stub changes only one thread's register, and runs after visibility writes, before Present. This resolves the temporary-input lifetime; it does not establish that every other live read of the border classifier is safe beside a day.

## 2. Who writes each, and when

| What the drawing reads | Written by | When |
| --- | --- | --- |
| The idler's pass bytes `+0x338`–`+0x33d` and the provinces' list `[world + 0x1e00] + 0x340` | the in-game frame (`0x814f0a`–`0x814f2f`), the idler's map mode (`0x793ae0`), **the provinces' phase of the day** (`0x9f5ee7`, `0x9f7a12`) | the update; **the day, with no crossing** |
| The map's fades, the provinces in view | the drawing itself | each frame drawn |
| The map's names' meshes (`map + 0x4e8`, `+0x590`) | `0x10a9be0`: the framework at the frame's preparation; **the map mode's set in a day** (`0x821ff0`, a crossing) | the preparation; the day |
| The provinces' color texture | `0x10a8c30`: the drawing's first step, at the preparation; **the map mode's set in a day** (`0x821ff0`); the framework's map modes (`core/src/map.rs`) | the preparation; the day |
| The mesh objects | made by the markers' update; let go by the markers' update, a unit's destructor and place reset, a port's model, the commands' step (`0x13235a1`) | the update; the day and the commands (now let go at the preparation, `core/src/drawing/deferred.rs`) |
| What holds the mesh objects: the markers' models (_inferred_: entities of `pdx_entity.cpp`, `0x14f0c90` among their code) | the markers' update; let go by a unit's destructor (`0xd3b147`) and place reset (`0x14488c3`), a port's model (`0x13238b0`, which clears a province entry's bit, `0x132390a`) | the update; **the day** (crossings) |
| The GUI's objects on the map (`0x2358730`) | made and let go with their windows | the update; **the day, as windows are made** (crossings) |
| The combat results' views | `0xde6300` | **the day** (a crossing) |
| The scene's global entry lists (`0x23588d0`, `0x2358840`, `0x2358960`) | the drawing itself; **`0xde6300`'s drawing into its texture on the main thread** | each frame; the day |

The province entries' flags at `[map + 0x70] + 0x1400·i + 0x13f8` (the owner mark `|= 8` and others) are read by the markers' update (`0x10a71da`, `0x10a7e62`), not by the drawing, as far as direct reads show: the update is where they take effect.

## 3. The cheapest safe read, each

| What | How the drawing thread can read it safely | Size |
| --- | --- | --- |
| The off-screen passes' rebuilds (the provinces' texture, the trees, `0x13bd3a0`) | **done on the main thread at the frame's preparation**, with the bytes and the list, as the drawing's first steps are; the drawing thread then runs `0x10a51e0` with the five bytes cleared, which leaves only what follows the camera (`0x12e8c00`, `0x12e5d30`, `0x12dac90`), reading the camera and the map's database | nothing copied |
| The names' meshes, the provinces' color texture | written only at the preparation: the map mode's set (`0x821ff0`) deferred to the preparation (C2) | nothing copied |
| The mesh objects | kept alive to the preparation (done) | nothing copied |
| The markers' models and the ports' | **their let go kept to the preparation**, as mesh objects are: the models' scalar deleting destructor deferred (its class to be read), so that the markers' lists keep pointing at live objects until the update takes them off; then the unit's destructor, place reset and the port's model need not hold the frame back | nothing copied |
| The GUI's objects on the map | with the windows: their making and letting go at the preparation (the GUI note's), or their destructor `0x13f9c80` deferred like mesh objects | nothing copied |
| The combat results' views | `0xde6300` deferred to the preparation (C2), which also takes its drawing into a texture, and its use of the global entry lists, off the day | nothing copied |
| The map's fades, the provinces in view | the drawing's own; the update reads them under the lock at the handover. The provinces in view written into the world's interface (`+0xc0`, some 700 bytes for 5,000 provinces, and `+0x308`) are read by the day through a window made in a day: a copy at the handover, or that window deferred | 0.7 KB a frame, if copied |
| The map mode, `world + 0x1e10`, the settings | written by the update and by the map mode's set; a word read whole | nothing |

So the map's half needs no copy of any size: every writer the drawing must not race either runs at the preparation already, or is a call of the day's that can be moved there (deferred, C2), or an object's letting go that can wait for it. The provinces' arrays (some 5,000 entries of 0x1400 bytes, 6.4 MB) are not read by the drawing and need no copy.

## 4. The crossings left, and the map

| Crossing (`core/src/drawing/crossings.rs`) | What it does to what the map's drawing reads |
| --- | --- |
| `0x81e280`, the interface's monthly refresh | windows' elements (`0x14e6170`, `0x14e4570`); floating texts, deferred. Not the map (the GUI's) |
| `0x8c1430`, a message's popup | windows; through the combat result (`0x8c7790` → `0xde6300`) the combat results' views and their textures |
| `0x10a03f0`, a window made | windows; the GUI's objects on the map if the window holds any (`0x2358730`) |
| `0x821ff0`, the map mode set | the provinces' color texture (`0x10a8c30`), the names' meshes (`0x10a9be0`), `0x10af270`, `0x12dd0c0`, and the pass bytes (`0x793ae0`): **the map** |
| `0x1347a50`, the map's markers of a province | the markers (`strait_indicator`, `river_indicator`, `province_terrain`) through the GUI manager: **the map's GUI objects** (_inferred_: those of `0x2358730`) |
| `0x13238b0`, a port's model | a port's model on the markers' lists, and a province entry's bit 1 cleared: **the map's scene** |
| `0xded0f0`, the new age's window | a window. Not the map |
| `0xde6300`, a combat's result drawn into a texture | **the combat results' views**, a render target, textures made and destroyed, and the scene's global entry lists on the main thread |
| `0xe428b0`, a country's window to the player | windows. Not the map |
| `0x123f800`, an item of the message list | a window's item. Not the map |
| The unit's destructor `0xd3b130`, the place reset `0x1448880` | **the unit's model** on the markers' lists let go |
| The renderer's scene begun `0x16d4e00` | the device and the renderer's state (its vertex declaration `+0x108`), whatever draws on the main thread |

## 5. A design for the map's half of T3b

1. **The off-screen passes' rebuilds on the main thread.** At the frame's preparation (`prepare.rs`, beside the first steps), call `0x10a51e0` with the idler's five bytes as the drawing would, which makes the provinces' texture and the trees again from the list, clears the bytes and sets the names due; then make the names. The drawing thread's call at `0x8188b5` is given cleared bytes (a stub over the five pointers, or the bytes copied at the preparation into a block of the framework's that the drawing thread's call points at), so it only does what follows the camera. This also ends the race on the provinces' list, which exists now.
2. **The day's writers of what the map draws, deferred to the preparation** (C2): the map mode's set (`0x821ff0`), the combat result (`0xde6300`), the markers of a province (`0x1347a50`), a port's model (`0x13238b0`), as the minimap now is: each answers nothing its callers read was to be checked one by one; the map mode's set is a slot of the idler's (`+0xc8`), called through the vtable.
3. **The models' let go deferred**, as mesh objects are: the class of the units' places' `+0x70` and of the ports' models, and their scalar deleting destructors, taken over; then the unit's destructor and the place reset are no longer crossings.
4. **The GUI's objects on the map** follow the GUI's half: their windows made and let go only at the preparation, or their destructor deferred.
5. The rest the drawing thread reads (the mesh objects, the meshes, the textures, the camera, the map's database) is written only at the preparation; the lock at the handover covers it, and the catches at the bottom (the scene begun, the mesh object let go) can go once every writer above is moved.

Risks:

- **The provinces' texture made again on the main thread** costs the main thread what it cost the drawing; how long `0x12efe40` takes, and how often a day asks for it, were not measured.
- **The bytes cleared for the drawing thread** must still let the per-camera parts run; `0x12dab90` takes the per-camera parts from the settings, not from the bytes, so they run either way (_inferred_ from its code; not seen).
- **A deferred map mode's set** shows the new map mode a frame later in a day; its callers in the day (the capital moved, `0x3041b0`) were not read to their ends for what they read back.
- **Models kept past their let go** stay on the markers' lists until the update, as mesh objects now do; a model whose owner is freed (the unit) must not be read through the unit by the drawing, which direct calls do not show but pointers might.
- What calls the mesh objects' slot `+0x78` each frame, and so what exactly holds them at draw time, was not settled by direct calls.

### What was built of it (2026-11-11)

- Step 1, the off-screen passes at the preparation: built.
- Step 2, in part. **The map mode's set** has nearly a hundred direct callers, the interface's buttons among them, which read the map mode back; only the moved capital's call (`0xa1a18f`, which reads nothing after it, in a day or in the commands' step) is deferred, known by where it returns to (`0xa1a194`). **A port's model** likewise only from the center of trade's update (`0x474d11`, returning to `0x474d16`); the markers' update's own calls run at once. **The combat result** (`0xde6300`) is called while a message's popup is built (`0x8c8558`, `0x8c8f68`, the view stored at the window's `+0xe0` just before), which is itself a crossing: deferring the view alone would leave the popup's window half made in the day, so it stays under the lock. **The markers of a province** (`0x1347a50`) work against the province the markers show (`[+0x958]`), so their order against the interface's own calls matters; rare, it stays under the lock.
- **A deferred call that frees a mesh object**: the deferred calls are done at the preparation, after the update that takes a model off the markers' lists; a port's model done there let its mesh object go at once (the deferred calls were not deferred again), while the lists still held it, and the drawing thread faulted on it in the shadow pass (`0x1540f20`, the object's parts at `+0xa0` gone, from `0x10a57ad`; [2026-11-11-port-models-deferred.md](../prototype-log/2026-11-11-port-models-deferred.md)). A mesh object the main thread frees now always waits for the next preparation, also when a deferred call frees it.
- Step 3, the models' let go deferred, and step 4: not built. With the mesh objects kept to the preparation, a model let go in a day leaves no freed mesh object on the lists; the unit's destructor and the place reset stay crossings, under the lock.

What static reading cannot settle: how often the day appends to the provinces' list, and whether the race above has faulted (it would be a crash in `0x12efe40` or `0x12ef5d0` on the drawing thread); the class of the markers' models; what reaches the mesh objects' slot `+0x78`; whether any pointer path from the drawing reaches a unit or a province changed in a day beyond those listed. A write watch on the drawing thread's frame while a day runs, as [drawing-beside-the-day.md](drawing-beside-the-day.md) did for the main thread, would answer the last.
