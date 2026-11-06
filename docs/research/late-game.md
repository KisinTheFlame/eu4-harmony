# Why days are slower later in a game

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Measured on 2026-11-06 with `cargo xtask profile --slice 60`, which writes a line for each minute of a run: its dates, days and frames a second, and the shares of the main thread's parts ([dev-tools.md](../dev-tools.md)). The parts are those of [main-thread.md](main-thread.md); the runs are in [prototype-log.md](../prototype-log.md).

In short: **what grows is the AI's handling of armies, and it grows on the main thread's critical path.** In one game the AI played alone from 1444 to 1821, the day's AI stage went from 5 ms of the main thread a game day to 30 ms, as the world's regiments tripled and gathered in a few great powers (the Ottomans' from 249 to 1,575). The stage runs one task per AI country on the worker threads, and the main thread spins until the last of them is done; timed task by task, a stage lasts exactly as long as its longest task, a great power's, which starts first and still ends last, while the workers sleep most of the time. The rest of the day did not grow: each country's daily update became cheaper as countries were annexed, and a frame cost the same in 1800 as in 1600. At speed 5 the days went from 19.7 a second around 1650 to 12.5 around 1800.

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
- More cores would not help: the stage already leaves most of them idle, and the stage already starts its longest tasks first. What would is the largest countries' tasks taking less time, or their items (`0xe4890`) running apart, and on the worst days the single item that takes most of the day taking less time. Each changes the AI's own code and the order its work is done in: a change of kind C ([ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/making-the-game-faster-leaves-what-it-computes-as-it-is.md)), which needs the bench that repeats ([game-speed.md](../game-speed.md), S5 and S6, set aside).
- A late game the player feels to be slow can also be slow for reasons of the session, not of the date: the windows left open, or (with the game's own pace instead of "Faster time") the frames inside a day.

## Saves to test with

The game's autosaves of the run are kept outside the game's folders, for runs that need a mid or late game, at `%LOCALAPPDATA%\eu4-harmony-dev\saves\` on the machine that made them:

- `observer-1600.1.1.eu4`, observing (`player="O00"`). `menu.load` refuses it ("no country O00"): within a claim, copy it into the save games folder with its header's `player` set to a country (`"TUR"`) and `not_observer=yes`, load that, and observe.
- `observer-1700.1.1.eu4`, written while the Ottomans were viewed, so `player="TUR"`: `menu.load` loads it as the Ottomans; observe, and view them again.

A claim's release removes what a run added to the save games folder, so a copy there goes with it.

## Not verified

- What an item of `0xe4890` is (its argument's object), and what `0xea250` and `0xeab50` do beyond `ai_army.cpp`'s error texts; whether two items of one country read or write what the other does, which decides whether they could run apart at all.
- What makes one item take over 100 ms on some days.
- The tasks timed in one game from 1700 to 1730 only.
- One game only, as the AI played it; a game a player plays, with the player's country out of the AI, and other great powers.
- The game's own pace (frames inside a day) over a long run, and a vanilla game without the framework.
