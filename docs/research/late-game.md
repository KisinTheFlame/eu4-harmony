# Why days are slower later in a game

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Measured on 2026-11-06 with `cargo xtask profile --slice 60`, which writes a line for each minute of a run: its dates, days and frames a second, and the shares of the main thread's parts ([dev-tools.md](../dev-tools.md)). The parts are those of [main-thread.md](main-thread.md); the runs are in [prototype-log/](../prototype-log/).

In short: **what grows is the AI's handling of armies, and it grows on the main thread's critical path.** In one game the AI played alone from 1444 to 1821, the day's AI stage went from 5 ms of the main thread a game day to 30 ms, as the world's regiments tripled and gathered in a few great powers (the Ottomans' from 249 to 1,575). The stage runs one task per AI country on the worker threads, and the main thread spins until the last of them is done; timed task by task, a stage lasts exactly as long as its longest task, a great power's, which starts first and still ends last, while the workers sleep most of the time. Inside that task its army agents each search the provinces around their army, and on the slowest days its top agent plans, asking route after route. It cannot be made much faster without changing what the AI computes or the order it computes it in ("Whether it can be made faster"); a country's agents read what the agents before them did that day, so running them apart would change the AI's decisions, not only the order of its random draws, while a game started with `-seed` repeats its days well enough to compare builds on ("Whether a game repeats its days"). Over the whole late day ("The whole day, late in a game"), the main thread is what holds the game back, at 2 to 3 of 16 threads busy: the AI is 38% of its day, frames 20%, each country's daily update 18%, and the monthly update stalls the month's first day for 266 ms. The rest of the day did not grow: each country's daily update became cheaper as countries were annexed, and a frame cost the same in 1800 as in 1600. At speed 5 the days went from 19.7 a second around 1650 to 12.5 around 1800.

## Setup

- The machine and screen of [performance.md](../performance.md): Ryzen 7 9700X (8 cores, 16 threads), 3840×2160 borderless, vsync at 144 Hz. The player's playset, the game hidden, the framework's player build of this branch.
- The player's own settings: "Faster time" (`fastest_speed="faster"`, no frames inside a day at speed 5, [game-speed.md](../game-speed.md)) and half-yearly autosaves, uncompressed. So at speed 5 a day is the day's work and, between days, about one frame.
- **The game**: a new game as Castile, `console run observe` at once, so that the AI plays every country; `time.advance` a year at a time at speed 5, which runs while the player observes ([remote-control.md](../remote-control.md)).
  - 1444 to 1608 observing, no country viewed;
  - then the game's autosave of 1600.1.1 loaded again, played as the first great power then, the Ottomans, observed at once, and viewed as the Ottomans (Ctrl and a click on one of their provinces, which makes the game show the observer that country's top bar, outliner and alerts while the AI plays it), on the political map (W), to 1821.1.3. The windows the game opened for the Ottomans' view (the ages' windows) were closed as they came, as a player closes them.
- The profiler ran through both, 9,420 s at 50 samples a second, which slows the game by about 7% throughout.

## The days, twenty-five years at a time

