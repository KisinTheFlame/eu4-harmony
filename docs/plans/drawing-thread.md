# Drawing thread plan

Status: proposed (2026-11-09), not decided; nothing built. It answers the owner's question of 2026-11-09, how the game's work and its drawing could be put on two threads, from what is known of both ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md)). Rewritten the same day after the owner's review: the goal is a map that answers at the display's own rate whatever the day does, not only a shorter day; the drawing thread keeps its own pace, at the display's refresh rate unless the player chooses no limit, whatever the game's vertical sync is (the owner, 2026-11-09). Its first parts are measurements and proofs that decide whether the later ones are built. Terms it adds to [GLOSSARY.md](../../GLOSSARY.md): Drawing thread, Crossing, Presentation lock. The bar it meets is [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md): its differences are of kinds A (the drawing, outside the game state) and B (the pace of frames) of [game-speed.md](game-speed.md), "What 'the same game' means"; nothing the day computes changes.

## Goal and scope

- **Goal**: the map answers dragging, zooming and scrolling at the display's rate, 144 frames a second on the machine of [performance.md](../performance.md), at every speed and however long a day takes. Late in a game at speed 5 the screen now gets about 20 frames a second: a frame between days, and one inside a day every 50 ms at the game's own pace ([game/day.md](../game/day.md), "Frames inside a day"); with "Faster time", 1.5 frames a day at 14.5 days a second ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The Direct3D calls on a thread of their own"). On a month's first day the main thread works a quarter of a second at a time ([game/day.md](../game/day.md), "Where the time goes").
- **Besides**: the days go faster by what the drawing costs the main thread, at most 5.5 ms of a late 69 ms day at "Faster time", about 8%; at the game's own pace the frames inside a day stop costing the day their drawing.
- **In three layers**, each standing on the one before:
  1. **The drawing at its own pace** (D3): a thread of the framework's draws the map, with a camera of its own that the mouse and the keys move at once, and the interface's windows as their last update left them, at the display's rate, beside whatever the main thread does.
  2. **The interface updated often inside a day** (D4): the windows, the tooltips and the map's markers updated on the main thread at the points where the game itself already lets a frame in during a day, between phases and every 32 countries, without drawing there.
  3. **The interface updated beside the day** (D7, research only): the interface's views reading the game state while the day changes it, made safe; the problem the game's code poses most sharply ("Set aside, and why").
- **Set aside, and why**:
  - **A copy of the world for the interface**: the game state is objects linked by pointers, which a copy would have to link again, every day; the interface's code reads the live objects through the game's own globals, so it would have to be pointed at the copy everywhere ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "A simulation thread and a drawing thread").
  - **The day moved to a thread of its own**, the main thread keeping the frames: the frames inside a day are called from the day's own code, the window and SDL's events belong to the main thread, and the device is reset there. Moving the drawing gives the same and changes less.
  - **Drawing with no limit by default**: frames past the display's refresh rate are not shown in a borderless window, where the desktop composes the screen at the display's rate, and tear in a full screen without vertical sync; a drawing thread always drawing would hold the presentation lock all the time, so every crossing and update of the day would wait for a frame, and would take a core from the worker threads the AI stage needs. It is the player's choice ("Behavior").
- **Out of scope**: what the day computes; the loading screen; multiplayer, where nothing the machines compute changes. The main menu's and the nudge editor's frames take the same mechanism after the game's (D6).

## What the game does that this rests on

