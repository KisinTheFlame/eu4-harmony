# What the main thread waits for in the AI stage (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-08 as the third step of [game-speed.md](../game-speed.md), "How a part is chosen", for its largest candidate, the AI stage's wait, with S9 (the armies' searches on idle threads) and S10 (sets of modifiers through an index) built. From the sampler and the call trace on test games ([the AI stage's wait, measured](../prototype-log/2026-11-08-ai-stage-wait-measured.md)) and from the game's code. What the AI's agents are and do is in [ai.md](ai.md); how the stage waits, and what it waited for before S9, in [late-game.md](late-game.md).

In short: late in a game the main thread waits **17 ms of a 67 ms day** for the AI stage's last task, a great power's: Austria-Hungary's (`AUH`), the Commonwealth's (`PLC`) or Great Britain's (`GBR`) on four days in five. What runs then is that country's army agents' thinks, and half of it is route queries. The largest single asker of them is **the support check** (`0x106320`): an army near enemies asks whether its friends can reach it in time to outnumber them, with a route query for every friendly army close enough, and adds up those that can. Its answer only says whether the sum passed a bar, and the sum only grows: **once it has passed, the queries left cannot change the answer**, so stopping there gives the same answer with fewer queries. That is bounded by the queries of the checks that answered yes, 72% of the checks' query time, at most about 3.6 ms of the wait a day; how much of it lies after the bar is passed only a prototype can count. The rest of the wait is spread over the army agents' other decisions.

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

`0x106320`(agent, province, army, strength there, share asked, strength to beat, two bytes), eight arguments, answers yes or no (`al`); which strength is whose is _inferred_ from how they are used. An army agent's think (`0xea250`) asks it for its main army where it stands (`0xea430`), when the agent's `+0xb1c` is set, with the strength in that province worked out first (`0xd46800`, into the fourth argument) and a share of 1000; its other units' decisions ask it too (`0xf4490`). A no sets the agent's `+0xb1e` and `+0xb1f` and holds that for 7 thinks (`+0xb20`); a yes counts that down. What the agent does differently while `+0xb1e` is set was not followed; inferred, it keeps away from where it would be outnumbered.

What it works out, in the game's terms: **whether the strength that can reach this province in time outnumbers the enemy's by the share asked.**

1. The sum begins at the strength already in the province and a constant (`[0x1fd3b38]` plus the fourth argument's, `0x1063c7`).
2. It walks the country's units (the list at `country+0xd18`, by the army's owner, `0x106405`..`0x106422`), and then, in two more loops of the same shape (`0x106816`.., `0x106d36`..), two more lists of units; whose (the allies', inferred) was not followed. It leaves out the army itself, units without a valid owner, units its method `+0x88` refuses, those retreating (`+0x203`), of three kinds by their method `+0x50` (5, 11 and 12), and those whose map distance to the province, made a time by a square root (`0x88a390`, `0x1065eb`..`0x106634`), is over the define at `[0x233fc7c]`.
3. For each unit left, a route query (`0x96c520`) from where it stands to the province; for one that can get there, its strength (`0xd46800`, or a cached value when it stands there already) is scaled down by how long it would take, and added (`add r15, r8`, `0x1068aa`).
4. After the three loops: the sum over the strength to beat (the sixth argument), scaled, against the share asked (the fifth), yes when it is larger (`0x107401`..`0x107459`).

Only the answer leaves the function: the routes go into lists of its own, freed after each query ([routes.md](routes.md), "What it writes"), and the cleanup after the loops (`0x10cd80`) gives nothing it uses. Each term added is a strength scaled by a positive factor, so the sum only grows, and the answer turns from no to yes at most once as it does. **So once the sum has passed the bar, every later query is asked for nothing**: a check that stops there answers as the game's does.

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
- Of the first 27,456 checks (1780 to 1784; the trace logs the answer of the first 30,000), **26,327 answered yes**, with 36,150 queries, **72% of those checks' query time**; 1,129 answered no, with 6,791 queries (6 a check), which a no must ask to the end.

## What this means for making the game faster

1. **The support check stopping once the bar is passed**: the same answer, with fewer route queries; bounded by the queries of the checks that answer yes, 72% of the check's queries' time, which in the wait above is at most about 3.6 ms of a 67 ms day, 5%, and more in the years of great wars. How much of it lies after the bar is passed was not counted: a check that passes it on its last query saves nothing. It needs the check's three loops worked by the framework (the function replaced, as S9 and S10 replace theirs), every term as the game's, the bar worked out before the loops instead of after them, and, in a developer build, the game's own check run beside it and the answers compared, as `checked` does for the other speed-ups. Kind C by where it acts, with no difference in what it computes.
2. **The transports' route queries** (`0xf1390`), 1.7 ms of the wait: what they ask was not read.
3. **The main army's decision** (`0xeab50`), 3.4 ms of the wait with S9: the search that S9 does not take ahead (a think that did not prepare one, or one under way when its think came) and the decisions around it.
4. **A country's agents run apart**, bounded by the largest single agent's day: unchanged from [late-game.md](late-game.md) ("Whether it can be made faster"), a change to what the AI decides.

So the AI's wait has one exact way bounded at about 5% late in a game, the support check; the others are smaller or change the AI.

## Not verified

- Whose units the check's second and third loops walk; what `+0xb1c` and `+0xb1e` make the agent do; the define at `[0x233fc7c]`.
- That every strength added is at least 0: read from the scaling, not followed into `0xd46800`.
- That a route query writes nothing outside its own lists: from [routes.md](routes.md), which lists what it writes, not from a watch of it.
- Where in a check the sum passes the bar, and so the gain itself.
- The answers of checks after 1784, and of the years of the ten that the trace did not log.
- One save, in observe mode; `AUH` is a tag of that game, not one the game starts with.