Milliseconds of the main thread for each game day, from the shares (the sampler's suspension shared out among them):

| Years | Days a second | The day | of it, the AI | each country's update | the monthly update | Frames between days |
| --- | --- | --- | --- | --- | --- | --- |
| **Observing** | | | | | | |
| 1444–1468 | 16.2 | 43 ms | 5.5 ms | 21 ms | 9 ms | 19 ms |
| 1469–1493 | 17.6 | 39 ms | 5.1 ms | 19 ms | 9 ms | 18 ms |
| 1494–1518 | 14.7 | 47 ms | 5.4 ms | 24 ms | 10 ms | 21 ms |
| 1519–1543 | 16.9 | 40 ms | 5.2 ms | 20 ms | 9 ms | 19 ms |
| 1544–1568 | 15.5 | 45 ms | 7.6 ms | 21 ms | 9 ms | 20 ms |
| 1569–1593 | 14.5 | 49 ms | 9.8 ms | 23 ms | 10 ms | 20 ms |
| **The Ottomans' view** | | | | | | |
| 1602–1624 | 17.6 | 42 ms | 9.0 ms | 18 ms | 9 ms | 15 ms |
| 1625–1649 | 18.9 | 39 ms | 9.6 ms | 14 ms | 9 ms | 14 ms |
| 1650–1674 | 19.7 | 36 ms | 8.3 ms | 13 ms | 9 ms | 14 ms |
| 1675–1699 | 14.8 | 52 ms | 20.7 ms | 14 ms | 10 ms | 16 ms |
| 1700–1724 | 15.0 | 50 ms | 19.2 ms | 15 ms | 10 ms | 17 ms |
| 1725–1749 | 15.3 | 50 ms | 19.6 ms | 14 ms | 10 ms | 15 ms |
| 1750–1774 | 13.5 | 57 ms | 25.1 ms | 14 ms | 11 ms | 17 ms |
| 1775–1799 | 12.6 | 63 ms | 29.8 ms | 15 ms | 10 ms | 17 ms |
| 1800–1820 | 12.5 | 63 ms | 29.8 ms | 15 ms | 11 ms | 17 ms |

- **The AI grew fivefold**, and with it the day. Nothing else in the day grew: each country's daily update (`0x2ee280`) fell from about 21 ms to 14 as the living countries fell from 665 to 117; the monthly update (`0x75d690`) stayed at 9 to 11 ms, the provinces' passes at 3 to 4.
- **The frames stayed level**: 14 to 17 ms a game day in the Ottomans' view, about one frame a day. Of it, the top bar's alerts grew from 0.9 ms to 2.0 and the outliner from 0.3 to 0.5; drawing stayed at 5 to 6 ms. The view of no country drew more (19 to 21 ms, drawing 9 to 11 ms) because it showed another map mode.
- **The AI rises and falls with the game.** Loaded afresh in 1700 (the game's autosave of 1700.1.1, viewed as the Ottomans), the AI took 10 ms a game day in the first minute, then 15, 23, 21, 17 and 13 ms in the minutes after, 1710 to 1724; the AI's work changes with the wars and the armies on the map, so a stretch of twenty-five years is the measure, not a minute.

## The AI stage

`0x75b4b0`, the AI's part of the day ([architecture.md](architecture.md), "The AI"), is nearly all `0x77d8a0`, a parallel stage over the AI countries: one task for each, each task an AI country's update (`0xb0fa0`).

- **The stage's loop** `0x179e90` is run by the main thread and the workers alike. It takes the next task by its number (`lock xadd [job+0x44], 1`), runs it while the number is below the count (`[job+0xa8]`), and counts it done (`[job+0x48]`). Once no task is left to take, it loops at `0x179eb3` reading the done count until it reaches the count, with no pause or wait in the loop: **the main thread spins until the last task anyone took is done.**
- That spin, `0x179e90`'s own samples, grew from 5.5 ms a game day (1602 to 1650) to 22 ms (1775 to 1820): most of the AI's growth is the main thread waiting for the tasks still running elsewhere.
- **The workers were idle.** In the game loaded at 1700, the sixteen worker threads ran 0.09 cores each (1.4 in all); the busiest thread of each second spent 82% of its samples asleep in the pool's condition variable (`0x1a13a58`), 7% in AI countries' updates. The stage is not short of cores: it waits for its longest tasks, the countries with the most to do.
- **What grew inside an AI country's update**, of the tasks the main thread ran itself, the same number of game days early and late: `0xb0fa0` → `0x786150` → `0x14b370` → `0xe4890` → `0xe9f30` → `0xea250` → `0xeab50` from 543 samples to 1,858. `0xeab50` reports errors from `ai_army.cpp` ("Main army in AI army '", "' does not belong to country! ("), so it is the AI's handling of its armies; under it `0x1022b0` reads provinces by ID (`map.h`). The rest of the AI country's update grew less: `0x14acc0` 260 to 482, `0x13d4c0` 454 to 604.

## Each AI country's task, timed

Measured on 2026-11-07 with the call trace's `times` ([dev-tools.md](../dev-tools.md), "Call tracing"): every call of the stage (`0x77d8a0`) and of each AI country's task (`0xb0fa0`, with the country's tag, at `+0x20` of its argument, the country's AI), on whichever thread it ran. Two runs of the same game, in the Ottomans' view at speed 5: the save of 1700.1.1 played to 1725.1.1 (9,125 stages), and the game's autosave of that run's 1721.1.1 played to 1730.1.1 with the functions inside the task timed too (3,285 stages).

| Years | Stage ms | Tasks | Their work ms | Longest task ms | Main thread's spin ms |
| --- | --- | --- | --- | --- | --- |
| 1700–1704 | 9.0 | 145 | 72 | 8.8 | 4.7 |
| 1705–1709 | 15.1 | 138 | 102 | 14.9 | 10.2 |
| 1710–1714 | 9.0 | 135 | 67 | 8.9 | 5.1 |
| 1715–1719 | 13.6 | 132 | 93 | 13.5 | 8.5 |
| 1720–1724 | 19.0 | 131 | 112 | 18.9 | 13.6 |
| 1721–1729 (second run) | 16.9 | 129 | 113 | 16.8 | 11.0 |

- **A stage lasts as long as its longest task.** The median stage was 1.00 times its longest task, and ended 0.01 ms after it. The work of all the tasks shared out among the 16 threads that ran them would have taken half as long (the median stage was 2.0 times that).
- **The longest task does not start late.** It was the first or second task of its stage to start (the median of 135), 0.02 ms into it: the order in which the stage takes its tasks changes from day to day, and the countries that took longest come first. Starting the longest task at once would have saved 0.1 ms a stage. **The order is not what to change.**
- **The longest tasks are the great powers'**: Austria's (2,899 stages of 9,125) and Great Britain's (2,766) from 1700 to 1725, then the Ottomans' (1,060 of 3,285), Great Britain's, France's and Austria's. One country's task varies greatly from day to day: Great Britain's took 3.4, 8.9 and 20.7 ms on 1700.1.2 to 1700.1.4.
- **The stages have a long tail.** A stage's median was 10.1 ms, its 90th percentile 19.0, its 99th 76.5, the longest 483 ms (Great Britain's task alone). Days whose stage took over 20 ms were 8.6% of the days and 29% of the stages' time; over 50 ms, 1.8% and 16%. Such a day stands out as a stutter at speed 5.
- **Inside a task, the time is in `0xe4890`, called once for each of the AI's items** (each call with an object of its own): 9.8 calls a task on average, 18.5 in the longest 1% of tasks, 22 to 25 in the Ottomans' and Great Britain's. `0xe4890` takes 97% of the longest task of a stage (the median), through `0xea250` → `0xeab50` (`ai_army.cpp`) for most calls and `0x13d4c0` for a few. The path `0xb0fa0` → `0x14b370` holds 92% of every task; `0x786150` comes between them only for smaller countries.
- **One item can take most of a day.** The largest single call of `0xe4890` in a stage's longest task was 6.6 ms on average (the stage 16.9 ms); on days whose stage took over 20 ms it was 45% of the stage, over 50 ms 64%. The longest seen: 141 ms in one call (a colonial nation's, 1728.9.19), and 127 ms in one call of the Ottomans' through `0x13d4c0`.