- **A frame in a game** runs, in order: the interface's steps before the commands, the commands with the daily tick that runs the day, the outliner, input (with the camera's update), the time step, the alerts, the map's markers, the GUI's update, and **the drawing last**: the map mode's colors when they changed, the device check, the map's passes, the 3D map, the sky, the GUI, the debug text when asked for, Present ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The in-game frame's steps, in order", "The camera in a frame").
- **The game lets its whole frame in during a day** at 34 points by direct calls, between the day's phases and between two countries' daily updates every 32 countries, once its interval has passed ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the game lets a frame in, inside the day"). There the game state is whole enough for the game's own interface.
- **The camera** moves once a frame, from that frame's events, on the main thread, gliding towards its goal and kept on the map ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The camera in a frame"; [research/camera.md](../research/camera.md)).
- **The day reaches the interface and the graphics** in at least 48 places by direct calls, besides the frames inside a day and the interface's daily and monthly refreshes: popups and messages, a combat result drawn into a texture, dialogs to the player, the map mode set again with its colors and names, the map's markers when a province changes owner, a port's model, the minimap, a unit's model, a reform's animation, the new age's window ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the day reaches the interface and the graphics"). These are the **crossings**.
- **The drawing reads little of the game state**: the map mode's colors and the debug text aside, the map's names when they are made again, the map's passes and views, and three fields of every province each frame; it draws no random numbers, and the GUI's windows draw from the GUI's own objects as far as direct calls show ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "What the drawing reaches of the game state").
- **The drawing's cost** on the main thread late in a game was about 3.6 ms a frame (5.5 ms a day at 1.53 frames a day), of it 2.0 ms a day waiting in the driver ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The Direct3D calls on a thread of their own"): some 270 frames a second before the graphics card's own limit, which was not measured.
- **The device** is made for several threads, and the game already draws its loading screen from another thread ([game/interface.md](../game/interface.md), "The frame").

## Behavior

- **The map** moves, zooms and scrolls at the drawing thread's pace, whatever the game's speed, and goes on doing so through a month's first day.
- **The windows, the tooltips, hovering and the map's markers** change at the interface's pace: between days as now, and inside a day at the game's own points, as often as the interface's interval allows (D4 measures the interval's cost and chooses its default).
- **What the map shows of the game** is what the last update left: a province's color or a unit's place changes when the day's update has been made, not between.
- **A setting**, in the settings window's "Game speed" group: "Frame rate" (帧率) with a choice bar, "The display's refresh rate" (显示器刷新率), the default, and "No limit" (不限), followed by an info mark: no limit gives a little less delay between the mouse and the map, and costs the game some speed and the graphics card more work. The game's own vertical sync setting no longer slows the game, since Present waits on the drawing thread; it still decides whether frames tear.
- **"At the fastest speed"** (S4 of [game-speed.md](game-speed.md)) loses its reason once the frames inside a day no longer draw: what is left of it is how often the interface is updated inside a day, which D4 decides whether to keep as a choice.
- **The drawing thread is a choice too until D3 to D5 have shown it**: "Drawing" with "On the main thread (as in the game)" and "On a thread of its own"; whether it stays a setting is the owner's, from what they measured.
- **The debug text** (the console's debug country) works a country's values out as it is drawn, so while it is on the frame is drawn on the main thread, as in the game.

## How it fits together

### The two threads

- **The main thread** keeps the window and SDL's events, the platform services, the input the game takes (clicks, keys, text), the commands and the days, the interface's update, the map mode's colors, the device check and reset, extensions and remote control.
- **The drawing thread**, one, made by the framework at startup, keeps the camera and draws: the map's passes, the 3D map, the sky, the windows, the framework UI, Present. It draws a frame, waits until the next is due (at the display's rate, from a high-resolution waitable timer, or not at all with no limit), and draws again.

```text
main:     update ─ day: phase … (crossing: lock) … phase ─ update at a point (lock) ─ … ─ month (255 ms) ─ update ─
drawing:  frame frame frame frame frame frame (waits for a crossing) frame frame frame frame frame frame frame frame
```

### The presentation lock

One lock between the two threads. The drawing thread holds it for each frame it draws; the main thread for each interface update and each crossing. So the interface's own objects (windows, sprite types, textures, the fonts' caches, the map's scene and its arrays) are never used by both at once and need no lock of their own, and the main thread waits at most one frame for it, about 3.6 ms late in a game, less often the more time the drawing thread spends waiting for its next frame.

- **Every interface update** takes it: between days, at the game's points inside a day, an idler switch, a device reset, a session left, a save loaded.
- **Every crossing** takes it, caught twice over: at its entry, an observer on each place of the research note's table; and at the bottom, whatever path led there, the main thread's calls into the back end's table and into the device's own methods (D3DX calls those), and the GUI manager's making and killing of windows, which catch the crossings the direct call graph missed. A crossing that comes so often that the lock's waits cost the day much may instead be put off to the next update (D5), where it returns nothing the day reads on.
- **The drawing's reads of the game state** happen beside the day, every frame. The three fields of every province are scalars in the province array, which does not move during a game; one read as the day writes it shows a frame's wrong pixel at worst, and none of the three was written on the days watched. The map's names, when they are to be made again, are made at the update, under the lock. What D2 finds the passes and views read is moved to the update, copied there, or shown to be such a scalar.
- **The drawing writes nothing the day reads, and draws no random numbers**: none found by direct calls; D2 shows it by the write watch, with the debug text off.

### The camera and its input

- **The camera is the drawing thread's.** Each frame it is moved from the mouse and the keys and glides towards its goal, as the game's update of it does now; the game's own update of the camera in the main thread's input is left out.
- **The mouse and the camera's keys reach the drawing thread** through Raw Input, sent to a hidden window of the drawing thread's own, so they arrive while the main thread is in a day: movement and buttons for dragging, the wheel for zooming, the cursor's place at the screen's edge for scrolling, the camera's keys. The main thread still takes every event as now for all else (clicks on windows and the map, keys, text), and the camera's share of them is no longer handed to the game's camera.
- **What the main thread asks of the camera** (the game's own moves, looking at a province, the camera capability of remote control) becomes a goal handed to the drawing thread at the next frame; **what it reads of it** (picking a province under the cursor, the map's markers, the ambient sound, views placed on the map) reads a copy taken at each update.

### Where the interface update runs

- **Between days**, as now, in the in-game frame, without its drawing.
- **Inside a day**, at the game's points for a frame inside a day (`0x769ea0`), once the interface's interval has passed: the frame there runs without its drawing, which the drawing thread does; how long the update alone takes there, and so how often it can run, is D0's and D4's.
- **During the AI stage**, where the main thread waits for the workers, an update could run too; that is a frame at a point the game was not written for ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "A frame drawn while the AI stage is waited for"), and is left for after D4.

