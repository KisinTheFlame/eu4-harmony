# Drawing thread plan

Status: proposed (2026-11-09), not decided; nothing built. It answers the owner's question of 2026-11-09, how the game's work and its drawing could be put on two threads, from what is known of both ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md)). Its first parts are measurements and proofs that decide whether the later ones are built; whether to take it, against the other ways of [game-speed.md](game-speed.md) ("Which part comes next"), is the owner's. Terms it adds to [GLOSSARY.md](../../GLOSSARY.md): Drawing thread, Crossing. The bar it meets is [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md): its differences are of kinds A (the drawing, outside the game state) and B (the pace of frames) of [game-speed.md](game-speed.md), "What 'the same game' means"; nothing the day computes changes.

## Goal and scope

- **Goal**: the main thread stops drawing. The days are worked out on it as now, with the interface's update between them; each frame's drawing (the map, the interface, Present) is done by a thread of the framework's while the main thread goes on with the day.
- **What it gives**, at most: the drawing's share of the main thread. Late in a game at "Faster time" that is 5.5 ms of a 69 ms day, about 8% more days a second ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The Direct3D calls on a thread of their own"); at the game's own pace ("Smooth map") the drawing was 16% of the main thread in 1624, most of it in the frames inside a day ([research/main-thread.md](../research/main-thread.md), "The shares"), so the smooth map would cost the day little more than the frames' update, and the choice "Faster time" makes between a smooth map and fast days would mostly go.
- **Set aside, with why**:
  - **A simulation thread and a drawing thread each at its own pace**, the drawing with its own update of the interface: the interface's views, tooltips and map modes work what they show out from the live game state as they update, and the day changes the game's containers, not only values; it would need a copy of the world for the interface each day, or the interface written again ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "A simulation thread and a drawing thread"). This plan moves only the drawing, which reads little of the game state.
  - **The day moved to a thread of its own**, the main thread keeping the frames: the frames inside a day are called from inside the day's code, the window and SDL's events belong to the main thread, and the device is reset there. Moving the drawing gives the same and changes less.
  - **The Direct3D calls recorded and played by a thread**: kind A and safe by construction, but at most 3.4 ms a late day ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The Direct3D calls on a thread of their own"); it may come later on top of this plan, the drawing thread playing them.
