# What the main thread waits for in the AI stage (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-08 as the third step of [game-speed.md](../plans/game-speed.md), "How a part is chosen", for its largest candidate, the AI stage's wait, with S9 (the armies' searches on idle threads) and S10 (sets of modifiers through an index) built. From the sampler and the call trace on test games ([the AI stage's wait, measured](../prototype-log/2026-11-08-ai-stage-wait-measured.md)) and from the game's code. What the AI's agents are and do is in [ai.md](ai.md); how the stage waits, and what it waited for before S9, in [late-game.md](late-game.md).

In short: late in a game the main thread waits **17 ms of a 67 ms day** for the AI stage's last task, a great power's: Austria-Hungary's (`AUH`), the Commonwealth's (`PLC`) or Great Britain's (`GBR`) on four days in five. What runs then is that country's army agents' thinks, and half of it is route queries. The largest single asker of them is **the support check** (`0x106320`): an army near enemies asks whether the friendly strength that can reach it in time outnumbers the enemy strength that can, with a route query for every unit of either side close enough. It counts the friends first and the enemies after, and the enemies' sum is the bar the friends must pass, so the friends' queries cannot be cut short as its code runs. With the enemies counted first, the friends' could be, with the same answer: at most the friends' queries of the checks that answer yes, 41% of the check's query time, under 2 ms of the wait a day, for a rewrite of the whole check. Not worth a prototype; the rest of the wait is spread over the army agents' other decisions. (Corrected on 2026-11-08: this note first read the check as friends against a bar given, and bounded a way that does not hold at 3.6 ms.)

## The wait, with S9 and S10 built

Sampled with `cargo xtask profile --threads --waiting-in 0x179e90`, the 1780.7.1 save in observe mode in the Ottomans' view at speed 5, 240 s, 1780.7.2 to 1790.9.2, 14.9 days a second, 67 ms of the main thread a day. The AI stage was 35% of the main thread; it waited **17.0 ms a day** in it, 13 to 23 in each minute, the most in the last. What ran meanwhile, each tick's wait shared out among the threads that ran (so the most to the tasks that run alone, at the end):

| What ran while the main thread waited in the AI stage (inclusive) | ms a game day |
| --- | --- |
| An AI country's update (`0xb0fa0`) | 15.9 |
| its King's day (`0x14b370`) | 15.3 |
| An army agent's think (`0xe9f30` → `0xea250`) | 11.2 |
| of it, the main army's decision (`0xeab50`, its search `0x1022b0` among it) | 3.4 |
| of it, the support check (`0x106320`), asked directly | 3.1 |
| of it, its other units (`0xf3f80` → `0xf4490`), the support check among it | 2.8 |
| of it, an army over the sea (`0xf1390`): transports | 2.0 |
| Route queries (`0x96c520`), from every caller | 8.2 |
| of them, the support check's (`0x106320`, from the think and from `0xf4490`) | 5.0 |
| of them, the transports' (`0xf1390` → `0xdc8a0`, `0xeea70`, `0xee5a0`) | 1.7 |
| of them, the King's targets (`0x14acc0` → `0x14e3d0`) | 0.4 |
| The recruitment plan's think (`0x168070`) | 1.7 |
| No thread ran | 0.6 |

Of every thread's samples in the run, the whole game merged, route queries were 16%, the support check's 46% of them.

## Whose task the stage waits for

Timed with the call trace's `times` over every AI country's task (`0xb0fa0`, its country's tag at `+0x20` of its argument), 1780.7.2 to 1782.7.1, 730 stages:

- The longest task of a stage was `AUH`'s on 246 days, `PLC`'s on 201, `GBR`'s on 121, a colonial nation's (`C04`) on 34, `SPA`'s on 31: three countries on 77% of the days.
- The longest task took 16.6 ms on average (median 12.0), the second longest 11.9: splitting the longest country's task alone would bound the stage by the next country's.
- Each country's tasks over the day: `AUH` 11.8 ms, `GBR` 10.2, `PLC` 9.3, `TUR` 4.9, then 4.3 and under.

## The support check

`0x106320`(agent, province, army, strength there, share asked, …), eight arguments, answers yes or no (`al`). An army agent's think (`0xea250`) asks it for its main army where it stands (`0xea430`), when the agent's `+0xb1c` is set, with the strength in that province worked out first (`0xd46800`, into the fourth argument) and a share of 1000; its other units' decisions ask it too (`0xf4490`). A no sets the agent's `+0xb1e` and `+0xb1f` and holds that for 7 thinks (`+0xb20`); a yes counts that down. What the agent does differently while `+0xb1e` is set was not followed; inferred, it keeps away from where it would be outnumbered.

What it works out, in the game's terms: **whether the friendly strength that can reach this province in time outnumbers the enemy strength that can by the share asked.** (Corrected on 2026-11-08: it was first read as the friends against a strength given, which the third loop below shows it is not.)

1. The friends' sum begins at the strength already in the province and a constant (`[0x1fd3b38]` plus the fourth argument's, `0x1063c7`), in `r15`.
2. **The country's own units** (the list at `country+0xd18` of the agent's country, `0x106405`..`0x106422`): for each, a route query (`0x10671e`); one that can get there in time adds its strength (`0xd46800`, or a cached value when it stands there already), scaled down by how long it would take (`add r15, r8`, `0x1068aa`).
3. **Its allies in its wars**: for each country in its `current_war_allies` (`country+0x1bc0`), that country's units, the same way (route query `0x106c3d`, `add r15, r8` at `0x106dca`).
4. **The enemies**: for each country in the list at `country+0x1b88` (_inferred_ its enemies in its wars), that country's units, the same way, but into a second sum, the strength to beat, which begins at 1.0 (`mov r12d, 0x8000`, `0x106e01`; route query `0x107194`, `add r12, r8` at `0x107328`), with their count in `[rbp+0x158]`.
5. After the three loops: the friends' sum over the enemies' (`idiv r12`, `0x107433`), scaled by the seventh argument, against the share asked (the fifth, 1000 from the think), yes when it is larger (`0x107401`..`0x107459`).