### The device and the window

- One device, used by the drawing thread for its frames and by the main thread under the lock; the device check, a lost device and its reset stay on the main thread under the lock.
- Present is called on the drawing thread, with the game's interval for vertical sync; a failed Present is told to the main thread.
- The window, its messages and SDL's events stay the main thread's; the drawing thread's hidden window only receives Raw Input.

### The framework's own parts

- **The Present observer** runs on the drawing thread: the framework UI's state, set on the main thread by extensions, remote control and the settings window's input, is handed over under the lock; the framework UI's own input stays where it is now, through the main thread's events. A screenshot is taken in the drawing thread's next frame and answered on the main thread.
- **The Unicode module's font pages**, written as text is drawn and measured, are written by either thread only under the lock.
- **The sampler** (`cargo xtask profile`) names the drawing thread and sorts its samples by the drawing's parts; it reports the frames drawn a second and the lock's waits.
- **Extensions and remote control** run at the in-game frame on the main thread, as now.
- **In the `speed` module** it is a speed-up of the group `frames`, which a developer build can leave to the game; its dependency points are the drawing's call and its split, the camera's update and input in the game's input, each crossing's entry and the catch-alls, held to the framework's whole startup like every other ([ADR framework-succeeds-or-fails-as-a-whole](../adr/2026-10-27-framework-succeeds-or-fails-as-a-whole.md)).

## Parts

### Every part is done when

As in [game-speed.md](game-speed.md), "Every part is done when": the checks pass, the gain is measured on its own target before and after, the run is a file of its own in [prototype-log/](../prototype-log/), what the research found is in [research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md) and the pages of [game/](../game/) it changes, and what players see is in README.md and README.zh-CN.md.

