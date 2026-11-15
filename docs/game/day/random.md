# The game's random numbers, and drawing them on several threads

## The generators

| Generator | Where | Drawn by | On several threads |
| --- | --- | --- | --- |
| **The global one** | `0x2325220`: 624 words and its place at `+0x9c0` | `0x14c7420`(location, line), 203 direct callers; inlined (`lea rcx, [0x2325220]`, `call 0x14c71b0`, `inc [0x23494b0]`) at 106 places in 36 functions; `0x14c74c0` for tables | No: the day's main-thread work, and the tables its parallel stages read |
| The world's | `world+0xb90` ([late-game.md](../ai/work-scheduling.md), "What an army's think writes") | the AI's agents without `-seed` | Yes, from every AI country's task at once, unguarded: the threads' timing decides who gets which number, and two at once can tear its state |
| An agent's own | `agent+0x78`, seeded when the agent is made (`0xe4050`) | the same agents, with `-seed` (`[0x23581c0]`) | With `-seed` the AI stage runs on the main thread ([ai.md](../ai/ai.md), "A day of the AI") |

**The step** `0x14c71b0(generator)` refills the 624 words when its place reaches 624 and returns the next tempered number, **below 2^31**: its tempering ends in an arithmetic shift (`sar eax, 0x12; xor eax, r8d`, `0x14c72c2`), which leaves the top bit 0. 176 functions call it directly, with one generator or another. A caller makes its own range from the number, and takes it as a non-negative `i32`: an AI agent's `x % 2 + 1` as a divisor (`0x13499a`..`0x1349f2`) is 0 for a negative odd `x`, so a generator that gave the top bit too would make it divide by zero (it did: [the AI countries' random streams](../../research/performance-evidence.md)).

**`0x14c7420`** checks the byte at `[0x1fd10c4]`, set where drawing is forbidden (a frame inside a day; the AI stage), and there logs "Calling random inside forbidden area! Location: "%s":%i" and returns 0; otherwise it steps the global generator, adds one to the count at `[0x23494b0]`, and, while `[0x233ff33]` is set, logs the draw with its location. The inlined draws do neither check nor log.

**The count** `[0x23494b0]` is read by 48 functions, among them the day's checksum record that multiplayer compares (`0x743aa0`), the world's save (`0x74ff30`) and the game's start (`0x8757e0`). A load sets the generator from the save's seed and count ([late-game.md](../ai/work-scheduling.md), "Whether a game repeats its days"); with `-seed` the first load of a process sets the seed to the switch's and the count to 0.

## How the game's own parallel stages draw

`0x14c74c0`(location, line, table) fills a table of 1000 numbers from the global generator and adds 1000 to the count; three phases call it on the main thread before their stages: the day's body (`0x75bbe0`), the provinces' passes (`0x75b2d0`) and the monthly update (`0x75d690`).

In the provinces' passes (`0x75b2d0`): the table is filled on the stack (`0x75b321`), and the world and the table are given to two parallel stages over the provinces (`0x77fe10`, `0x7802c0`). A task (vtable `0x1cafb10`, its run `0x788cf0`) calls `0x9f4b40`(province, table) and `0x9f5160`(province, table), which take the number at `(the province's ID + 0x167e + an offset of their own) mod 1000` (`0x9f51b7`..`0x9f51e8`; `0x9f4d0a`, `0x9f4e9f`).

So the game's answer to drawing on several threads: draw on the main thread, in the game's order, and let each task read what was drawn by a key of its object. Each task's numbers are the same whatever the threads do, and the run repeats. What it gives up: a task can take only what the table holds (a number by its key and the offsets it uses), and objects whose keys are 1000 apart share numbers.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0xb1028` | code | AI country's task reads its tag |


## Limits of the reading

Indirect-pointer callers are outside the direct-call graph. Exact load reconstruction from seed/count and readers of draw-log switch `[0x233ff33]` remain unknown. Seeded runs do not establish general determinism; see [the observed limits](../../research/performance-evidence.md#randomness-and-ai-order).
