# AI agent scheduling and execution order

## An AI country's agents

Read from the code on 2026-11-07, with what [main-menu.md](../interface/main-menu.md) found of the agents' objects ("The AI's agents").

- **A country's AI works through agents**, in a tree. Its top agent (vtable `0x1c3e9e0`, at the AI's `+0x28`) is what `0xb0fa0` steps through `0x14b370`; it keeps the agents it started on its list at `+0xc50`, the 0xbf8-byte agents (vtable `0x1c3cdb8`) that lead armies, and steps others too, such as the foreign minister (vtable `0x1c3df20`, `ai_foreignminister.cpp`).
- **An agent's day** is `0xe4890`, slot `+0x58` of every agent's vtable: when its slot `+0x50` says so, it counts its `+0x20` down and, at 0, calls its slot `+0x60`, its think, whose result is the number of days to its next think, kept at `+0x20`; then it steps the agents on its own list at `+0x40`. The top agent does the same at `0x14b603`..`0x14b673` and then walks its list at `+0xc50`. So the items of the timed task are agents, and the costly ones those whose think comes that day: on a day few think, on another many, which is why one country's task varies so much from day to day.
- **An army agent's think** is `0xe9f30`, which calls `0xea250`; the foreign minister's is `0x13d4c0`.
- **The top agent's plan.** Before its agents, `0x14b370` reads its byte `+0xce0` under a lock shared by every country (`[0x2357ba0]`); when it is set and its list at `+0xcc0` is empty, it plans with `0x147660`, and otherwise, one day in twelve at random (`0x14b52c`, a draw modulo 12 equal to 1), with `0x146ee0`. Both come to `0x146750` through `0x145d50`, which asks route after route.

## What an army's think writes

The developer build's write watch ([dev-tools.md](../../dev-tools.md)) on three calls of `0xea250` on the main thread, in the stage, on 1721.1.2 to 1721.1.6: 643, 148 and 2,779 writes. What existed before the call and was written: the agent itself (the 0xbf8-byte block); the world (`CGameState`, 0x7518 bytes) at `+0x1550`; the thread pool library's own data (`tbb.dll`, through `0x178530`); small blocks of the game's allocator (`0x178xxx`, `0x179xxx`); two globals. The rest were blocks made during the call. No army, province or country object was written: those were read.

- **`+0x1550` of the world is its random generator's place** in its state at `+0xb90`: `0x14c71b0` is a Mersenne twister (624 words, offset 397), with no lock. An agent draws from the world's generator, unless the byte `[0x23581c0]` is set; then from its own, at `+0x78` of the agent (`0xe9fd3`, `0x14b50f`, and 146 places in all). Only the command line's `-seed` sets that byte, with the seed at `[0x23581c4]` (`main`, `0x876a8b`, next to `autotest`). Without it, the AI countries' tasks, on many threads at once, draw from one generator, so which numbers a country gets depends on the threads' timing; [main-thread.md](../day/frame-scheduling.md) saw a day draw tens of numbers more or fewer from run to run.
- **One lock for every country** (`[0x2357ba0]`, `0x1a139cc` / `0x1a139f0`) guards some of the AI's shared state; `0xf8ea0` in the search and the plans' `0x146ee0`, `0x1479e0`..`0x147aa0` and others take it.

## Whether a game repeats its days

Measured on 2026-11-07 ([Whether a game repeats its days with -seed](../../research/performance-evidence.md)): the autosave of 1780.7.1 loaded in a new process, observed, and 185 days at speed 5 to the game's autosave of 1781.1.1, a save of 69 MB and 3.6 million lines, the runs' saves compared line by line. A load writes a `campaign_id` of its own and `campaign_length`, its time played; those are left out.

- **Without `-seed` no two runs are alike.** Two runs differed in 3.4 million of their lines. A load does not keep the save's random state: the first run's `multiplayer_random_seed` was 4122684752, the second's 3961721867, the save's 4025518484. So a game loaded twice goes two ways from its first day, whatever the AI does on the threads.
- **With `-seed` most runs are alike.** The game sets its generator's seed to the switch's and its draw count to 0 at the first load of a process; a second load in the same process keeps the save's seed and count, so only a load in a new process repeats. Each agent's own generator is seeded with the same number when the agent is made (`0xe4050`; without the switch, with the time, `0x1a49a5c`), and the AI reads no clock otherwise: the timers the game imports are read only to seed generators, and by the frame's timing (`0x1761e20`..`0x1761fd0`). Of four runs started with `-seed=1` and the AI's walks as the game's, three gave saves identical but for those two lines after the 185 days; the fourth went another way. That is not the threads repeating themselves: with `-seed` the AI stage runs the countries one after another on the main thread, sorted by their index, instead of on the parallel stage (`0x75b891`, `0x75b990`; [ai.md](ai.md), "A day of the AI").
- **What still varies with `-seed`**: in two runs of five, Great Britain's "Royal Navy" got one of two admirals as its leader (`leader` `id=50176` against `id=49643`), and the rest of the save followed it: 6 lines of a diff after a month, about 8,000 to 10,000 after two, and a third of the save after six (monthly autosaves, five runs). Which code picks the admiral, and why it varies, was not found.