So, of the stage's 13 to 19 ms a day: if the longest task alone could be split, the next country's would bound the stage, 8.6 ms on average (1700 to 1725); if every country's items could run apart, the largest single item would, 6.6 ms (1721 to 1730); the work shared out evenly is 5.6 ms. The order of the stage's tasks gives nothing.

## An AI country's agents

Read from the code on 2026-11-07, with what [main-menu.md](main-menu.md) found of the agents' objects ("The AI's agents").

- **A country's AI works through agents**, in a tree. Its top agent (vtable `0x1c3e9e0`, at the AI's `+0x28`) is what `0xb0fa0` steps through `0x14b370`; it keeps the agents it started on its list at `+0xc50`, the 0xbf8-byte agents (vtable `0x1c3cdb8`) that lead armies, and steps others too, such as the foreign minister (vtable `0x1c3df20`, `ai_foreignminister.cpp`).
- **An agent's day** is `0xe4890`, slot `+0x58` of every agent's vtable: when its slot `+0x50` says so, it counts its `+0x20` down and, at 0, calls its slot `+0x60`, its think, whose result is the number of days to its next think, kept at `+0x20`; then it steps the agents on its own list at `+0x40`. The top agent does the same at `0x14b603`..`0x14b673` and then walks its list at `+0xc50`. So the items of the timed task are agents, and the costly ones those whose think comes that day: on a day few think, on another many, which is why one country's task varies so much from day to day.
- **An army agent's think** is `0xe9f30`, which calls `0xea250`; the foreign minister's is `0x13d4c0`.
- **The top agent's plan.** Before its agents, `0x14b370` reads its byte `+0xce0` under a lock shared by every country (`[0x2357ba0]`); when it is set and its list at `+0xcc0` is empty, it plans with `0x147660`, and otherwise, one day in twelve at random (`0x14b52c`, a draw modulo 12 equal to 1), with `0x146ee0`. Both come to `0x146750` through `0x145d50`, which asks route after route.

## Where an agent's time goes

Sampled on 2026-11-07 with `cargo xtask profile --threads --slice 1` ([dev-tools.md](../dev-tools.md)), which samples every thread and merges them into the whole game's CPU: the run's autosave of 1721.1.1, in the Ottomans' view at speed 5, 240 s, 1721 to about 1731. The whole game ran 2.17 cores (the main thread 0.94).

| Inside | Cores |
| --- | --- |
| The AI countries' updates (`0xb0fa0`), on every thread | 1.01 |
| An army agent's think (`0xea250`) | 0.62 |
| its search (`0xeab50` → `0x1022b0`) | 0.40 |
| Route queries (`0x96c520`, [routes.md](routes.md)), from every caller | 0.23 |
| The stage's spin (`0x179e90`) | 0.27 |

- **The search `0x1022b0`** is a best-first search out from the army's province: a heap of 0x28-byte nodes kept by `0x1128d0`, each province reached weighed by `0xf8ea0` (which takes the shared lock `[0x2357ba0]`), by the route edge check `0x96f860` (whose blocking check `0xd45250` → `0xd40ae0` reads the army's route) and by `0x102140`. It is 65% of an army's think.
- **The slowest seconds.** Of the run's 236 seconds, the slowest tenth went at 6.1 days a second, the middle half at 15.7. Per game day, the whole game's CPU was 328 ms against 141; route queries 68 ms against 15; the stage's spin 67 against 17; the monthly update 38 against 10; an army's think 66 against 44.
- **A slow day is the top agent's plan.** In the slowest seconds 57% of the route queries came from `0x14b370` → `0x147660` → `0x145d50` → `0x146750`, against 2.6% in the middle half: a great power's plan asks route after route, each up to about 10 ms, while the main thread spins. The rest of the slow seconds' route queries came from army thinks (`0x106320`, `0xf3f80` → `0xf4490` → `0x109ee0`).
- **Route queries are not asked again.** The call trace's log of 18,666 route queries from 1721.1.1 to 1721.12.27, 52 a day: the same army, origin and goal was asked again the same day 6.2% of the time from the same call site, 13.4% from any; a query took under 1 ms at the median and 11 ms at most (to the log's millisecond).

## What the main thread waits on in the AI stage

Sampled on 2026-11-08 with `cargo xtask profile --threads --waiting-in 0x179e90 --slice 60` ([dev-tools.md](../dev-tools.md)): the 1780.7.1 save ("Saves to test with"), in observe mode at speed 5, 180 s, 1780.10.12 to 1788.10.23, 16.3 days a second, the player build of master at `c201eac0`. Whenever the main thread spins in `0x179e90` or sleeps, the tick's wait is shared out among the threads that run at it: what runs alone, the longest tasks once the others are done, takes the whole of it, so that a function's share is what the main thread waited on it, not the CPU it took.

