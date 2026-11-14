# Performance

`scenario perf` now collects completed days, frame submission intervals and interface updates in bounded in-process windows on a dedicated performance developer build. [Performance windows](dev-tools.md#performance-windows) explains its output and collection-off comparisons; the [metric plan](plans/performance-metrics.md) also defines action response, which remains unmeasured. The historical results below retain their original methods: their date-based rates and frame counts are not retrospectively redefined as the new measurements.

## Completed days, submissions and updates (2026-11-14)

The [first complete run](prototype-log/2026-11-14-performance-baseline.md) used the performance developer build, a hidden new Ming campaign with the player's playset (including Regional Combat and Convenience Tools), 3840x2160, and drawing on its own thread with no framework frame limit. After paused warmup, it collected 30 s paused and 120 s at speed 5. Both windows were complete, with no lost events; neither is an uninstrumented player-build measurement.

| Measure | Paused | Speed 5 |
| --- | --- | --- |
| Completed days/min | 0 | 1,015.5 (2,031 days) |
| Successful submissions/s | 144.033 | 128.825 |
| Submission interval P95 / P99 / max, ms | 7.329 / 7.967 / 17.215 | 10.019 / 38.034 / 150.687 |
| Completed submission gaps over 50 / 100 ms | 0 / 0 | 78 / 14 |
| Interface updates ready/s | 72.067 | 24.525 |
| Update-ready interval P95 / P99, ms | 14.325 / 14.971 | 62.939 / 76.513 |
| Ready-to-first-submission P95 / P99, ms | 7.190 / 7.938 | 9.065 / 20.056 |
| Time-weighted interface age P95 / P99 / max, ms | 20.055 / 20.690 / 30.851 | 62.138 / 90.098 / 163.525 |
| CPU, cores | 0.656 | 3.494 |

The running window contained 15,459 submissions and 2,943 prepared updates, 749 of them inside daily calls. Drawing could therefore remain frequent while reusing the same update. Eleven marked local saves remain included. There were only six year starts, too few to treat their P95 as stable. The run record gives counts, update outcomes, missing age coverage and unfinished intervals; the JSON and Markdown reports are in `target/scenarios/perf-baseline/`.

[The main-thread check](prototype-log/2026-11-14-performance-native.md) also collected complete windows with `drawing-thread` and `background-compression` disabled, all successful submissions attributed to the main thread, and both local saves marked. It establishes path coverage, not a controlled speed comparison. Physical display latency, GPU time, pure interface-work duration and action response remain unmeasured.

### Investigating submission-interval P95

The [2026-11-14 investigation](research/frame-drawing-p95.md) made another [default run](prototype-log/2026-11-14-frame-p95-baseline.md), a [paused camera comparison](prototype-log/2026-11-14-frame-p95-camera.md), and a [repeat after concurrent analysis stopped](prototype-log/2026-11-14-frame-p95-repeat.md). The first two overlapped Ghidra analysis; another game was present during the last. All collected windows were complete with no dropped events or failed submissions, but none establishes an isolated optimization effect. No runtime code was changed.

The two new speed-5 windows had submission-interval P95 of 9.911 and 10.028 ms, P99 of 31.249 and 35.510 ms, and maxima of 168.426 and 182.359 ms. Their maximum Present-call durations were only 29.390 and 40.450 ms, so the longest intervals include substantial time outside the ending Present call. Drawing logs point toward invalidated frames and presentation-lock waits, especially around message popups, but use a different endpoint and a roughly 13.89 ms threshold; they do not attribute the 8–10 ms region determining P95.

In the final paused comparison, fixed-camera P95 was 7.333 and 7.533 ms and moving-camera P95 was 7.721 and 7.518 ms. One moving window also had Present disturbances, a 20.563 ms P99 and older interface updates despite more submissions per second. The warmed route did not establish a large stable camera-work penalty; these observations cannot rule out other views or late-game behavior. The research note ranks shorter lock occupancy and invalidation, repeated preparation work and narrower drawing helpers, with the evidence still required before changing their behavior. It claims no expected percentage gain.

### Why submission-interval P99 is long

The [individual-interval investigation](research/frame-drawing-p99.md) recorded four complete speed-5 windows, with temporary stage probes and then lock-entry stacks and an internal-scope return marker. P99 was 39.605, 55.457, 38.583 and 30.872 ms; these are different windows and world progress, with differing external load, not an optimization comparison. The first three lasted 120 seconds each; the final scope check lasted 60 seconds.

In the third window's P99 neighborhood (147 intervals, ranks 98.5–99.5%), presentation-lock waits occupied 61.82% of elapsed time, invalidation outside those waits 26.32%, and map names due 5.40%. Pre-Present drawing work was 4.86% and Present 1.58%. The first two windows independently assigned 86.76% and 88.37% to lock waits plus invalidation. No marked save overlapped these neighborhoods, although saves remain in the extreme tail.

The dominant lock-order trigger was a stream-related internal lock entered during AI-country work. The final run paired 676 scopes: the internal getter returned a median 14 microseconds after the presentation lock was acquired, but that presentation lock remained held for a further median 12.522 ms (P99 34.270 ms). An exact submission-P99 sample retained it for 30.268 ms after the getter returned. The framework keeps lock-order ownership until a later frame check, so it can block drawing through unrelated remaining AI work. A separate popup example establishes that frame invalidation can likewise persist long after the popup returns.

The first optimization target is therefore the lifetime of ownership acquired only for lock ordering, while preserving genuine crossing protection and avoiding deadlock. All attribution-window counts and P99 values match the formal reports; all event/probe buffers were complete. The diagnostic probes were removed and the core rebuilt.

The subsequent [bounded ownership experiment](research/frame-drawing-p99.md#bounded-ownership-experiment) tested one native scope in A/B/B/A order. Baseline submission P99 was 38.402 and 39.277 ms; prototype P99 was 37.767 and 39.379 ms. Paired changes were -1.65% and +0.26%, with completed-day rates +0.63% and +1.79%. P95, submission rate and interface age did not improve consistently. All eight paused/running windows were complete without lost events or failed submissions, and all four games released without a crash. This does not demonstrate a useful optimization; the prototype was removed and its patch retained.

The view retained an open introduction window over mostly unexplored map, and the second prototype's paused control had a Present disturbance (submission P99 20.870 ms versus 7.646–7.904 ms in the other controls). This is not validation of normal map play or late-game behavior. The ordinary reports do not count eligible early releases or attribute the prototype's remaining long intervals, so the experiment cannot explain why shortening this scope failed to produce a repeatable gain. The research note and four separate comparison run records retain the results and those limits.

The [release-path follow-up](research/frame-drawing-p99-release-path.md) directly observed 1,268 and 1,060 early releases in the original and Beijing views. The scoped hold fell to median 15 and 16 microseconds, but a subsequent helper in the same stream initialization took the same game lock and restored long presentation-lock ownership. Consecutive acquisitions on the dominant AI chain had a median one-microsecond gap; the second hold lasted median 12.541 and 16.483 ms. Lock waits still occupied 68.42% and 71.27% of the prototype P99 neighborhoods. The narrow prototype releases correctly but leaves the next acquisition's lifetime unchanged.

Diagnostic P99 was 31.259 ms baseline versus 37.543 ms prototype in the original view, and 44.727 versus 40.216 ms in Beijing. These are not isolated optimization gains: the normal-view pair has another session's game in surrounding status records, acquisition counts change probe cost, and campaign progress differs. All eight windows were complete, and the four owned games released without a crash. The temporary source and artifacts were removed; only research evidence remains.

The [safe-boundary investigation](research/frame-drawing-p99-safe-boundary.md) read the successive stream helpers and the complete captured formatting path. Four independently checked normal-return scopes, with cold paths and abnormal completion retaining protection, removed the dominant chain's long ownership. A complete diagnostic repeat recorded 41,197 eligible early releases; the fourth helper's hold P99 was 4 microseconds. An earlier four-scope trace overflowed and remains explicitly incomplete.

Final A/B/B/A timing without temporary stage probes measured baseline P99 of 44.338 and 43.929 ms versus prototype 13.166 and 13.248 ms: paired reductions of 70.31% and 69.84%. P95 fell from 14.350/15.206 to 8.137/7.805 ms. Completed-day rates changed by -17.87% and -0.34%; the first prototype also had higher system CPU load, so the daily-rate cost is not isolated. Interface age and maximum submission intervals did not improve consistently. All eight formal windows completed with no lost events or failed submissions, and all four games released without a crash. Other games were permitted; none appeared in these collection windows' five-second process samples. The result supports a substantial P99 improvement in this early-game Beijing scene, not universal cost or all-path safety. The runtime prototype was removed after retaining its source and evidence; no production change was installed.

The checked four-scope behavior is now implemented in `core/src/drawing/lock_order.rs`, using the same native boundaries and conservative retention rules as the measured timing candidate. The measurements above retain their scene and load limitations.

### Collection on/off comparison

Six further runs used that same compiled core (all retained startup reports agreed on its size and modification timestamp), playset, resolution and initial camera. Each restarted a hidden new Ming game with the same warmup, then ran 5 s paused and 30 s at speed 5. The order was on/off, off/on, on/off. All passed; all collection-on windows reached their deadlines with no lost events, and collection-off reports correctly left the in-process metrics absent. No other game was running at any start, and all test games were released.

| Run, in execution order | Collection | External date-days/min | Paused CPU, cores | Speed-5 CPU, cores |
| --- | --- | --- | --- | --- |
| [Pair 1 on](prototype-log/2026-11-14-performance-overhead-1-on.md) | On | 975.7 | 0.672 | 3.404 |
| [Pair 1 off](prototype-log/2026-11-14-performance-overhead-1-off.md) | Off | 1,116.9 | 0.725 | 3.598 |
| [Pair 2 off](prototype-log/2026-11-14-performance-overhead-2-off.md) | Off | 1,014.8 | 0.731 | 3.378 |
| [Pair 2 on](prototype-log/2026-11-14-performance-overhead-2-on.md) | On | 1,056.8 | 0.742 | 3.629 |
| [Pair 3 on](prototype-log/2026-11-14-performance-overhead-3-on.md) | On | 1,061.2 | 0.735 | 3.625 |
| [Pair 3 off](prototype-log/2026-11-14-performance-overhead-3-off.md) | Off | 1,071.0 | 0.644 | 3.249 |

| Measure | Collection on: median (range) | Collection off: median (range) |
| --- | --- | --- |
| External date-days/min | 1,056.8 (975.7–1,061.2) | 1,071.0 (1,014.8–1,116.9) |
| Paused CPU, cores | 0.735 (0.672–0.742) | 0.725 (0.644–0.731) |
| Speed-5 CPU, cores | 3.625 (3.404–3.629) | 3.378 (3.249–3.598) |

The collection-on date-rate median was 1.3% lower, but the individual paired differences were −12.6%, +4.1% and −0.9%. The speed-5 CPU median was 0.247 cores (7.3%) higher with collection on. These short independent campaigns have overlapping ranges and materially different outcomes; they do not isolate a stable collection cost or establish that it is negligible. No randomness seed or saved starting state was fixed, and background machine load was not controlled. The on path also begins and exports a window: the external date interval includes that request and export time before pausing, while the CPU bracket ends before export. These results include those control costs and cannot identify the recorder's hot-path cost alone.

Only the external date-based rate is compared, because collection-off runs have no completed-day observations. This is the cost of enabling collection in one developer build, not the fixed cost of that build's observers versus a player build and not a framework-versus-unmodified-game comparison. The bounded measurements and explicit missing values are verified; a precise overhead estimate remains unresolved.

## Original measurement (2026-10-28)

What the framework costs the game, measured on 2026-10-28 with the framework at commit 44b89f3. In short:

- **Running the game**: at speed 5 the game advanced about 2.5% fewer days per second with the framework, on as much CPU: with the English playset, each of three runs with the framework was behind each of three runs without it (17.8 against 18.2 days per second). Where the time goes was not looked into.
- **Starting the game**: about 1 s longer with the English playset. With the Chinese playset, system fonts took about 3.3 s longer than bitmap fonts (the default), because every character is drawn at startup.
- **Memory**: 30 to 65 MB more with the English playset. With the Chinese playset, bitmap fonts took about **410 MB more** than system fonts, in the menu and in the game. That is the largest cost, and the first to reduce: most of it is what Direct3D keeps after the framework writes its grown font pages, the rest the glyph records each font keeps a copy of.

## Since then

Two changes the same day, both found by the measurements under [Memory](#memory):

1. The grown font pages are written through a small system-memory texture instead of being locked in place.
2. Fonts that bring the same glyphs from their `.fnt` files share one set of them, stored inline, instead of each keeping its own copy of every record.

Measured from the framework's log, Chinese playset, to the main menu:

| | Bitmap fonts: before | after 1 | after 2 | System fonts: before | after 1 | after 2 |
| --- | --- | --- | --- | --- | --- | --- |
| Process private bytes at the main menu | 4353 MB | 3866 MB | 3761 MB, 3763 MB | 3860 MB | 3778 MB | 3767 MB, 3746 MB |
| The framework's heap at the main menu | 66 MB | 66 MB | 3.2 MB | 2.8 MB | 2.8 MB | 1.3 MB |
| Writing the six pages | 560 MB | 40 MB | 38 MB | not measured | 9 MB | 9 MB |
| Drawing a glyph into a page | 2.1 ms | 91 µs | 58 µs | 1.1 ms | 9 µs | 9 µs |
| Making the six pages | 181 ms | 27 ms | 27 ms | 84 ms | 23 ms | 22 ms |
| From the framework loading to the main menu | 30.8 s | 31.5 s | 31.2 s | 34.9 s | 30.8 s | 30.6 s, 30.5 s |

- **Memory**: bitmap fonts now hold about 590 MB less than before, and as much as system fonts. The 994,216 glyph records the 46 fonts brought are kept as 6 shared sets of 115,053.
- **Starting the game**: system fonts no longer take longer than bitmap fonts. Their 3,143 glyphs were drawn in 28 ms instead of 3.5 s; the time had been the locks.
- **System fonts at an interface scale above 1** (2026-10-30): glyphs are drawn that many times larger, on pages that many times wider and taller, so a page holds as many. At 1.5 the five pages of Microsoft YaHei are 1536 pixels square instead of 1024, and creating each added 9 MB of private bytes instead of 4, about 25 MB in all, as the framework's log counts it; the process at the main menu was not measured again.
- **Bitmap fonts are gone** (2026-11-02, [ADR game-text-in-system-fonts](adr/2026-11-02-game-text-in-system-fonts.md)): every font is drawn from system fonts, which the measurements above put level with bitmap fonts in memory and in the time to start.
- **Pages that start small and grow** (2026-11-08, [ADR font-pages-grow](adr/2026-11-08-font-pages-grow.md)): with the player's Chinese playset at 1.5, six pages of body text, headings and the console start 768 pixels square instead of 1536 and the map font's 2560 instead of 4864. At the main menu two had grown, one body text page to 768x1536 and the map font's to 2560x5120, and starting a game as Ming grew a heading page to 768x1536: about 71 MB of page textures in all, against about 150 MB at the sizes before, as worked out from the sizes. The framework's log counted 2.2 to 3.7 MB of private bytes for creating each of the first pages and 50 MB for the map font's growing, against 9 MB for each 1536 page before; the process at the main menu was not measured against a build from before. Two pages growing took 0.4 to 13 ms together, and building the names on the map again 9 to 22 ms ([the run](prototype-log/2026-11-08-font-pages-grow.md)).
- The tables below are from before the changes. Game speed was not measured again.

## The game speed plan (2026-11-04)

[game-speed.md](plans/game-speed.md)'s S1 to S4, built on the branch of that date: Austria-1 (an ironman save of 1618.9.2, the player's playset), 3840x2160, vsync at 144 Hz; frames counted at the application's frame count and days at the date, 30 to 32 s each, no sampler. Before is the same save the same morning with the framework of that time ([Game speed S1 to S4](prototype-log/2026-11-04-game-speed-s1-s4.md)).

| | Before | After |
| --- | --- | --- |
| Paused: frames a second | 63 | 83.8 |
| Speed 5, the game's pace, 1618.9 to 1620.4: days a second (frames a second) | 12.5 (18.2) | 13.8 (19.3) |
| Speed 5, 1620 to 1622: "Faster time" against the game's pace | | 15.3 (15.4) against 13.9 (19.2) |
| ICU's line breaking at speed 5, share of the main thread | 6% (1624) | 0% |
| POPS on the main thread at speed 5 | 5% (1624) | 0% |
| An ironman autosave: compressing, share of the save's samples | 39% | 23% (copying at level 0) |

- Two more pairs of the paces, later the same run, came out at 11.0 and 6.4 days a second, both paces alike; the machine was busy with other work then (a sample took 2.2 ms against 1.4), and they are left out.
- The autosave's main thread time, from the shares: about a sixth less. The thread then compressed 61 MB in 1.6 to 2.0 s, to 7.1 to 7.2 MB, smaller than the game's 8.8 MB.

Later in a game (the same day, the build of master at `c6bda5f`): Austria-37_4-1, an ironman save of 1724, the same machine and playset, the game resumed within 0.3 s whenever a message paused it ([Game speed late in a game](prototype-log/2026-11-04-game-speed-late-game.md)):

| | The game's pace | "Faster time" |
| --- | --- | --- |
| Paused, 1724: frames a second | 44.6, 49.7 | |
| Speed 5, 1730 to 1803, eight blocks of 60 s each way: days a second (frames a second) | 16.5 (20.0 to 24.2) | 19.1 (16.9 to 21.6) |

- "Faster time" was 16% faster there, more than the 10% of 1620. Earlier runs of the same save, resumed only every 2 s, came out the other way round, which was not explained; they are left out.
- The thread compressed each ironman save of 55 MB to 6.1 MB in 1.5 s; the game without the framework wrote the same game at 7.75 MB.

## A save's stop (2026-11-05)

[game-speed.md](plans/game-speed.md)'s S8 on its branch: Austria-37_4-1 (an ironman save of 1724, the player's playset), the same machine, loaded three times with `menu.load`, which saves it as it is loaded and as it is left; the stop is the framework's log line from the start of the local save to its end, five saves each way, in one developer build with the test switches, `game-writes-saves` on for "before" ([A save's countries and provinces on threads](prototype-log/2026-11-05-save-countries-provinces-threads.md)).