So a game started with `-seed` repeats its days, with its AI on one thread, well enough for a bench (S5, [game-speed.md](../../plans/README.md#performance)) to compare two builds by their saves, as long as each is run a few times and its runs are taken in the same history as the other's, the admiral's.

## Whether an AI country's agents depend on each other

A country's top agent has its children (`+0x40`) and then its armies' agents (`+0xc50`) do their day one after another (`0x14b622`..`0x14b675` in `0x14b370`), each list the game's doubly linked list (its first node at `+0`, its last at `+8`; a node's agent at `+0`, the node before it at `+8` and after it at `+0x10`). The developer build's test switch `agents-order` ([dev-tools.md](../../dev-tools.md), `core/src/agents_order.rs`) has the framework walk the two lists: `forward` from the first node as the game does, `reversed` from the last back, each node's neighbor read before its agent's day as the game reads it. With `-seed` each agent draws from its own generator, so the order of the agents changes nothing of what they draw: if no agent read what another of its country did that day, the runs in either order would play the same days.

Five runs from the autosave of 1780.7.1 with `-seed=1` and monthly autosaves ([the same run](../../research/performance-evidence.md)), two `forward` and three `reversed`, the lines of a diff between two runs' saves:

| | After a month (1780.8.1) | After two (1780.9.1) |
| --- | --- | --- |
| Two runs in the same order | 6, or none: the admiral above | 8,199 and 10,444 |
| A run in one order against a run in the other | 14,947 and 14,951 | 29,397 and 32,730 |

- `forward` played the same days as the game's own walk: its half-year run gave the same save as two runs of the player build, line for line. The walk replaced is the game's.
- Reversed, the agents took other decisions within a month, about 7,500 of the save's lines changed against 2: the countries' armies and fleets (`regiment`, `ship`, `morale`, their leaders, `last_target`), their trade (`trading_policy`, `steer_power`, the nodes), their provinces, their relations (`attitude`). So an agent's day reads what the agents before it in its country did that day, beyond the random generator. The code shows which paths can carry it: units handed between agents, fleets taken from the Admiral, the armies' targets that a siege counts, the war plan ([ai.md](ai.md), "What one agent reads of another the same day"); which of them made this difference was not followed.
- Agents running apart on several threads would read some of what the others did and not the rest, by the threads' timing. A reversed order is still one agent after another, as the game runs them, in an order its lists could have had (where their order comes from was not followed); agents run apart could also each miss the others' decisions of the day, which no order of the game's gives. A country's agents were written to run one after another: no lock is known to guard what they share but the one lock (above).


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0xeae58` | code | Referenced while examining the armies' searches on idle threads; complete role not established |
| `0xeb475` | code | army think's war, hire and size |
| `0xeb572` | code | Referenced while examining the armies' searches on idle threads; complete role not established |
| `0xeb64b` | code | army think's friend to follow |
| `0xeb8e3` | code | army think's route check |
| `0xebaa2` | code | Referenced while examining the armies' searches on idle threads; complete role not established |
| `0xf70d0` | function | army's friend to follow |
| `0x102140` | function | army threat at a province |
| `0x1128d0` | function | Referenced while examining where an agent's time goes; complete role not established |
| `0x179eb3` | code | Referenced while examining the ai stage; complete role not established |
| `0x221750` | function | Referenced while examining the whole day, late in a game; complete role not established |
| `0x435c10` | code | Referenced while examining the armies' searches on idle threads; complete role not established |
| `0x786150` | function | Referenced while examining the ai stage; complete role not established |
| `0x8ea4d0` | code | set's search for a modifier added |
| `0x8ea4e0` | code | Referenced while examining the whole day, late in a game; complete role not established |
| `0x8ea730` | function | modifiers taken from a set |
| `0x8ea7e2` | code | set's search for a modifier taken |
| `0x960330` | function | Referenced while examining the whole day, late in a game; complete role not established |
| `0xb4ba90` | function | Referenced while examining the whole day, late in a game; complete role not established |
| `0x1a13a58` | function | Referenced while examining the ai stage; complete role not established |


## Limits of the reading

The write watch covered three short main-thread army thinks, not long plans. The top plan’s query dependencies and the exact reads responsible for order-dependent choices remain unresolved. Results came from one observe-mode save and bounded years, not every player setup.
