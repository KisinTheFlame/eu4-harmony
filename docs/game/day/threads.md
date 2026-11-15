# Threads


No thread name reaches Windows (no `SetThreadDescription`, no `0x406D1388` exception). The names below are the game's own, kept in a registry at `0x23268b0` that the crash reporter reads.

| Thread | Made at | What it does |
| --- | --- | --- |
| Main | the process | Everything in the frame: input, commands, the game state, the interface, drawing; above normal priority, pinned to one CPU (not with the framework) |
| LoadingThread | `0x878043`, entry `0x874010` | The startup load; it registers itself as "Main Thread" too |
| "Short Task" ×max(cores − 1, 1) | `0x1712f50`, entry `0x1712d40` | The engine's parallel work: the daily update's parallel stages (`0x1798a0`), database loading |
| "Long Task" ×max(cores − 1, 1) | the same | Longer jobs at below normal priority: parts of InitGame, out-of-sync saves |
| AudioUpdateThread | `0x16a6a8a` | Refills the sounds' buffers |
| SDL's audio device thread | inside SDL | Mixes (the callback is `0x16a6c20`) |
| SaveSessionUpdateThread, RNWSessionUpdateThread | `0x776b90` and nearby; `0xd84490` | Keep the session's network polled every 50 ms while a save is written or read, or a random new world is made (inferred: so multiplayer peers do not time out) |
| TBB's workers | `tbb.dll` | `tbb::parallel_for` in about ten places (history loading `0x7ea2c0`, map and graphics code); the engine's work sets are used instead unless two switches allow it (`[0x2357b34]`, `[0x1fd10f2]`; inferred) |
| libcurl's resolver | `_beginthreadex` | One short thread per name lookup |

Which thread draws the loading screen while a save loads from the main menu ([safe-points.md](safe-points.md)) was not found.
