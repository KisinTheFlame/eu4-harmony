# Drawing thread plan

Status: decided (2026-11-09): phases 0, 1 and 2 are to be built, in one effort, and then looked at as a whole by the owner before phase 3. Built and tried on 2026-11-09 ([the first runs](../prototype-log/2026-11-09-drawing-thread-first-runs.md), [the crossings deferred](../prototype-log/2026-11-09-drawing-thread-crossings-deferred.md)): phase 0 measured (M1 to M4); C1, the crossings' tool, built and run over 1,050 days; T1 and T2, the drawing thread under the lock with the camera on it, built, and made a fixed part with no setting and no fallback, "Faster time" taken out (the owner, 2026-11-09); C2 for what a late day crosses most: the units' crossings caught where a model is let go, the floating texts and the interface's daily refresh done at the next frame, the rest under the lock; R1 for the units' models and for what the map's drawing leaves for the next update. At speed 5 the drawing thread draws 133 to 138 frames a second where the main thread drew 20 to 40, 144 paused, and the days go as fast as with the drawing on the main thread, within what the computer's other work moves them by. T3, the layout double buffered, is not built: what it was to take away, the interface's update and the drawing taking turns, now costs the main thread 4 to 6% of its time at speed 5, and the drawing already keeps the display's rate; whether it is still to be built is the owner's to decide. Not seen yet: the map following the mouse in a game the player sees. It answers the owner's question of 2026-11-09, how the game's work and its drawing could be put apart, from what is known of both ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md)). Its end is the simulation, the interface and the drawing each on a thread of its own, none waiting for another; the simulation itself on several threads is not part of it (the owner, 2026-11-09). The drawing keeps its own pace, at the display's refresh rate unless the player chooses no limit, whatever the game's vertical sync is; the interface reads a world held still for it, by double buffering (the owner, 2026-11-09). Terms it adds to [GLOSSARY.md](../../GLOSSARY.md): Drawing thread, Crossing, Presentation lock. The bar it meets is [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md): its differences are of kinds A (the drawing, outside the game state) and B (the pace of frames) of [game-speed.md](game-speed.md), "What 'the same game' means"; nothing the day computes changes. On 2026-11-10 the end state was designed and decided: one process, the drawing alone apart ("The end state"; [ADR simulation-and-interface-take-turns](../adr/2026-11-10-simulation-and-interface-take-turns.md)), phase 3 set aside, and T3 split into T3a and T3b; T3a built the same day: the map icons follow the drawing thread's camera ([the run](../prototype-log/2026-11-10-map-icons-follow-the-camera.md)).

## Goal and scope

- **Goal**: the map answers dragging, zooming and scrolling at the display's rate, 144 frames a second on the machine of [performance.md](../performance.md), at every speed and however long a day takes; later the windows and tooltips too. Late in a game at speed 5 the screen now gets about 20 frames a second: a frame between days, and one inside a day every 50 ms at the game's own pace ([game/day.md](../game/day.md), "Frames inside a day"); with "Faster time", 1.5 frames a day at 14.5 days a second. On a month's first day the main thread works a quarter of a second at a time ([game/day.md](../game/day.md), "Where the time goes").
- **Besides**: the days go faster by what the interface and the drawing cost the main thread, late in a game about 10 ms of a 69 ms day.
- **The end, in three roles**:

  | Role | Reads | Makes |
  | --- | --- | --- |
  | The simulation | the live world | the world |
  | The interface | the world held still for it | the layout: what each window's elements show and where, each province's color, the map's markers |
  | The drawing | the layout, the camera, the models and textures | the pixels |

  The drawing reads no world. Between the interface and the drawing the layout is double buffered; between the simulation and the interface the world is, which needs the interface in a process of its own, since the world's pointers must be valid at the same addresses (phase 3).
- **Phases**: 0, measures; 1, the drawing thread; 2, the crossings made into events, so that the simulation no longer touches the interface; 3, the world double buffered between two processes, researched before it is decided. Phases 0 to 2 are this effort.
- **Set aside, and why**:
  - **A copy of the world in the same process**: the world's objects point at each other by address, and the game's code reaches the world through fixed globals the same for every thread; a copy at other addresses would need every pointer in it found and moved, with no types to tell which eight bytes are pointers, and the interface's code changed wherever it reads those globals ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "A simulation thread and a drawing thread").
  - **Drawing with no limit by default**: frames past the display's refresh rate are not shown in a borderless window and tear in a full screen without vertical sync, and a drawing thread always drawing would make every update and crossing wait and take a core from the worker threads. It is the player's choice ("Behavior").
  - **The simulation on several threads**: [game-speed.md](game-speed.md)'s, not this plan's.