- The AI stage was 25% of the main thread's samples, 15.3 ms a game day. The main thread waited 11.3 ms of it, and ran its own share of the tasks the rest; 0.4 ms of the wait found no other thread running.

| What ran while the main thread waited in the AI stage (inclusive) | ms a game day | Share |
| --- | --- | --- |
| An AI country's update (`0xb0fa0`), its King's day (`0x14b370`) | 10.3 | 92% |
| The agents' day (`0xe4890`) | 9.7 | 86% |
| An army agent's think (`0xe9f30` → `0xea250`) | 8.2 | 73% |
| its search (`0xeab50` → `0x1022b0`) | 5.7 | 51% |
| the search's route edge check (`0x96f860`) | 2.8 | 25% |
| Route queries (`0x96c520`), from every caller | 2.3 | 20% |
| The search's score (`0xf8ea0`) | 2.1 | 18% |
| The recruitment plan's think (`0x168070`) | 0.6 | 6% |

- **Half of the main thread's wait in the AI stage is the armies' searches**, more than their share of the AI's CPU (0.40 of 1.01 cores in "Where an agent's time goes"): the tasks that end last are the great powers', which have the most armies. Searching a country's armies on idle threads would be bounded by that half, less what the country's other agents do meanwhile.
- **The recruitment plan is little of the wait**, though its thinks were timed at 2.8 ms a day in Austria's task ([ai.md](ai.md)); why is not verified: those times went through the call trace's observer, and from another year's game.
- Each stretch of 60 s had a third of the AI's wait (34%, 33%, 33%).

## What an army's think writes

The developer build's write watch ([dev-tools.md](../dev-tools.md)) on three calls of `0xea250` on the main thread, in the stage, on 1721.1.2 to 1721.1.6: 643, 148 and 2,779 writes. What existed before the call and was written: the agent itself (the 0xbf8-byte block); the world (`CGameState`, 0x7518 bytes) at `+0x1550`; the thread pool library's own data (`tbb.dll`, through `0x178530`); small blocks of the game's allocator (`0x178xxx`, `0x179xxx`); two globals. The rest were blocks made during the call. No army, province or country object was written: those were read.

- **`+0x1550` of the world is its random generator's place** in its state at `+0xb90`: `0x14c71b0` is a Mersenne twister (624 words, offset 397), with no lock. An agent draws from the world's generator, unless the byte `[0x23581c0]` is set; then from its own, at `+0x78` of the agent (`0xe9fd3`, `0x14b50f`, and 146 places in all). Only the command line's `-seed` sets that byte, with the seed at `[0x23581c4]` (`main`, `0x876a8b`, next to `autotest`). Without it, the AI countries' tasks, on many threads at once, draw from one generator, so which numbers a country gets depends on the threads' timing; [main-thread.md](main-thread.md) saw a day draw tens of numbers more or fewer from run to run.
- **One lock for every country** (`[0x2357ba0]`, `0x1a139cc` / `0x1a139f0`) guards some of the AI's shared state; `0xf8ea0` in the search and the plans' `0x146ee0`, `0x1479e0`..`0x147aa0` and others take it.

## Whether a game repeats its days

Measured on 2026-11-07 ([Whether a game repeats its days with -seed](../prototype-log/2026-11-07-seed-repeats-agents-reordered.md)): the autosave of 1780.7.1 loaded in a new process, observed, and 185 days at speed 5 to the game's autosave of 1781.1.1, a save of 69 MB and 3.6 million lines, the runs' saves compared line by line. A load writes a `campaign_id` of its own and `campaign_length`, its time played; those are left out.

- **Without `-seed` no two runs are alike.** Two runs differed in 3.4 million of their lines. A load does not keep the save's random state: the first run's `multiplayer_random_seed` was 4122684752, the second's 3961721867, the save's 4025518484. So a game loaded twice goes two ways from its first day, whatever the AI does on the threads.
- **With `-seed` most runs are alike.** The game sets its generator's seed to the switch's and its draw count to 0 at the first load of a process; a second load in the same process keeps the save's seed and count, so only a load in a new process repeats. Each agent's own generator is seeded with the same number when the agent is made (`0xe4050`; without the switch, with the time, `0x1a49a5c`), and the AI reads no clock otherwise: the timers the game imports are read only to seed generators, and by the frame's timing (`0x1761e20`..`0x1761fd0`). Of four runs started with `-seed=1` and the AI's walks as the game's, three gave saves identical but for those two lines after the 185 days; the fourth went another way. That is not the threads repeating themselves: with `-seed` the AI stage runs the countries one after another on the main thread, sorted by their index, instead of on the parallel stage (`0x75b891`, `0x75b990`; [ai.md](ai.md), "A day of the AI").
- **What still varies with `-seed`**: in two runs of five, Great Britain's "Royal Navy" got one of two admirals as its leader (`leader` `id=50176` against `id=49643`), and the rest of the save followed it: 6 lines of a diff after a month, about 8,000 to 10,000 after two, and a third of the save after six (monthly autosaves, five runs). Which code picks the admiral, and why it varies, was not found.

So a game started with `-seed` repeats its days, with its AI on one thread, well enough for a bench (S5, [game-speed.md](../game-speed.md)) to compare two builds by their saves, as long as each is run a few times and its runs are taken in the same history as the other's, the admiral's.