### D0 The measures

Without building anything, in a test game from the saves of [research/late-game.md](../research/late-game.md) ("Saves to test with"), with the call trace's `times` ([dev-tools.md](../dev-tools.md), "Call tracing") and the sampler:

- how long the interface's update takes alone, between days and at the game's points inside a day, which bounds how often D4 can update the interface and what that costs the day;
- how often the crossings come in a late day, and how far apart, which bounds the lock's waits;
- the drawing's cost a frame on the main thread, and the graphics card's ceiling: frames a second paused, with vertical sync off and POPS not sleeping.

### D1 Every crossing found

A development tool that records, while a day runs, each time the main thread goes into the interface's code, the back end's table, the device or the GUI manager, with its call stack, counted by stack, over decades of an AI-only late game and a player's game with windows open. Every stack is a crossing caught, or becomes one. It is a lasting development capability, so its shape is talked through with the owner before it is built ([game-speed.md](game-speed.md), "How a part is chosen"). D3 keeps it on as a check in long runs.

### D2 What the drawing reads

The write watch over the drawing, with the back end's table answered by stubs for the watched call, so that the GUI's drawing does not wait in the driver, which the watch held on 2026-11-08; its reads of the game state against the day's writes, in several windows of the interface; and its writes against what the day reads. Out: the list of what the drawing reads of the game state, each moved to the update, copied there, or shown harmless; that it writes nothing the day reads; and whether the units' models are moved by the day or read from the units by the drawing.

### D3 The drawing at its own pace

Behind a developer build's switch: the drawing split at its device check, the drawing thread with its pacing and the "Frame rate" choice, the presentation lock taken by the updates and by every crossing with both catch-alls, the map's names made at the update, the camera and its Raw Input on the drawing thread, the Present observer there. Measured: frames a second on the map while a late game runs at speed 5 and through a month's first day; the delay from a mouse move to the frame that shows it; days a second with it on and off; the lock's waits; screenshots of a paused game the same, pixel for pixel; an AI-only run of decades with D1's tool on, which finds no crossing that did not take the lock.

### D4 The interface updated inside a day

The frames at the game's points inside a day run as updates without drawing, at an interval D0's measures choose, with what that costs the day in days a second; and whether "At the fastest speed" stays a choice of the interval.

### D5 Crossings put off

Those whose lock waits D3 finds to cost the day most, queued to the next update, each only where it returns nothing the day reads on; for each, what the player sees later than before (a popup an update later), said in its research.

### D6 The setting and the menus

The settings and their words in the player language, their defaults chosen by the owner from what D3 to D5 measured; the main menu's frame (`0x10d2e80`) and the nudge editor's on the drawing thread too, which share the camera's update with the game's; README.md and README.zh-CN.md.

### D7 The interface beside the day (research)

Whether the interface's update can run beside the day instead of at its points, so that the windows and tooltips change at the drawing's pace too. What the research must answer before any of it is built:

- **Memory the day lets go while the interface reads it**: the heap's frees during a day held back until no update is reading (as the developer build's quarantine holds the repairs' frees), so that a vector the day grows or an object it deletes stays readable, as it stood.
- **What the interface writes of the game state** as it updates, such as a country's cached data worked out when asked for: found with the write watch over the update, each such write made the day's alone or taken under a lock, since two threads working the same cache out at once could change what the day computes.
- **Containers that a reader cannot walk while they are changed**, such as trees being rebalanced and tables being grown, on the paths the interface reads: found and counted, each a reason this layer may not be possible as the game's code stands.

What it delivers is a research note and the owner's decision whether to plan it.

## Order

D0 first, with D1 and D2 beside it: they decide what the lock will cost and what the drawing must not read. Then D3, then D4 and D5 by what D3 measures, D6 last; one pull request for the parts built, as the other plans. D7 is research that may run beside D3 to D6 and ends in a decision of its own.