- **Out of scope**: what the day computes; the main menu's frame, the nudge editor's and the loading screen (the same mechanism could take the main menu's later); multiplayer, where nothing the machines compute changes and so nothing needs to.

## What the game does that this rests on

- **A frame in a game** runs, in order: the interface's steps before the commands (a pending dialog, the GUI's, the date's text), the commands, the daily tick among them, which runs the day; the outliner, input, the time step, the alerts, the map's markers, the GUI's update; and **the drawing last** (`0x8183b0`): the map mode's colors when they changed, the device check, the map's off-screen passes, the 3D map, the sky, the GUI, the debug text when asked for, Present ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The in-game frame's steps, in order"; [game/program.md](../game/program.md), "A frame").
- **Frames inside a day** run that whole frame between the day's phases once the day has run past an interval, without commands and random draws ([game/day.md](../game/day.md), "Frames inside a day").
- **The day reaches the interface and the graphics** in at least 48 places by direct calls, besides the frames inside a day and the interface's daily and monthly refreshes: popups and messages, a combat result drawn into a texture, dialogs to the player, the map mode set again with its colors and names, the map's markers when a province changes owner, a port's model, the minimap, a unit's model, a reform's animation, the new age's window ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the day reaches the interface and the graphics"). These are the **crossings**.
- **The drawing reads little of the game state**: its first step, the map mode's colors, works every province's color out from it, and the debug text reads (and through a country's cached data writes) a country; without them the drawing reads the map's names when they are made again, the map's passes and views, and three fields of every province each frame, draws no random numbers, and the GUI's drawing draws from the GUI's own objects as far as direct calls show ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "What the drawing reaches of the game state", "The drawing on a thread of its own while the next day runs").
- **The device** is made for several threads (`D3DCREATE_MULTITHREADED`), and the game already draws its loading screen from another thread ([game/interface.md](../game/interface.md), "The frame").

## Behavior

- **What the player sees** does not change: each frame shows what the interface's update prepared for it, as now; the frame comes to the screen as soon as the drawing thread has drawn it, while the day goes on. Input is read at the update, as now.
- **What changes is how long a day takes**: the day's next phases run while the frame is drawn, instead of after it.
- **A setting**, in the settings window's "Game speed" group, beside "At the fastest speed": "Drawing" with a choice bar, "On the main thread (as in the game)" and "On a thread of its own"; the game's own way until the parts below show the other safe, then the other by default if the owner chooses so. Its words in the player language are written with D6.
- **The debug text** (the console's debug country) needs the drawing on the main thread: while it is on, the frame is drawn there, as in the game.

## How it fits together

### The two threads

- **The main thread** keeps everything but the drawing: the window and SDL's events, the platform services, input, the commands and the days, the interface's update, extensions and remote control, and of the drawing's own steps the map mode's colors and the device check, lost device and reset, which run before the job is handed over.
- **The drawing thread**, one, made by the framework at startup, at the main thread's priority, takes one **draw job** at a time: the rest of the drawing, from the map's off-screen passes to Present, with the framework's Present observer (the framework UI, screenshots). The main thread hands the job over where it would have called the drawing, and goes on.

```text
main:     update₁ ─ hand over ─ day, phase k ──────── crossing? wait ─ phase k+1 … ─ update₂ (waits) ─ hand over ─ …
drawing:                       └── draw job₁: map, GUI, Present ──┘                                └── draw job₂ ──
```

### The rules that keep the two apart

1. **The interface's update and the drawing take turns.** Before any step of the interface (the in-game frame's steps around its commands, a frame inside a day, an idler switch, a device reset, a session left, a save loaded), the main thread waits until the drawing thread's job has ended. So the interface's own objects, its windows, sprite types, textures, the fonts' caches and the map's scene, are never used by the two threads at once, and need no lock.
2. **The day waits at each crossing.** While a job runs, the day's code that goes into the interface or the graphics waits for the job to end first, then goes on as the game's code goes now. A crossing is caught twice over:
   - at its entry, an observer on each place of the research note's table;
   - at the bottom, whatever path led there: the main thread's calls into the back end's table and into the device's own methods (D3DX calls those), and the GUI manager's making and killing of windows; these catch the crossings the direct call graph missed, which go through pointers.
   A crossing that comes often and early in the day, so that the job always makes the day wait, may instead be put off to the next update (D5), where it returns nothing the day reads on.
3. **What the drawing reads of the game state is copied, moved or shown harmless.** The three fields of every province are scalars in the province array, which does not move during a game; a byte read as the day writes it gives a frame's wrong pixel at worst, and none of the three was written by the days watched. The map's names, when they are to be made again, are made at the update, before the job is handed over. What D2 finds the passes and views read is each moved to the update, or copied there, or shown to be such a scalar.
4. **The drawing writes nothing the day reads, and draws no random numbers**: none found by direct calls; D2 shows it by the write watch, with the debug text off.

### Where the drawing runs beside the day

- **After a frame inside a day** (at the game's pace): the job runs beside the day's next phases, up to the next crossing, the next frame inside the day or the end of the day. Late in a game most of the frames drawn at the game's pace are of this kind ([research/main-thread.md](../research/main-thread.md), "The shares").
- **After the frame between days** (at "Faster time" and at the slower speeds): the job runs beside the next frame's first steps, and beside its day only once those steps before the commands either touch nothing the job uses or wait only when they do something (D4). Until then this frame's job ends before the next frame's interface steps, and gives only what Present waits.

### The device and the window

- One device, used by the drawing thread during a job and by the main thread between jobs, never by both at once (rules 1 and 2); the device check, a lost device and its reset stay on the main thread between jobs, where the game does them.
- Present is called on the drawing thread; a failed Present is told to the main thread, whose next frame waits its 100 ms one frame later than now.
- The window, its messages, SDL's events and the cursor stay the main thread's.

### The framework's own parts

- **The Present observer** runs on the drawing thread: the framework UI's state, set on the main thread by extensions, remote control and the settings window's input, is handed to the job with it; a screenshot is taken in the job and answered on the main thread.
- **The Unicode module's font pages**, written as text is drawn and measured, are written by the drawing thread in a job and by the main thread at the update, never at once (rule 1).
- **The sampler** (`cargo xtask profile`) names the drawing thread and sorts its samples by the drawing's parts, so the main thread's samples show what the job took off it.
- **Extensions and remote control** run at the in-game frame on the main thread, as now; a capability that reads the interface (the player's view, a window) runs at the update.
- **In the `speed` module** it is one speed-up of the group `frames`, which a developer build can leave to the game; its dependency points are the drawing's call and its split, each crossing's entry and the catch-alls, held to the framework's whole startup like every other ([ADR framework-succeeds-or-fails-as-a-whole](../adr/2026-10-27-framework-succeeds-or-fails-as-a-whole.md)).

## Parts

### Every part is done when

As in [game-speed.md](game-speed.md), "Every part is done when": the checks pass, the gain is measured on its own target before and after, the run is a file of its own in [prototype-log/](../prototype-log/), what the research found is in [research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md) and the pages of [game/](../game/) it changes, and what players see is in README.md and README.zh-CN.md.

### D0 Where the crossings come

A measurement, with the call trace's `times` ([dev-tools.md](../dev-tools.md), "Call tracing") on the crossings' entries, on the frame inside a day and on the drawing, over late days at both paces, from the saves of [research/late-game.md](../research/late-game.md) ("Saves to test with"): for each frame, how long its drawing took and how long the day ran after it before its next crossing or frame. That bounds what each window gives, in ms a day, before anything is built; if crossings come within a millisecond or two of each frame, the plan stops here or goes to D5 first.

### D1 Every crossing found

A development tool that records, while a day runs, each time the main thread goes into the interface's code, the back end's table, the device or the GUI manager, with its call stack, counted by stack, over decades of an AI-only late game and a player's game with windows open. Every stack is one of the crossings caught, or becomes one. It is a lasting development capability, so its shape is talked through with the owner before it is built ([game-speed.md](game-speed.md), "How a part is chosen"). D3 keeps it on as a check in long runs.

### D2 What the drawing reads

The write watch over the drawing, with the back end's table answered by stubs for the watched call, so that the GUI's drawing does not wait in the driver, which the watch held on 2026-11-08; its reads of the game state against the next day's writes, in several windows of the interface; and its writes against what the day reads. Out: the list of what the drawing reads of the game state, each moved to the update, copied there, or shown harmless (rule 3); and that it writes nothing the day reads (rule 4).

### D3 The drawing thread, after frames inside a day

Behind a developer build's switch: the drawing split at its device check, the drawing thread and its jobs, the waits of rules 1 and 2 with both catch-alls, the map's names made at the update, the Present observer on the drawing thread. Measured: days a second at the game's pace late in a game, with it on and off; frames a second; screenshots of a paused game the same, pixel for pixel; an AI-only run of decades with D1's tool on, which finds no crossing that did not wait.

### D4 After the frame between days

The in-game frame's steps before its commands read to their ends: which run in every frame, what each changes; each made to wait only when it does something, or shown to touch nothing the job uses. Measured at "Faster time".

### D5 Crossings put off

Those D0 finds early and frequent in the day, queued to the next update, each only where it returns nothing the day reads on; for each, what the player sees later than before (a popup a frame later), said in its research.

### D6 The setting

The setting and its words in the player language, the default chosen by the owner from what D3 to D5 measured, README.md and README.zh-CN.md.

## Order

D0 first, with D1 and D2 beside it: they decide whether the rest is worth building and what it must wait for. D3 only if D0 bounds the gain at the game's pace above the drawing's other ways ([game-speed.md](game-speed.md), "Which part comes next"); D4 and D5 after D3, each by what it measures; D6 last. One pull request for the parts built, as the other plans.
