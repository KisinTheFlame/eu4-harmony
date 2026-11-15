# The application, its frame and the idlers


**The application.**

- `CEU4Application` (`eu4application.cpp`) is 0x6a8 bytes, with constructor `0x1fe8f0` and vtable `0x1c48bf8`. It derives from the engine's `CApplication` (`application.cpp`; constructor `0x14bfd80`, vtable `0x1d8c248`).
- The global `[0x242bf50]` holds it, and every idler keeps it at `+0x330`.
- Its vtable: Init `+0x40` (`0x14c02b0`); before and after the loop `+0x48` and `+0x50`; SDL's application events `+0x58`–`+0x70`.

| Field | What |
| --- | --- |
| `+0x40` | The current idler; at first an engine idler that does nothing (vtable `0x1d924a8`) |
| `+0x48`, `+0x70`, `+0x78`, `+0x80` | The idler switch: pending, the next idler, the parked old one, keep the old one ([notice-window.md](../interface/window-construction.md)) |
| `+0x83`, `+0x84`, `+0x88` | Quit, restart, the restart's extra arguments ([main-menu.md](../interface/main-menu.md)) |
| `+0x2f8` | Quit asked by the event pump |
| `+0x308`–`+0x320` | The frame's start time, the time after the idler's frame, the frame's length in seconds, the frame count |
| `+0x348` | The context owner: audio `+0`, POPS `+0x60`, the gamepad `+0x188` (partly inferred) |
| `+0x350` | The graphics object (also `[0x23494b8]`) |
| `+0x360` | The graphical map |
| `+0x370` | The session |
| `+0x648` | The random seed: `-seed`, or the low half of `QueryPerformanceCounter` |

**The run** `0x14c2d00`:

1. `UpdateWindow`.
2. Pins the main thread to one CPU (`GetProcessAffinityMask`, `SetThreadAffinityMask` at `0x14c2db4`): of the process's processors, the one numbered `count / 2 + 1` from the lowest, the same in every game on a computer ([headless.md](../program/headless.md), "One game per machine"; the framework leaves it out).
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
| Main menu | `0x1d5a8c8` | `0x10d23c0` (0xb48) | `0x10d2e80` | `frontend.cpp` ([main-menu.md](../interface/main-menu.md)) |
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