## Whether an AI country's agents depend on each other

A country's top agent has its children (`+0x40`) and then its armies' agents (`+0xc50`) do their day one after another (`0x14b622`..`0x14b675` in `0x14b370`), each list the game's doubly linked list (its first node at `+0`, its last at `+8`; a node's agent at `+0`, the node before it at `+8` and after it at `+0x10`). The developer build's test switch `agents-order` ([dev-tools.md](../dev-tools.md), `core/src/agents_order.rs`) has the framework walk the two lists: `forward` from the first node as the game does, `reversed` from the last back, each node's neighbor read before its agent's day as the game reads it. With `-seed` each agent draws from its own generator, so the order of the agents changes nothing of what they draw: if no agent read what another of its country did that day, the runs in either order would play the same days.

Five runs from the autosave of 1780.7.1 with `-seed=1` and monthly autosaves ([the same run](../prototype-log/2026-11-07-seed-repeats-agents-reordered.md)), two `forward` and three `reversed`, the lines of a diff between two runs' saves:

| | After a month (1780.8.1) | After two (1780.9.1) |
| --- | --- | --- |
| Two runs in the same order | 6, or none: the admiral above | 8,199 and 10,444 |
| A run in one order against a run in the other | 14,947 and 14,951 | 29,397 and 32,730 |

- `forward` played the same days as the game's own walk: its half-year run gave the same save as two runs of the player build, line for line. The walk replaced is the game's.
- Reversed, the agents took other decisions within a month, about 7,500 of the save's lines changed against 2: the countries' armies and fleets (`regiment`, `ship`, `morale`, their leaders, `last_target`), their trade (`trading_policy`, `steer_power`, the nodes), their provinces, their relations (`attitude`). So an agent's day reads what the agents before it in its country did that day, beyond the random generator. The code shows which paths can carry it: units handed between agents, fleets taken from the Admiral, the armies' targets that a siege counts, the war plan ([ai.md](ai.md), "What one agent reads of another the same day"); which of them made this difference was not followed.
- Agents running apart on several threads would read some of what the others did and not the rest, by the threads' timing. A reversed order is still one agent after another, as the game runs them, in an order its lists could have had (where their order comes from was not followed); agents run apart could also each miss the others' decisions of the day, which no order of the game's gives. A country's agents were written to run one after another: no lock is known to guard what they share but the one lock (above).

## Whether it can be made faster

Against the bar of [ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/2026-11-04-making-the-game-faster-leaves-what-it-computes-as-it-is.md): the same AI, the same decisions taken by the same code.

| What | What it could give | Why not, or what it needs |
| --- | --- | --- |
| The stage's order of tasks | 0.1 ms a stage | The heaviest already start first. |
| Remembering route queries for the rest of the day | at most 13% of route queries, about 2 ms a day | Few are asked again; and it needs a query's answer to depend on its arguments alone within a day, not shown. |
| A country's agents run apart | the stage bound by its largest single item, 6.6 ms a day against 16.9 (1721 to 1730) | An agent's day reads what the agents before it in its country did that day: in another order they take other decisions, with each agent's draws its own ("Whether an AI country's agents depend on each other"). Run apart, each would see some of the others' decisions and miss the rest. Kind C, and it changes what the AI decides, not only the order of its draws. |
| The top agent's plan asking its routes on several threads | the plan's route queries, about 40 ms of CPU a slow day | Its loop decides each query from the last's answer or not, which is not known; kind C as well. |
| The route query and the search's checks rewritten to give the same answers faster | part of 0.6 cores | Every cost, tie and order in the heap must come out the same; a rewrite of the game's AI code, with no way yet to check it but by its answers. |

So the late game's AI cannot be made much faster without changing what it computes or the order it computes it in. The largest gain, a country's agents running apart, is of kind C, and it would change the AI's decisions: not only through the random generator, but through what each agent reads of the others' day. Within the bar, what is left is small (repeated routes, about 2 ms a day). A game started with `-seed` repeats its days, all but a rare admiral ("Whether a game repeats its days"): a bench that compares builds by their saves can be made on it.

## The armies' searches on idle threads

The bar above was the old one. Since [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md) a change may differ from the game's code when it says why; the first is this one, of the first kind there, when the work is done (`core/src/army_search.rs`).

**What it does.** Once a country's King has thought (`0x14b622` in `0x14b370`), the framework finds the army agents that think today and will search, works out the arguments each think will give the search, and hands the searches to a pool of its own threads (up to 8). When a think calls the search, it takes the result worked out for it if its arguments and the agent's and army's state are the same, and otherwise searches itself. A search worked out ahead reads the world as it stood when the King had thought, before its ministers and the armies before it did their day; the game's own threads already read other countries' state at such moments.

**Which armies.** A think ends before its search on many paths of `0xeab50`, and a search prepared for one of those is worked out for nothing, while it holds a thread the others wait for. Counted in the game, 48% of the searches prepared were never called for. Most of them were armies at peace: an army whose country is in no war, not hired and not a navy over its size goes on to its search on one think in ten, by a random draw, or once 15 thinks have passed without one (`+0xbf0`, `0xeb475`..`0xeb572`). Those are now left to the think, since the draw cannot be read without taking it; so are those that never searched in 140,000 prepared: a target over the sea, a battle, a retreat, `+0x218`, a friendly army to follow in the province (`0xf70d0`, `0xeb64b`), a move locked or the unit's flag (`0xeae58`). A think whose route check fails still searches, deep and without the score of where it goes (`0xeb8e3`, `0xebaa2`).