| | The game's code | The framework's |
| --- | --- | --- |
| A local ironman save of 55 MB stops the game, ms | 986, 1116, 1233, 1256, 1362 | 308, 431, 437, 438, 447 |
| Of that, the provinces and the countries (eight threads), ms | | 80 to 125 |
| Of that, the zip's three entries, ms | about 250 | 45 |

- What was left of the 310 ms save: the game state's other sections 66 ms, its save checksum 44, the zip 45, its bytes into the file 25, and 41 that was the wait for the game's update thread ([research/saves.md](research/saves.md), "Where a save's zip goes").
- The pool of seven threads waits from startup and costs nothing between saves; while a save is written, the runs' buffers hold the two loops' bytes once more, about 60 MB in that save.

Then the game state's other sections written while the pool writes the loops, the save checksum's sum and the zip's CRC on the pool, and the update thread woken (S8, the same branch). The same save and way, measured both ways back to back while other builds on the machine held its processor at 100% the whole time, so the numbers are of that machine and not comparable with the ones above. The update thread was woken in both columns, `game-writes-saves` not yet turning it off; the wait for it with the game's code is from traced saves of the same runs before it was built:

| | The game's code | The framework's |
| --- | --- | --- |
| A local ironman save of 55 MB stops the game, ms | 1149, 1195, 1211, 1219, 1312 | 284, 300, 301, 302 |
| Of that, the save checksum of the game state, ms | 60 to 65 | 11 to 29 |
| Of that, the wait for the update thread after the zip, ms | 40 to 96 | 0 to 2 |

