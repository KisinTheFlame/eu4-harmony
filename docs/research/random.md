# The game's random numbers, and drawing them on several threads

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Read from the game's code on 2026-11-08 (a Capstone script over the image, the graph of its direct calls), for [ADR random-draws-may-change-hands](../adr/random-draws-may-change-hands.md): what the game draws from, how its own parallel stages draw, and how work the framework runs on several threads could draw safely.

In short: the game's day draws from **one generator**, a Mersenne twister at `0x2325220`, through `0x14c7420` from 203 functions and inlined in 36 more, every draw ending in **`0x14c71b0`**, its step, with no lock. The game's own parallel stages do not draw from it: the main thread first fills **a table of 1000 numbers** (`0x14c74c0`), and each task takes the number at its object's ID plus an offset, modulo 1000, so a task's numbers do not depend on the threads. Work the framework runs on several threads can draw the same way in kind with **a stream for each task**: one number drawn on the main thread as the work begins, each object's stream seeded from it and the object's ID, and `0x14c71b0` answering from the stream of the task the thread is running while the global generator is asked for. Each object then draws as many numbers as it needs, independent of the threads' timing, and a game started with `-seed` still repeats.

## The generators

| Generator | Where | Drawn by | On several threads |
| --- | --- | --- | --- |
| **The global one** | `0x2325220`: 624 words and its place at `+0x9c0` | `0x14c7420`(location, line), 203 direct callers; inlined (`lea rcx, [0x2325220]`, `call 0x14c71b0`, `inc [0x23494b0]`) at 106 places in 36 functions; `0x14c74c0` for tables | No: the day's main-thread work, and the tables its parallel stages read |
| The world's | `world+0xb90` ([late-game.md](late-game.md), "What an army's think writes") | the AI's agents without `-seed` | Yes, from every AI country's task at once, unguarded: the threads' timing decides who gets which number, and two at once can tear its state |
| An agent's own | `agent+0x78`, seeded when the agent is made (`0xe4050`) | the same agents, with `-seed` (`[0x23581c0]`) | With `-seed` the AI stage runs on the main thread ([ai.md](ai.md), "A day of the AI") |

**The step** `0x14c71b0(generator)` refills the 624 words when its place reaches 624 and returns the next tempered 32-bit number; 176 functions call it directly, with one generator or another. A caller makes its own range from the number.

**`0x14c7420`** checks the byte at `[0x1fd10c4]`, set where drawing is forbidden (a frame inside a day; the AI stage), and there logs "Calling random inside forbidden area! Location: "%s":%i" and returns 0; otherwise it steps the global generator, adds one to the count at `[0x23494b0]`, and, while `[0x233ff33]` is set, logs the draw with its location. The inlined draws do neither check nor log.

**The count** `[0x23494b0]` is read by 48 functions, among them the day's checksum record that multiplayer compares (`0x743aa0`), the world's save (`0x74ff30`) and the game's start (`0x8757e0`). A load sets the generator from the save's seed and count ([late-game.md](late-game.md), "Whether a game repeats its days"); with `-seed` the first load of a process sets the seed to the switch's and the count to 0.

## How the game's own parallel stages draw

`0x14c74c0`(location, line, table) fills a table of 1000 numbers from the global generator and adds 1000 to the count; three phases call it on the main thread before their stages: the day's body (`0x75bbe0`), the provinces' passes (`0x75b2d0`) and the monthly update (`0x75d690`).

In the provinces' passes (`0x75b2d0`): the table is filled on the stack (`0x75b321`), and the world and the table are given to two parallel stages over the provinces (`0x77fe10`, `0x7802c0`). A task (vtable `0x1cafb10`, its run `0x788cf0`) calls `0x9f4b40`(province, table) and `0x9f5160`(province, table), which take the number at `(the province's ID + 0x167e + an offset of their own) mod 1000` (`0x9f51b7`..`0x9f51e8`; `0x9f4d0a`, `0x9f4e9f`).

So the game's answer to drawing on several threads: draw on the main thread, in the game's order, and let each task read what was drawn by a key of its object. Each task's numbers are the same whatever the threads do, and the run repeats. What it gives up: a task can take only what the table holds (a number by its key and the offsets it uses), and objects whose keys are 1000 apart share numbers.

## A stream for each task

For work the framework runs on several threads (a loop over the countries or the provinces):

1. **As the work begins**, on the main thread: one number drawn from the global generator the game's way (`0x14c7420` with the framework's location), so that the global generator and its count move as in any game, by one.
2. **Each object** (by its ID: a country's tag index, a province's ID) gets a stream seeded from that number and its ID through a mixing function (SplitMix64), feeding a small generator of 32-bit numbers (PCG32 or xoshiro128**) kept by the task, not shared.
3. **While a task runs**, a thread-local pointer names its stream. `0x14c71b0`, replaced, answers from that stream when it is asked for the global generator (`rcx` = `0x2325220`) on a thread running a task, and is the game's step otherwise: the world's generator, the agents', and the global one on the main thread outside the work are untouched. Every draw of the global generator ends there, through `0x14c7420` or inlined, so one point covers them all.
4. **The count**: callers add one to `[0x23494b0]` themselves, with a plain `inc`, which on several threads at once loses some, so the count ends a little low. Only multiplayer's checksum and a load's setting of the generator read it, and the global generator itself did not step for those draws: a load of a save written after such work sets the generator from a count that differs from the steps it took, which gives other numbers after the load, a difference the ADR allows. Whether to keep the count exact (the callers' `inc` patched to add atomically, or the stream's draws left out of it) is decided with the first such work.
5. **Forbidden draws**: `0x14c7420`'s check stays as it is; the work runs in the day, where drawing is allowed, as the game's own loops do.

What it gives: each object draws as many numbers as it needs, independent of the other objects and of the threads; a run started with `-seed` repeats, since the one number drawn and the objects' IDs are the same; no lock is taken. The numbers differ from the game's own order of draws, which the ADR allows. A 32-bit generator of good quality, seeded per object, gives numbers of the same distribution as the twister's to callers that take a range of them.

**The world's generator** could be given the same treatment for the AI stage: a stream for each AI country's task, so that its agents' draws no longer race on one generator. That is a repair of the game's own race as much as a speed-up (two threads stepping the twister at once can hand them one number, or tear its 624 words), and its effect on the stage's time was not measured.

## Not verified

- That nothing but the 36 functions and `0x14c7420` draws from the global generator: calls through pointers to `0x14c71b0` are not in the graph.
- How a load sets the generator from the seed and the count (stepping it the count's times, or another way), and where the save keeps the seed.
- What reads `[0x233ff33]`, the draws' log switch.
- The cost of a replaced `0x14c71b0` on the game's 176 callers when no task runs.