**The country's access cache.** The search's fourth argument is the agent's day's, the country's access cache (`AI+0x60`, [ai.md](ai.md)), and its steps add to it (`0x96f860` → `0x1a2280` → `0x374c50` → `0x415b00`). Searches of one country on several threads added to that map at once and corrupted the heap: the game ended with `0xc0000374`, no crash folder and nothing in the logs, or crashed in the cache's insert (`0x435c10`). The pool's searches are now given no cache, and look access up each time; the answers are the same, an access query without a cache only counts a global miss (`0x2458e70`).

Measured on 2026-11-08 with the developer build at speed 5 from the 1780.7.1 save in observe mode, 120 s a run, the main thread's waits shared out (`cargo xtask profile --threads --waiting-in 0x179e90`, [dev-tools.md](../dev-tools.md)), alternate runs with the searches the game's (`game-searches-armies`):

| | The game's searches (2 runs) | The framework's (2 runs) |
| --- | --- | --- |
| The search (`0x1022b0`) in the main thread's wait in the AI stage | 4.9, 5.5 ms a game day | 1.3, 1.6 ms |
| The AI stage's whole wait | 15.4, 16.4 ms | 15.8, 19.8 ms |

- **The search leaves the wait**: 3.5 to 3.9 ms a game day less of it in the AI stage's wait, in every run. The whole wait does not show it: these runs went to wars, with route queries (`0x96c520`) from 4.6 to 12.1 ms a day, the runs of one save parting from each other within days ([The armies' searches on idle threads](../prototype-log/2026-11-08-armies-searches-idle-threads.md)). Whole-game speed is for a benchmark of many runs, not for a change's runs.
- **Of 40,000 searches prepared** (one run): 84% taken; 2% prepared for a think that did not search; 14% not begun when their think came, and searched by the think; 17% under way, the think waiting 1 to 2 ms for them on average. 8% of the searches the thinks made had none prepared, mostly armies at peace on their one think in ten.
- **The difference**: with `check-army-searches` every search taken was worked out again as the game does at the think: 4.3% of them gave another result (1,527 of 35,145), from what the King's ministers and the armies before it changed in between.
- Not verified: other saves and years, a game a player plays; how much of the King's thread preparing takes on the stage's longest task (2.7 to 3.1 s for 40,000 searches, all countries together).

## The whole day, late in a game

Measured on 2026-11-07 on the same game, in the Ottomans' view at speed 5: every thread sampled (`cargo xtask profile --threads`) from 1721.1.1 for 300 s and from 1759.7.1 to 1821.1.3 for 1,500 s in stretches of 300 s; and the day's phases timed with the call trace's `times` from 1780.7.1 for 306 s, 4,745 days.

**The main thread is what holds the game back.** It ran 0.92 to 0.94 cores; the whole game 2.2 to 2.9 of the machine's 16 (8 cores). Of the day's phases only the AI's runs on the other threads: each country's daily update, the monthly update and the provinces' serial pass ran on the main thread alone, with no sample of them on another thread.

Milliseconds of the main thread a game day, from the sampled shares:

| Years | Days a second | The day and what follows | the AI | each country's daily update | the monthly update | the provinces' passes | the day, other | between days |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1721–1735 | 14.7–21.3 | 47–68 | 9.4–20.5 | 10.3–16.1 | 8.8–10.3 | 3.3–4.6 | 2.1–3.4 | 12.6–15.4 |
| 1759–1774 | 17.8 | 56 | 15.6 | 11.9 | 9.2 | 3.2 | 2.6 | 13.5 |
| 1774–1786 | 14.4 | 69 | 26.4 | 12.4 | 9.4 | 3.3 | 3.3 | 14.7 |
| 1786–1797 | 13.8 | 72 | 29.8 | 12.1 | 9.5 | 3.4 | 2.9 | 14.4 |
| 1797–1810 | 15.3 | 65 | 22.0 | 12.7 | 9.8 | 3.3 | 3.0 | 14.5 |

(1810 to 1821 is left out: its last 34 s ran paused at the end of the run.)

