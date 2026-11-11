# The simulation and the interface take turns; only the drawing runs apart

The end state of the drawing thread's work ([drawing-thread.md](../plans/drawing-thread.md), "The end state") is one process: the simulation and the interface take turns on the main thread, the interface reading the world live between days, and the drawing alone runs on a thread of its own, reading only the layout the interface hands on and placing what follows the camera itself. The world is not double buffered, and phase 3, the world in a second process, is not built (the owner, 2026-11-10). The cost kept is the simulation waiting for the interface's update, about 4.7 ms of a 50 to 70 ms day late in a game.

## Considered Options

- **The world in a second process at the same addresses** (E1): Windows gives what it needs cheaply, but the game does not. One heap holds the world, the databases and the interface side by side with nothing to tell them apart as they are made; only the code that makes the world makes the in-game interface; and some 45,000 pointers lead from the world into the interface's and the graphics' objects, a fifth of their kinds changing within 30 days ([world-in-a-second-process.md](../research/world-in-a-second-process.md), [world-interface-seams.md](../research/world-interface-seams.md)).
- **The simulation and the interface on two threads of one process**: one process has one world, which the interface reads widely while the simulation makes and deletes its objects. A lock puts them back in turn; a copy for the interface would be at other addresses, where the world's pointers are wrong; reading it unlocked races the simulation.

## Consequences

- The interface's update may later overlap stretches of the day shown to write nothing it reads, such as the AI stage's wait for its workers ([drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "A frame drawn while the AI stage is waited for"); at most it saves the update's 4.7 ms.
- The crossings stay the main thread's, under the presentation lock, until they are made events (C2); they no longer have to hold the drawing back once it reads nothing live (T3b).