- The main thread waited 40 to 80 ms for the pool where the world has written its game state; the pool's threads took 170 to 190 ms for both loops under that load.
- A plain save of 1444 (34 MB) stopped the game for 149 ms.

## The startup and a load (2026-11-05)

[loading.md](plans/loading.md)'s L1 to L3, one developer build with the test switches, `game-loads` on for the game's own startup and loads; the player's playset, every DLC, the files in the system's cache; other builds held the machine's processor at 67 to 94% ([The startup and a load sampled](prototype-log/2026-11-05-startup-load-sampled.md)).

| | The game's | The framework's |
| --- | --- | --- |
| From the start to the main menu, s | 82, 82 | 68, 71 |
| A load of an ironman save of 55 MB from the main menu: the save read and the world set up (`0x5cf740`), s | 6.2, 7.3 | 5.1, 4.7 |
| The same load with what follows it (`0xd84b80`), s | 8.1, 8.7 | 6.7, 6.2 |

- The first start after the framework is installed reads the music as the game does; the numbers above are of starts after it.
- What is left is mostly the databases read on the LoadingThread, about 30 s of the startup on this machine ([research/loading.md](research/loading.md)).

## Parameterized script in one pass (2026-11-11)

[game-speed.md](plans/game-speed.md)'s S12, one developer build with the test switches, `game-paths = ["script-parameters"]` for the game's own code; every DLC, the files in the system's cache, no other test game running ([the run](prototype-log/2026-11-11-script-parameters-in-one-pass.md)). The uses' making is `0xdb0860`'s calls summed, timed by the call trace; the main menu is `cargo xtask game start`'s whole seconds.