**Day by day**, timed from 1780.7.1 (a day from one day's body to the next, 64.4 ms on average):

| Phase | Calls | Mean ms | Median | 99th percentile | Longest |
| --- | --- | --- | --- | --- | --- |
| The day's body (`0x75bbe0`) | 4,745 | 42.3 | 37.6 | 107 | 595 |
| of it, the AI (`0x75b4b0`) | 4,745 | 24.2 | 20.2 | 76 | 576 |
| of it, the countries (`0x75b0d0`, each country's daily update) | 4,745 | 11.9 | 10.1 | 52 | 101 |
| of it, the provinces (`0x75b2d0`) | 4,745 | 3.2 | 3.1 | 4.7 | 12 |
| The monthly update (`0x75d690`), after the body | 156 | 266 | 265 | 283 | 290 |
| The yearly update (`0x75e260`) | 13 | 10.0 | 10.0 | 10.5 | 11.9 |
| Drawing (`0x8183b0`), 1.53 frames a day | 7,256 | 4.9 | 4.4 | 16 | 132 |

- **The month's first day stalls.** The monthly update took 265 to 290 ms every time, on the main thread alone, so that day took 355 ms against 55 for another (medians 323 and 50): at 15 days a second, a stall of a third of a second every two seconds. The month's days were 3.3% of the days and 18% of the time. The monthly update did not grow with the game: 9 to 11 ms a day, its share, from 1444 to 1821 (above).
- **An average day of 64.4 ms**: the AI 24.2, each country's daily update 11.9, the monthly update 8.8 (its share), drawing 7.5, the rest between days 5.8 (the commands, the interface's update, the map's markers, the alerts), the provinces 3.2, the rest of the day's body 3.1.

**What the monthly update does.** Sampled from 1759 to 1821, of its time:

- `0x2f36c0`, 49%: spread over many functions, each under 8% of it (`0x18f040`, `0x327dc0` → `0x93d130` making a country's modifiers again, `0x302ee0`, `0x2f9c50`, `0x329140`, `0x4109f0`).
- `0x763470` → `0x3bc640` → `0xd54600` → `0x8ea420`, 22%, nearly all of it in `0x8ea420`'s own code.
- `0x9f35c0` → `0x9f9500`, 17%: `0x960330` and `0x9f95d0` ([main-menu.md](main-menu.md) names `0x9f9500` the modifiers a load works out).
- `0xb4ba90`, 5%, again through `0xd54600` → `0x8ea420`.

**`0x8ea420` adds one set of modifiers' values to another** ([main-menu.md](main-menu.md), "What points to them"): for each (modifier, value) of the set added, when the receiving set keeps its values by modifier (its byte `+0x10` set, a modifier below 0x286), it adds at that place; otherwise it looks for the modifier among the receiving set's pairs one by one (`0x8ea4e0`) and adds there, or appends a pair, growing the array (`0x1a33310`) as it fills. So adding to a set kept as pairs costs the product of the two sets' sizes. Its own code was 4.9% of the main thread over the 25 minutes, about 3.3 ms a day on average; 64% of that was in the monthly update through `0xd54600`, 11% in the day's body through `0x22c080` → `0x221750`, and about 10% where a country's modifiers are made again (`0x327dc0` → `0x93d130`), in its daily and monthly updates.

**Between days**, 1.53 frames a game day at "Faster time" (the frame count against the days): drawing 4.9 ms a frame, 7.5 ms a day; the rest of what runs between days 5.8 ms ([main-thread.md](main-thread.md), "Drawing", found no drawing left out that the player does not see).

### Adding sets of modifiers

`0x8ea420(set, added)` and its twin `0x8ea730`, which takes the values away, search a set kept as pairs for each modifier added, pair by pair from the first (`0x8ea4d0`, `0x8ea7e2`: the set's pairs at `+0x58`, their count at `+0x64`). The framework replaces those two searches alone (`core/src/modifier_sums.rs`): after four searches in one call of a set of 16 pairs or more, it puts the set's pairs in an index of its thread's, takes up the pairs the game appends as the call goes on, and answers each search from it. Every answer is the first pair holding the modifier, as the game's search gives it; the adding, the appending, the set's growth and the record of the sets added stay the game's code. A set kept by modifier (`+0x10`) writes modifiers into its places as the call goes on, and is searched pair by pair as before. A difference of the second kind, the same answer ([ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md)).

Measured on 2026-11-08 in a test game ([the run](../prototype-log/2026-11-08-modifier-sets-through-an-index.md)), the developer build, the 1780.7.1 save in observe mode at speed 5, 120 s a run, the main thread sampled (`cargo xtask profile --test-game`):

- **The same answers**: with `check-modifier-sums`, 63 million searches through the index made as the game's too, none differing.
- **The cost, per game month of the main thread**: `0x8ea420`'s own code 101 and 107 ms with the game's code (`game-sums-modifiers`, the framework's points left out), against 16 and 20 ms with the index, and 22 and 24 ms in the framework's search, whose samples the sampler cannot walk past its stub: about 104 ms a month against 41.
- The monthly update as the sampler counts it went from 323 and 287 ms a month to 257 and 250, the framework's search left out of it; the months of different runs differ, and part of `0x8ea420`'s time is in the days' updates.

### Where the late day could be shorter

| What | Its time late | What a change could give | Of which kind ([game-speed.md](../game-speed.md)) |
| --- | --- | --- | --- |
| The AI | 24 ms a day, 38% | Above, "Whether it can be made faster": little without changing the AI | C |
| The monthly update | 266 ms a month, 8.8 ms a day | `0x8ea420` looking its pairs up through an index made for the call instead of one by one: built, about 60 ms a month of 104 ("Adding sets of modifiers"). The rest of the month is many countries' and provinces' updates one after another, nothing alone large | C: the simulation's own code, the same answers checked call by call |
| Each country's daily update | 12 ms a day | One country after another, spread over many functions (units, armies, diplomacy's caches `0x93fe30`, modifiers); none found alone large | C |
| Drawing between days | 7.5 ms a day | Drawing fewer frames at speed 5, as "Faster time" leaves out those inside a day; not researched | A or B, the player's choice |
| Running the countries' or the month's updates on the idle threads | up to the most of 21 ms a day and 266 ms a month | Changes the order the game does its work in, as running the AI's agents apart would | C |

## What the world gathers

Counted in the game's autosaves of the run (a script of the run's own):

| Date | Living countries | Armies | Regiments | Most regiments |
| --- | --- | --- | --- | --- |
| 1445.7.1 | 665 | 726 | 4,143 | Ming 64, the Ottomans 30 |
| 1525.7.1 | 370 | 509 | 4,003 | the Ottomans 89, Ming 80 |
| 1601.1.1 | 245 | 404 | 4,240 | the Ottomans 249, Austria 160, Great Britain 143 |
| 1661.1.1 | 169 | 352 | 5,168 | Great Britain 305, the Ottomans 296 |
| 1701.1.1 | 144 | 424 | 7,444 | Great Britain 544, the Ottomans 447 |
| 1761.1.1 | 132 | 492 | 9,829 | Great Britain 866, the Ottomans 593 |
| 1801.1.1 | 129 | 559 | 12,329 | the Ottomans 1,219, Great Britain 1,018 |
| 1821.1.1 | 117 | 588 | 13,532 | the Ottomans 1,575, Great Britain 1,021 |

- Regiments tripled after 1600, while the armies grew by half: armies grew larger, and the largest countries' grew most. The AI's army handling grew 3.4 times over the same years, and the stage's spin with it.
- The save grew from 38 MB in 1445 to 66 MB in 1700, most of it the history kept for provinces and countries (10,000 dated entries in 1445 to 164,000 in 1605) and the countries' relations with each other (`active_relations`, 120,000 entries to 305,000 in the same years). Neither showed in the day's cost.

## Two other runs, and what they showed

- **A player who plays nothing** (a new game as Castile, questions answered with their first option, offers declined, 1444 to 1580): the days fell from 12.7 a second to 6.8, nearly all of it in the frames, from 20 ms to 74 ms each, the GUI drawing its objects (`0x14dd010` → `0x14dd350`). The game opens a window for many of its messages, and such a player closes none; every frame draws them all. Loaded afresh, the same game of 1580 ran at 15.1 days a second and drew 144 frames a second paused, against 14. A run that measures the game must close its windows as a player does, or observe.
- **Saves loaded afresh**, 120 s at speed 5 each: England 1444.11.11, 14.9 days a second; the player's Spain of 1618.9.2, 13.1; the player's Holy Roman Empire of 1724.5.13, 31.9, its day a quarter of the others' and the AI under half a millisecond: in that game the player had united the world, so no country was left to the AI.

## What this means

- Later in a game the day is slower because the AI countries that are left have more armies to handle, and the largest of them set how long the AI stage lasts. A player's own country has no AI, so a player's own armies do not count here; the AI great powers' do.
- More cores would not help: the stage already leaves most of them idle, and the stage already starts its longest tasks first. What would is the largest countries' tasks taking less time, or their agents running apart, and on the worst days a great power's plan, which asks route after route, taking less time. Each changes the AI's own code and the order its work is done in, and the agents' order changes what they decide ("Whether an AI country's agents depend on each other"): a change of kind C ([ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/2026-11-04-making-the-game-faster-leaves-what-it-computes-as-it-is.md)), which needs the bench that repeats ([game-speed.md](../game-speed.md), S5 and S6, set aside); "Whether it can be made faster" above weighs each.
- A late game the player feels to be slow can also be slow for reasons of the session, not of the date: the windows left open, or (with the game's own pace instead of "Faster time") the frames inside a day.

## Saves to test with

The game's autosaves of the run are kept outside the game's folders, for runs that need a mid or late game, at `%LOCALAPPDATA%\eu4-harmony-dev\saves\` on the machine that made them:

- `observer-1600.1.1.eu4`, observing (`player="O00"`). `menu.load` refuses it ("no country O00"): copy it into a test game's save games folder (`user\save games\` in the claim's folder, [test-games.md](../test-games.md)) with its header's `player` set to a country (`"TUR"`) and `not_observer=yes`, load that, and observe.
- `observer-1700.1.1.eu4`, written while the Ottomans were viewed, so `player="TUR"`: `menu.load` loads it as the Ottomans; observe, and view them again.

A claim's release removes what a run added to the save games folder, so a copy there goes with it.

## Not verified

- Which of the paths by which an agent reads what the agents before it did that day ([ai.md](ai.md)) made the reversed order's difference.
- What the top agent's plan decides, and whether its route queries depend on each other.
- That the AI countries' draws from the world's generator race between threads without `-seed`: read from the code (no lock in `0x14c71b0`), not seen apart from a load's new seed, which makes runs differ anyway. What picks the Royal Navy's admiral, which varies with `-seed`; whether `-seed` repeats other saves, earlier years, or longer than half a year as well; whether the agents' order matters in the children's list, the armies' list or both.
- The write watch saw three army thinks on the main thread, short ones; not a plan, nor one of the long thinks.
- The tasks timed in one game from 1700 to 1730 only; the route queries counted over one year; the day's phases timed for 306 s from 1780.
- What `0x2f36c0` is beyond the functions it calls; whether the countries' daily updates or the month's updates read what another country's writes.
- One game only, as the AI played it; a game a player plays, with the player's country out of the AI, and other great powers.
- The game's own pace (frames inside a day) over a long run, and a vanilla game without the framework.
- How near the wait shared out by ticks comes to the stage's longest task: it was not sampled beside the tasks' timing.
- What the thinks of armies at peace decide on their nine thinks in ten without a search, and whether more of an army's think could run ahead ("The armies' searches on idle threads").