- **Out of scope**: what the day computes; the loading screen; multiplayer, where nothing the machines compute changes.

## The end state (designed 2026-11-10)

The owner asked, after seeing the drawing thread in a game, for the end state to be designed first and built toward directly. What showing the game taught: whatever the drawing reads live of the interface lags or races it. The map's icons, placed by the interface with the camera of its update, stayed behind the map the drawing thread moved; the camera's cursor read used the calling thread's window; the crossings had to hold whole frames back. So the end state is designed around what each role reads.

### What each role owns

| Role | Where it runs | Owns | Reads | Hands on |
| --- | --- | --- | --- | --- |
| The simulation | the main thread, or a process of its own (below) | the world | the world, the commands | at each day's end, the world held still; events for what it did to the interface |
| The interface | the main thread, at the update points | the GUI's objects, the map's presentation data (the map mode's colors, the per-province arrays, the markers, the models' places), the map icons | the world held still, the camera's copy, the input | at each update, the layout: the windows' drawing recorded, the map's data, each map icon's window and point on the map |
| The drawing | the drawing thread | the camera, the frame and the device's use in it, what follows the camera | the last layout handed on, the camera, the models and textures | the pixels; at each update, a copy of the camera |

### Its rules

1. **The drawing reads only what was handed on and its own state.** It never reads a GUI object or the world live, so no crossing can hold it back, and the presentation lock covers only the handover of the layout.
2. **What follows the camera is placed by the drawing, at its pace**: the 3D map, the models, and the map icons, whose places the interface works out with the camera of its update (`0x132e300`, [research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the map icons are placed") and the drawing works out again with its own.
3. **The simulation never calls into the interface.** What it did there, the crossings, is an event queued and done at the next update.
4. **The interface updates at the update points**: between days and at the game's points inside a day.
5. **The camera's input** (dragging, the wheel, the edge of the screen, the camera's keys) **is the drawing's**; every other input is the main thread's.

### Where the simulation runs: decided, one process

- **E1, the world in a second process** (phase 3): the simulation in a process of its own, the interface and the drawing in the other, the world's pages changed in a day copied into it at the same addresses at each day's end. The simulation then never waits for the interface.
- **E2, one process**: the simulation and the interface take turns on the main thread, as now; the interface reads the world live between days, and the simulation waits for its update, about 4.7 ms of a 50 to 70 ms day late in a game (M2).

The rules hold in both; only how the world reaches the interface differs. W1 to W3, read and measured on 2026-11-10, found E1 blocked by the game: one heap holds the world and the interface side by side with nothing to tell them apart, only the code that makes the world makes the in-game interface, and some 45,000 pointers lead from the world into the interface's objects ([world-interface-seams.md](../research/world-interface-seams.md), [world-in-a-second-process.md](../research/world-in-a-second-process.md)). **E2 is the end state** ([ADR simulation-and-interface-take-turns](../adr/2026-11-10-simulation-and-interface-take-turns.md)); the interface's update may later overlap stretches of the day shown to write nothing it reads.

### What is built, against it

- **Stays**: the drawing thread and its pace, the presentation lock (narrowed in the end to the layout's handover), the camera and its cursor read on the drawing thread, the drawing thread stopped as the game quits, the deferred calls (the first of rule 3's events).
- **Goes once rule 1 holds**: the crossings caught at the bottom to hold the drawing back (the mesh object let go, the renderer's scene begun, a crossing anywhere holding the prepared frame back), and the idler's switch waiting for the drawing's frame; they protect what the drawing still reads live.

## What the game does that this rests on

- **A frame in a game** runs, in order: the interface's steps before the commands, the commands with the daily tick that runs the day, the outliner, input (with the camera's update), the time step, the alerts, the map's markers, the GUI's update, and **the drawing last**: the map mode's colors when they changed, the device check, the map's passes, the 3D map, the sky, the GUI, the debug text when asked for, Present ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The in-game frame's steps, in order", "The camera in a frame").
- **The game lets its whole frame in during a day** at 34 points by direct calls, between the day's phases and between two countries' daily updates every 32 countries, once its interval has passed ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the game lets a frame in, inside the day").
- **The day reaches the interface and the graphics** in at least 48 places by direct calls, besides the frames inside a day and the interface's daily and monthly refreshes: the **crossings** ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the day reaches the interface and the graphics").
- **The drawing reads little of the game state**: the map mode's colors and the debug text aside, the map's names when they are made again, the map's passes and views, and three fields of every province each frame; it draws no random numbers, and the GUI's windows draw from the GUI's own objects as far as direct calls show ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "What the drawing reaches of the game state").
- **The device** is made for several threads, and the game already draws its loading screen from another thread ([game/interface.md](../game/interface.md), "The frame").

## Behavior

- **The map** moves, zooms and scrolls at the drawing thread's pace, whatever the game's speed, and goes on through a month's first day.
- **The windows, the tooltips, hovering and the map's markers** change at the interface's pace: between days as now, and inside a day at the game's own points.
- **What the map shows of the game** is what the last update left.
- **A setting**, in the settings window's "Game speed" group: "Frame rate" (帧率), "The display's refresh rate" (显示器刷新率) by default or "No limit" (不限), with an info mark: no limit gives a little less delay between the mouse and the map, and costs the game some speed and the graphics card more work. The game's own vertical sync no longer slows the game; it still decides whether frames tear.
- **No choice of drawing**: the drawing thread is a fixed part of the framework, not a setting (the owner, 2026-11-09). A frame of it that faults is not left out: the game crashes as it would on its own fault, since the drawing's stability is to rest on the code's being right, not on a way round it (the owner, 2026-11-09). Development builds can leave it to the game (`game-paths = ["drawing-thread"]`) to measure against it.
- **Frames inside a day stay**, as updates of the interface: they draw nothing, as the drawing thread draws, but take the input, update the windows and the map's markers and end what a crossing held back. "Faster time" (S4 of [game-speed.md](game-speed.md)), which left them out at speed 5, is taken out: with the drawing thread the days went as fast with them as without them, 1,043 and 935 against 946 and 944 days a minute ([the run](../prototype-log/2026-11-09-drawing-thread-crossings-deferred.md)). An interim answer (the owner, 2026-11-09): whether the interface's update inside a day stays is to be decided again with phase 3.
- **The debug text** (the console's debug country) works a country's values out as it is drawn, so while it is on the frame is drawn on the main thread.

## How it fits together

### The threads

- **The main thread** keeps the window and SDL's events, the platform services, the input the game takes, the commands and the days, the interface's update, the map mode's colors, extensions and remote control.
- **The drawing thread**, made by the framework once a game has its interface, keeps the camera and draws the frame at its own pace, from a high-resolution waitable timer at the display's rate or without waiting.

### The presentation lock

One lock between the two, fair, so that neither starves. The drawing thread holds it for each frame it draws. The main thread holds it except while the day's simulation runs: it lets it go as the day's body begins and after each frame inside a day, and takes it again at the day's end, at each frame inside a day, and at each crossing, which then holds it until the next of those points. So the interface's own objects are never used by both threads at once and need no lock of their own.

A crossing is caught at its entry, an observer on each place of the research note's table, and at the bottom whatever path led there: the main thread's calls into the back end's table and into the device's own methods (D3DX calls those), and the GUI manager's making and killing of windows. Phase 2 turns the crossings into events, so that the simulation stops taking the lock at all.

### The layout, double buffered

Phase 1 begins with the lock alone (T1): the drawing thread runs the game's own drawing under it. T3 then puts a buffer between the interface and the drawing, so that the interface's update and the drawing no longer take turns: the windows' drawing recorded on the main thread, at the update, into a list of the back end's calls with their data, which the drawing thread plays; the map's drawing, which follows the camera, run on the drawing thread from the map's own data (each province's colors, the map's names, the models' places) copied at the update.

### The camera and its input

- **The camera is the drawing thread's.** It is updated each frame the drawing thread draws, gliding towards its goal as the game's update of it does; the game's own update of it in the main thread's input is left out.
- **The mouse and the camera's keys reach the drawing thread** through Raw Input, sent to a hidden window of its own, so they arrive while the main thread is in a day: dragging, the wheel for zooming, the cursor at the screen's edge, the camera's keys. The main thread takes every event as now for all else.
- **What the main thread asks of the camera** (the game's own moves, looking at a province, remote control's camera) is handed to the drawing thread as a goal; **what it reads of it** (picking, the markers, the ambient sound) reads a copy taken at each update.

### Time

The drawing's animations (the map's fades, the water, the units' models) run on the frame's length, which the application keeps for the main thread's frame; the drawing thread draws with a frame length of its own, set while it holds the lock.

### The device, the window and the framework's own parts

- One device; the device check and a reset under the lock. Present is called on the drawing thread; a failed Present is told to the main thread.
- The window, its messages and SDL's events stay the main thread's.
- The Present observer (the framework UI, screenshots) runs on the drawing thread, under the lock, which the main thread holds wherever it changes the framework UI's state; a screenshot is answered on the main thread.
- The sampler names the drawing thread and reports its frames a second and the lock's waits.
- It is a speed-up of the group `frames` in the `speed` module; its dependency points are held to the framework's whole startup like every other ([ADR framework-succeeds-or-fails-as-a-whole](../adr/2026-10-27-framework-succeeds-or-fails-as-a-whole.md)).

## Parts

### Every part is done when

As in [game-speed.md](game-speed.md), "Every part is done when": the checks pass, the gain is measured on its own target before and after, the run is a file of its own in [prototype-log/](../prototype-log/), what the research found is in [research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md) and the pages of [game/](../game/) it changes, and what players see is in README.md and README.zh-CN.md.

### Phase 0: the measures

In a test game, from the saves of [research/late-game.md](../research/late-game.md) ("Saves to test with"):

- **M1 The pages a day writes**: how many different pages of memory a late day writes, and between two of the game's points for a frame inside a day; it decides whether the world's double buffer (phase 3) can copy what changed at each handover, and how long the simulation would stop for it.
- **M2 The interface's update alone**: how long it takes between days and at the game's points inside a day, which bounds how often the interface can be updated and what it costs the day.
- **M3 The crossings**: how often each comes in a late day and how far apart, which says what the lock costs and which crossings phase 2 takes first.
- **M4 The drawing**: its cost a frame, and the graphics card's ceiling, paused, with vertical sync off.

### Phase 1: the drawing thread

- **R1 What the drawing reads**: of the game state, beyond the direct calls, each read moved to the update, copied there, or shown harmless; that it writes nothing the day reads; whether the units' models are moved by the day or read from the units.
- **T1 The drawing thread under the lock**: the drawing thread and its pacing, the frame rate's setting, the lock with its points and catch-alls, the map's names made at the update, the drawing's own frame length, the Present observer on the drawing thread, the frames inside a day as updates without drawing.
- **T2 The camera and its input** on the drawing thread.
- **T3 The layout double buffered**, in two parts:
  - **T3a The map icons follow the drawing's camera**: the interface's placing of each map icon (`0x132e300`) kept with its window, its point on the map and where it put it; the drawing thread works the point out again with its own camera before it draws, and moves the window by the difference (rule 2).
  - **T3b The windows' drawing recorded** at the update into a list of the back end's calls with their data, which the drawing thread plays, and the map's data copied at the update: then the drawing reads nothing live (rule 1).

### Phase 2: the crossings as events

- **C1 Every crossing found**: a development tool that records, while a day runs, each time the main thread goes into the interface's code, the back end's table, the device or the GUI manager, with its call stack, counted by stack. It is a lasting development capability, so its shape is talked through with the owner before it is built ([game-speed.md](game-speed.md), "How a part is chosen").
- **C2 The crossings as events**: each crossing whose result the day does not read on queued and done at the next update; one whose result it reads (a unit's model) given a stand-in at once and the real thing at the update; until every crossing a day reaches is one of these, the lock catches the rest.

### Phase 3: the world double buffered (researched, not built)

W1 to W3 were read and measured on 2026-11-10 and phase 3 set aside (E2, above).


- **W1** The seams between the world and the layout: pointers from the world's objects to the interface's, globals, and objects of both in one heap.
- **W2** The game's allocations taken over: the world's in memory whose writes Windows watches, the layout's apart.
- **W3** A second process with the world at the same addresses: `eu4.exe` at the same base, its data the same, the game's databases mapped from the simulation's process rather than loaded again.
- **W4** The handover: the pages written since the last copied at a point where the world is whole, two buffers in turn, and how long the simulation stops for it (M1).

## Order

Phase 0 first; R1 beside it. Then T1, T2 and T3 in turn, C1 before C2, each measured as it is built; one pull request for phases 0 to 2, which the owner looks at as a whole before phase 3 is decided.

Toward the end state (2026-11-10), what is left goes in this order: T3a, which a player sees at once; W1 to W3 beside it, read and measured, which decide between E1 and E2; then T3b, after which the crossings no longer hold the drawing back and their catches at the bottom go; then C2 to its end, every crossing an event, which frees the day from the lock. Phase 3 is not built (E2, [ADR simulation-and-interface-take-turns](../adr/2026-11-10-simulation-and-interface-take-turns.md)); W1 to W3 stay as its research, and W4 is not taken up.