| | The game's | The framework's |
| --- | --- | --- |
| The player's playset: the uses' making (320,905 uses), s | 1.25, 1.27, 1.23 | 0.25, 0.24, 0.24 |
| The player's playset: from the start to the main menu, s | 17, 17, 17 | 16, 16, 16 |
| With Voltaire's Nightmare: the uses' making (114,886), s; the main menu, s | 1.44; 23 | 0.11; 21 |
| With the 2000 AD mod: the uses' making (1,129,203), s; the main menu, s | 4.03; 90 | 0.98; 83 |

## Localisation and the triggers' optimization profile (2026-11-14)

[large-mod-startup.md](plans/large-mod-startup.md)'s M1 to M4, one developer build with the test switches, `game-paths = ["localisation", "trigger-profile"]` for the game's own code; every DLC, music cached, the files in the system's cache, no other game running ([the run](prototype-log/2026-11-14-large-mod-startup.md)). Seconds from the framework's log (`core.log`):

| | The game's | The framework's |
| --- | --- | --- |
| With Guixu (`ugc_3047072888`, 333 `localisation/replace` files): `main` up to the engine's Init, s | 11.78, 11.76, 11.70 | 2.48, 2.38, 2.44 |
| With Guixu: the rest of `InitGame` after the flags, s | 3.57, 3.69, 3.72 | 2.39, 2.43, 2.42 |
| With Guixu: from the bootstrap to the main menu, s | 30.52, 30.55, 32.38 | 20.55, 20.16, 20.02 |
| The player's playset: `main` to `Init`, s; rest of `InitGame`, s; main menu, s | 2.23, 2.18; 3.08, 2.95; 18.24, 17.91 | 1.91, 1.79; 2.01, 1.97; 16.88, 16.45 |
| With the 2000 AD mod: `main` to `Init`, s; rest of `InitGame`, s; main menu, s | 5.54; 10.16; 78.08 | 2.24; 8.46; 75.61 |