In each loop it leaves out the army itself, units without a valid owner, units its method `+0x88` refuses, those retreating (`+0x203`), of three kinds by their method `+0x50` (5, 11 and 12), and those whose map distance to the province, made a time by a square root (`0x88a390`, `0x1065eb`..`0x106634`), is over the define at `[0x233fc7c]`.

Only the answer leaves the function: the routes go into lists of its own, freed after each query ([routes.md](routes.md), "What it writes"), and the cleanup after the loops (`0x10cd80`) gives nothing it uses. Each term added is a strength scaled by a positive factor, and both sums are integers, so neither depends on the order its terms are added in. But the bar the friends must pass is the enemies' sum, which the third loop works out after them: **while the friends are counted, the bar is not yet known**, so the friends' queries cannot be stopped early as the game's code runs. What can, each with the same answer: the enemies' loop stopping once the friends can no longer pass (a no), and, with the enemies counted first, the friends' loops stopping once they have passed (a yes).

**How often**, timed over ten years with the call trace, 1780.7.2 to 1790.5.12 (its own cost slows each call, so the milliseconds are larger than without it, but the years can be set against each other):

| Year | Checks a day | Their route queries a day | Those queries' ms a day |
| --- | --- | --- | --- |
| 1780 | 20.4 | 1.6 | 0.3 |
| 1781 | 23.1 | 24.6 | 1.7 |
| 1782 | 23.7 | 45.9 | 6.2 |
| 1783 | 26.1 | 48.0 | 15.6 |
| 1784 | 29.3 | 70.2 | 18.5 |
| 1785 | 26.7 | 23.6 | 7.8 |
| 1786 | 31.1 | 26.3 | 21.7 |
| 1787 | 34.5 | 43.2 | 33.6 |
| 1788 | 30.5 | 46.0 | 32.6 |
| 1789 | 27.8 | 26.6 | 6.9 |
| 1790 | 26.4 | 42.2 | 23.4 |

- 79% of the checks asked no route at all (no friendly unit close enough); one asked 31. They are cheap at peace and costly in the wars of a great power, whose armies stand near enemies and near each other: one route query took 0.6 ms on average under the trace, and the queries from all callers came to 114 a day.
- Of the first 27,456 checks (1780 to 1784; the trace logs the answer of the first 30,000), **26,327 answered yes**, with 36,150 queries, 72% of those checks' query time; 1,129 answered no, with 6,791 queries (6 a check).
- **By loop**, a third run ([the support check's queries by loop](../prototype-log/2026-11-08-support-check-queries-by-loop.md)) logged where each of the first 60,000 route queries was asked from (1780.7 to 1783.3), the check's three call sites telling its loops apart. Of the time of the queries inside 19,918 checks:

  | The check's answer | Its own units | Its war allies' | The enemies' |
  | --- | --- | --- | --- |
  | Yes (19,487 checks) | 31.9% | 9.1% | 50.1% |
  | No (431 checks) | 0.3% | 0.4% | 8.1% |

  The friends could reach the province on nearly every query (14,846 of 15,043 in yes checks); the enemies on 5,443 of 7,386.

## What this means for making the game faster

1. **The support check, cut short with the same answer**, two ways, of the check's queries, 5.0 ms of the wait a day: the enemies' loop stopping once the friends can no longer pass, at most the enemies' queries of the checks that answer no, 8%, about 0.4 ms; or the enemies counted first and the friends' loops stopping once they have passed, at most the friends' queries of the checks that answer yes, 41%, under 2 ms, less the queries asked before the bar is passed, which were not counted. The second needs the whole check worked by the framework in another order, every term, filter and floating-point step as the game's, checked against the game's answer: a rewrite of a function of 4 KB for under 3% of a day. Not worth a prototype under the bar of a certain gain.
2. **The transports' route queries** (`0xf1390`), 1.7 ms of the wait: what they ask was not read.
3. **The main army's decision** (`0xeab50`), 3.4 ms of the wait with S9: the search that S9 does not take ahead (a think that did not prepare one, or one under way when its think came) and the decisions around it.
4. **A country's agents run apart**, bounded by the largest single agent's day: unchanged from [late-game.md](late-game.md) ("Whether it can be made faster"), a change to what the AI decides.

So the AI's wait has no way left both exact and worth its cost: the support check's are bounded under 3% of a day, the transports' and the main army's are not read, and running agents apart changes the AI.

## Not verified

- That `country+0x1b88` is the country's enemies in its wars (its third loop's list): inferred from its sum being the bar; what `+0xb1c` and `+0xb1e` make the agent do; the define at `[0x233fc7c]`.
- That every strength added is at least 0: read from the scaling, not followed into `0xd46800`.
- That a route query writes nothing outside its own lists: from [routes.md](routes.md), which lists what it writes, not from a watch of it.
- Where in a check the friends' sum passes the bar.
- The answers of checks after 1784, and of the years of the ten that the trace did not log.
- One save, in observe mode; `AUH` is a tag of that game, not one the game starts with.
