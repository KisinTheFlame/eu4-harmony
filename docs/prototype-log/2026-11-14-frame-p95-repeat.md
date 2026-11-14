# 2026-11-14 Submission-interval repeat after analysis stopped

This run repeated the [paused camera comparison](2026-11-14-frame-p95-camera.md) and added a 120-second fixed-camera speed-5 window in the same new Ming game. Source was `8d5358f1`, using the identical performance core built for the [first baseline](2026-11-14-frame-p95-baseline.md), with `dev-performance` and `dev-test-games`. The Ghidra import had been closed before startup; no compilation, stack sampling or screenshots ran during collection. The game stayed hidden.

This was **not an exclusive-machine run**. A `cargo xtask game status` observation during measurement found owned process 13628 and another, non-test-game `eu4.exe` process 94376. That other game was left alone; its exact lifetime and resource use were not measured. Each window records whole-system CPU busy time. No absolute performance improvement or clean camera-cost estimate follows from this run.

## Setup and method

The claim was `loving-wiles-p95-clean`; its report folder is `target/scenarios/frame-p95-clean/` (the folder name does not imply an idle machine). Settings copied before release: 3840x2160 borderless, vsync enabled, 8x multisampling, framework drawing unlimited, terrain/trees/shadows/reflections enabled, and the inherited playset. The framework detected 144 Hz; the game settings file's refresh field was 60. These settings were not changed for the probe.

The one-off pipe client used `menu.start MNG`, `time.pause`, ten seconds warmup, then the same closed 320-map-pixel route and two warmup repetitions as the first camera comparison. The initial camera query, immediately after campaign creation, was x=2984, y=1455, height=455. By collection the game's startup camera had settled to height=310. All five measured windows ended at height=310 and y=1455. Fixed windows ended at x=2984 and moving windows at x=2992, the route's final step. The position differs slightly from the other new-game runs; they must not be pooled as identical-view comparisons.

Each paused window lasted exactly 24 in-process seconds. Idle sent no requests; each fixed and moving window sent 240 immediate `camera.look_at` requests on a 100 ms schedule. Fixed repeated the same coordinates; moving advanced 8 pixels per step and returned along the same route. Three seconds separated windows. Each moving measurement covered three complete eight-second route periods. Actual request timestamps and durations were retained; the last request was at 23.906–23.907 seconds and the longest round trip was 47 ms. Remote request duration is not hardware-input latency.

After the last fixed window, the client reset the camera to its starting position, selected speed 5 and resumed, then measured 120 seconds without camera requests. It collected the report before pausing. No saved starting state or seed was fixed across the separate processes.

## Results

| Window | Submissions | Interval P95 / P99 / max, ms | Present-call P95 / max, ms | Interface age P95, ms | System CPU busy, % |
| --- | --- | --- | --- | --- | --- |
| Paused idle | 3,462 | 7.721 / 8.672 / 15.488 | 6.537 / 13.598 | 20.136 | 22.59 |
| Paused fixed 1 | 3,455 | 7.333 / 7.671 / 11.004 | 6.196 / 9.522 | 20.037 | 23.13 |
| Paused moving 1 | 4,182 | 7.721 / 20.563 / 21.924 | 6.480 / 20.721 | 34.477 | 26.17 |
| Paused moving 2 | 3,457 | 7.518 / 8.075 / 14.000 | 6.284 / 12.510 | 20.065 | 32.32 |
| Paused fixed 2 | 3,447 | 7.533 / 8.221 / 39.445 | 6.270 / 36.947 | 20.113 | 26.56 |
| Speed 5, fixed | 16,250 | 10.028 / 35.510 / 182.359 | 6.095 / 40.450 | 62.237 | 51.96 |

All six windows reached their deadlines, retained their full duration, reported complete collection, dropped no events and recorded no failed submissions. Every successful submission was on the drawing thread. Each paused window had zero completed days and zero gaps above 50 ms. Each interval distribution has one fewer sample than its submission count.

The running window completed 1,884 days (942.0/min), prepared 2,864 interface updates (23.867/s), and retained eleven local-save intervals. It contained 73 completed submission gaps above 50 ms and 13 above 100 ms. Eight submissions were unattributed; interface age was unknown for 68.784 ms. The final unfinished submission gap was 1.188 ms. Interface age P99 was 91.269 ms and its maximum 194.839 ms. No target-relative long-gap result was inferred from the measured frame rate because framework drawing was unlimited.

## What this narrows

The fixed-camera speed-5 P95 remained near 10 ms, as in the earlier baseline, despite a different new-game view and load. This is repeatability of an observation, not a controlled effect of a change.

The two moving-camera P95s were 7.721 and 7.518 ms, against fixed values of 7.333 and 7.533 ms. They do not establish a large stable moving-camera penalty in this warmed route. Moving 1 did have an elevated P99 and interface age, despite more submissions/s (174.25). Its overlapping drawing-log buckets attributed 61 and 33 late drawings to Present; native drawing averaged only 1.0 and 1.2 ms in those buckets. The next moving window did not reproduce that tail. The changing system load and external game prevent attributing the disturbance to camera work or a particular graphics component. Higher submission throughput did not imply better tail latency in that window.

Twelve ten-second drawing-log buckets ending from 05:19:06.686 through 05:20:56.734 straddled most of the running window. They contained 339 late completed drawings: 124 held back, 35 attributed to the lock before drawing, 6 to drawing itself, 85 to Present, 71 to the lock after Present, and 18 to other time. Message popups remained the most common named hold-back source. The first bucket also includes several paused seconds, and the final few running seconds lie outside these buckets. The native-drawing average in the eleven fully running buckets was 2.0–2.9 ms.

As explained in the [baseline record](2026-11-14-frame-p95-baseline.md), these categories are not a decomposition of submission P95. They measure a later endpoint and classify gaps above about 13.89 ms. The 8–10 ms region that determines P95 still needs per-interval stage attribution. The longest 182.359 ms interval cannot consist solely of its ending Present call, since the maximum measured call was 40.450 ms and there were no failures.

## Evidence, cleanup and limits

Raw reports are `target/scenarios/frame-p95-clean/{idle,fixed-1,moving-1,moving-2,fixed-2,running}.json`, with `camera.json`, `drawing-log.txt`, `environment.txt`, `setup/` and `release.txt`. Logs were retained under `%LOCALAPPDATA%/eu4-harmony-dev/claims/loving-wiles-p95-clean-20261114-051545/kept/`.

Process 13628 ended through release at 05:21:02 with code 1 and no crash. The final status check reported zero test games held or running. A non-test-game process remained; no command stopped it. The one-off driving script was removed after its method and results were recorded; reports and selected source evidence were kept.

No runtime optimization was applied. No late-game, physical-display, GPU-time, continuous hardware-drag, per-frame stage attribution or collection-overhead claim is made. Shared CPU/GPU use, the unexplained Present disturbance and only two windows per camera condition limit causal conclusions.