- On the player build (`cargo xtask scenario perf`, the player's playset, music not cached yet in the scenario's fresh user folder): 26.5 s to the main menu (10 s of it reading the music on the first start), 2410 MB working set / 3721 MB private at the main menu, 4.3 s for `menu.start MNG`, 0.647 cores paused, and 1047.8 game days a minute at speed 5 on 3.412 cores.

## The events (2026-11-05)

What the events of [player-actions.md](plans/player-actions.md) cost the game: `building_finished`, `battle`, `war_declared`, `peace_made`, `province_owner_changed`, `month` and `year`. `cargo xtask scenario perf` on the player build of `claude/player-actions-fu-cost-view-4c4534`: a new game as Ming, 120 s at speed 5 from 1444.11.11, about 5.4 years; with `--perf.events` naming the seven kinds, a remote client reads events of those kinds before the game runs, so they are worked out from then on, and counts what was heard at the end. The framework counts the time each piece of that work takes and logs it when the session ends (`event costs:`, since `signal costs:` in [`core/src/game/signal_costs.rs`](../core/src/game/signal_costs.rs), the events now being signals). The machine was the one under [Machine](#machine), busy with other sessions' builds.

| Run | Listening | Game days a minute | CPU at speed 5 (cores) |
| --- | --- | --- | --- |
| 1 | none | 855.0 | 2.55 |
| 2 | the seven, and the reads below after the run | 964.3 | 2.67 |
| 3 | the seven, owners read alone (below) | 987.1 | 2.78 |
| 4 | none | 996.5 | 2.85 |

The pace spread by 16% between the two runs that listened to nothing, so it cannot show the events. The framework's own timing can, on the game's main thread:

| Work | When | Run 2 | Run 3 |
| --- | --- | --- | --- |
| The wars read and compared with the day before (`war_declared`, `peace_made`) | each new day, 1929 and 1975 | 22.7 µs a day | 19.0 µs a day |
| The provinces' owners read and compared (`province_owner_changed`) | each new day | 84.2 µs a day | 54.8 µs a day |
| A building finished read (`building_finished`) | each one finished, 19 and 24 | 1.6 µs each | 1.1 µs each |
| A combat's results read (`battle`) | each combat's results, sieges' too, 378 and 346 | 1.1 µs each | 1.1 µs each |
| What a frame heard made into remote control's events | every frame, listening or not | 1.0 µs a frame | 1.0 µs a frame |

- `month` and `year` compare the new day's date with the day before, which is done for every new day whether anyone listens or not; nothing is read for them.
- **In all**, the seven cost about 74 µs a game day: at the 16 days a second of these runs, 1.2 ms of the main thread a second, about 0.1%, far below the spread between runs. Heard in run 3: 97 provinces changing owner, 52 wars declared, 46 peaces made, 18 battles, 24 buildings finished, 65 months and 6 years.
- The owners were read with the rest of each province's record (its fog of war, its kind) until run 2; reading only the owner took the watch from 84 to 55 µs a day.
- **Reads**: after run 2, the scenario read `country.get` and `country.budget` of each of the 170 countries that own a province and `province.get` of each of the 777 provinces the player had discovered that a country owns. On the main thread, as the framework's log times each request, `country.get` took 0.09 ms on average, `country.budget` 0.04 ms and `province.get` 0.01 ms (`province.list`, every province, 2.4 ms). From the client each took 23 to 25 ms, which is the wait for the game's next frame, not work.
- **Not measured**: an extension's handlers (their Lua is the extension's own; the work above is the same for them), a late game with more wars, and the pace with the events on and off over enough runs to see 0.1%.

## Setups

Two playsets:

- **Chinese**, the one the project's author plays: the Chinese language mod, a mod that sorts and colors modifiers, Graphical Map Improvements, Emoji Universalis, and the regional combat mod (inert without the framework).
- **English**: the same without the Chinese language mod and the modifier mod.

| Setup | Playset | In the game folder |
| --- | --- | --- |
| Framework, bitmap fonts | Chinese | the framework; framework settings "Keep the fonts as they are" (the default) |
| Framework, system fonts | Chinese | the framework; "System fonts", Microsoft YaHei for text, KaiTi for the map |
| Vanilla | English | no framework |
| Framework | English | the framework, bitmap fonts |

The game ran in English with Windows in Chinese, so the framework's player language was Simplified Chinese throughout.

## Machine

AMD Ryzen 7 9700X (8 cores, 16 threads), 64 GB of memory, GeForce RTX 4070 Ti SUPER (16 GB), Windows 11 Pro, 3840×2160. EU4 1.37.5 from Steam, started directly as `eu4.exe`.

A training job that kept the processor about 80% busy and held 14 GB of the graphics card's memory was stopped before the runs below. One run made while it ran is left out: framework with system fonts, 51 s to the main menu and 11 game days per second, against 37 s and 17.4 days idle.

## Method

The runs below were made with `tools/perf-run.ps1`, which measured vanilla too and so drove the game by the screen. It ran the game the same way each time:

1. Starts `eu4.exe` and times the launch until the main menu is on screen (a region of the menu's logo, compared every 0.25 s).
2. 20 s later reads the process's working set and private bytes.
3. Opens single player, picks the 1444 East Asia bookmark and Ming, starts in normal mode, and times the load until the start-of-game introduction appears.
4. Closes the introduction, waits 10 s, and measures CPU over 30 s paused, on the map as the game opens it.
5. Sets speed 5, runs the game for 120 s, pauses, and reads the CPU, memory and date.

CPU is given in cores: the process's CPU seconds per second of wall time (the machine has 16 hardware threads). Memory is what Windows reports for the process; graphics memory was not measured. Each setup was measured two or three times, going through the setups and then back again, with menu-only runs for idle CPU at the main menu (Chinese playset). Game days are counted in the game's calendar, which has no leap years.

The scenario `perf` ([xtask/src/scenario/perf.rs](../xtask/src/scenario/perf.rs), `cargo xtask scenario perf`, [scenarios.md](plans/scenarios.md)) measures the framework's game only, on the player build (with the test games' pipes, which name the pipe and nothing else): in its test game ([test-games.md](plans/test-games.md)) it starts the game hidden and times the start to the main menu, reads memory 20 s later, starts a new game as Ming with `menu.start` and times it, measures CPU over 30 s paused, then 120 s at speed 5 through `time.set_speed`, `time.resume` and `time.pause`, and reads the dates with `time.get`, from which come the game days a minute. Its start is timed from the start command, not from a menu logo on screen, so its menu seconds are not those of the runs below. With `signals` a remote client listens to those kinds of signals while the game runs, with `reads` it times `country.get`, `country.budget` and `province.get` afterwards, and it ends at the main menu, where the framework logs what working out the signals cost ([The events](#the-events-2026-11-05)).

The figures on this page come from `tools/perf-run.ps1` (since removed), from the framework's log and from `cargo xtask profile`; none yet from the scenario. Its first runs, on 2026-11-05 ([Scenarios](prototype-log/2026-11-05-scenarios.md)), checked that it plays to its end: 47.2 s to the main menu, 9.7 s for `menu.start MNG`, 0.81 cores paused and 1.75 at speed 5, and 487 game days a minute, with 305.6 and 453.3 in two later runs, a spread larger than the framework's setups are likely to differ by. The next measurement of what the framework costs is taken with `cargo xtask scenario perf` and recorded here.

## Results

Means, with the single runs in brackets where there were several.

### Chinese playset

| | Framework, bitmap fonts | Framework, system fonts |
| --- | --- | --- |
| Launch to main menu (s) | 33.5 (33.1, 33.1, 34.2) | 36.8 (37.4, 36.3, 36.8) |
| Main menu: working set / private (MB) | 2856 / 4194 | 2466 / 3787 |
| Main menu, idle: CPU (cores) | 0.56 | 0.56 |
| Loading a new game (s) | 3.5 (3.4, 3.5) | 3.7 (3.7, 3.7) |
| Paused in game: CPU (cores) | 0.99 (1.01, 0.97) | 0.93 (0.88, 0.99) |
| Paused in game: working set / private (MB) | 3073 / 4494 | 2672 / 4077 |
| Speed 5: game days per second | 17.3 (16.7, 18.0) | 17.4 (17.5, 17.4) |
| Speed 5: CPU (cores) | 2.95 (2.96, 2.93) | 2.98 (3.07, 2.90) |
| After 2 min at speed 5: working set / private (MB) | 3164 / 4596 | 2774 / 4185 |

### English playset

| | Vanilla | Framework, bitmap fonts |
| --- | --- | --- |
| Launch to main menu (s) | 30.3 (30.2, 30.5, 30.2) | 31.2 (31.3, 31.2, 31.2) |
| Main menu: working set / private (MB) | 2365 / 3622 | 2401 / 3674 |
| Loading a new game (s) | 3.3 (3.4, 3.2, 3.2) | 3.3 (3.2, 3.2, 3.4) |
| Paused in game: CPU (cores) | 0.93 (0.95, 0.84, 1.00) | 0.98 (1.03, 1.06, 0.86) |
| Paused in game: working set / private (MB) | 2582 / 3907 | 2619 / 3971 |
| Speed 5: game days per second | 18.2 (18.24, 18.22, 18.23) | 17.8 (17.73, 17.96, 17.59) |
| Speed 5: CPU (cores) | 3.02 (2.99, 3.01, 3.05) | 2.97 (2.95, 2.93, 3.03) |
| After 2 min at speed 5: working set / private (MB) | 2672 / 4017 | 2699 / 4082 |

### Running the game

With the English playset, vanilla advanced 2210 to 2212 days in 121.3 s and the framework 2134 to 2179: every framework run was behind every vanilla run, by 2.5% on average. CPU was the same, so with the framework each game day took slightly more of it. Candidates, none measured: the stubs every drawn glyph goes through, the daily events the framework sends the regional combat extension, and the framework UI's work each frame. A profile of a run at speed 5 would tell.

With the Chinese playset, where the runs of one setup spread from 16.7 to 18.0 days per second, the two font looks ran alike.

Paused, the framework used as much CPU as vanilla, within the spread between runs. All runs kept to the map: paused as the game opens and running at speed 5, without opening windows full of text.

### Starting the game

With the English playset the framework added about 0.9 s; every framework run was slower than every vanilla run.

System fonts take about 3.3 s longer than bitmap fonts. The framework's log accounts for it: every character of the text fonts and the map font is drawn at startup, about 3150 glyphs at 1.1 ms each (3.4 to 3.5 s, on the thread that loads the fonts). Bitmap fonts draw only the few characters the mod's fonts lack (7 glyphs, 15 ms).

### Memory

With the English playset the framework took 27 to 37 MB more working set and 52 to 65 MB more private memory, at the menu and in the game. Its bitmap fonts grow the 18 vanilla font pages too, but those are small (256×1280 for most).

With the Chinese playset, bitmap fonts cost about 410 MB more than system fonts, at the menu and in the game alike; the difference does not grow while the game runs.

With bitmap fonts, each of the Chinese language mod's six font pages is grown by 1024 rows for the characters the font lacks (log of one run):

| Font | Grown page (DXT5) | Size |
| --- | --- | --- |
| Microsoft YaHei, 12.1 px | 2048×4864 | 10 MB |
| Microsoft YaHei, 10.6 px | 2048×3584 | 7 MB |
| KaiTi, 88 px (map) | 6400×8688 | 56 MB |
| Microsoft YaHei, 13.6 px | 2048×5120 | 10 MB |
| Microsoft YaHei, 18.2 px | 3072×5120 | 16 MB |
| Microsoft YaHei, 9.1 px | 2048×2636 | 5 MB |

That is about 105 MB of pages, and the 1024 rows reserved below the mod's glyphs are only 18 MB of it. Measuring the process's private bytes around each step of loading the fonts (logged since, see `core/src/heap.rs`) found where the difference goes, against a run with system fonts to the same main menu (4353 against 3860 MB):

| What | Bitmap fonts | System fonts | Difference |
| --- | --- | --- | --- |
| Loading the map font, page included | +539 MB | +253 MB | +286 MB |
| Loading the five body font pages | +213 MB | +58 MB | +155 MB |
| The framework's heap at the menu | 66 MB | 3 MB | +63 MB |

- **The pages**: creating each grown page as a dynamic texture commits about its own size, but writing it, 256 rows at a time with a lock each, commits several times its size and keeps it: 425 MB for the 56 MB map page alone (details in [research/font-textures.md](research/font-textures.md)). Writing a page with fewer locks, or keeping it in video memory and copying glyphs in from a small system-memory texture, should remove most of it.
- **The heap**: 46 font objects load, most of them from the same few `.fnt` files, and each keeps its own copy of the 21,443 glyphs above 0xFF: 994,216 records, each allocated on its own. System fonts drop them. Sharing one table per file, with the records stored inline, would leave a few MB.

It is the default setting, so it is the first thing to reduce.

### The framework's own costs

From the framework's log, at the main menu of each run:

| | Chinese, bitmap fonts | Chinese, system fonts | English, bitmap fonts |
| --- | --- | --- | --- |
| Line break questions | 85,000 to 86,000 | 72,700 to 73,100 | 412 |
| Texts whose breaks ICU worked out | 24,860, in 98 to 103 ms (4 µs each) | 20,220, in 95 to 100 ms (5 µs each) | 378, in 7 ms |
| Glyphs drawn | 7, in 15 ms | 3,140 to 3,160, in 3.4 to 3.5 s | 1, in 0.6 ms |
| Font pages made | 6, in 181 to 186 ms | 6, in 83 to 84 ms | 18, in 27 ms |

The regional combat mod's extension was called once by the time of the report, for 1 ms. The framework logs these costs again every 10 minutes when they change; the runs were shorter, so costs during play were not recorded.

## Not covered

- Where the 2.5% at speed 5 goes. Since measured in part: the framework's line breaking was 1.8% of the main thread at speed 5 early in a game and 6% late ([research/main-thread.md](research/main-thread.md)), what the game itself spends its time on is there too, and making the game faster is planned in [game-speed.md](plans/game-speed.md).
- Frame rate and frame times: no tool for them was at hand.
- Graphics memory.
- Interface-heavy play (the ledger, long tooltips, many open windows), late game, and games longer than the six years each run covered.
- Other machines, in particular ones with 8 or 16 GB of memory, where 410 MB matters more; other playsets.
