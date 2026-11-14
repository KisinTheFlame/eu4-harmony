# Successful-submission P99 attributed to individual intervals

The [P99 investigation](../research/frame-drawing-p99.md) followed the successful Present-wrapper returns on one clock, together with temporary records of lock waits, main-thread lock ownership, drawing, preparation, invalidation sources and crossing function bodies. Both running windows reproduced long submission intervals. The dominant waits were directly recorded; no optimization was applied.

## Setup

- Source: `8d5358f1`, with the temporary `DEBUG-P99` patch retained at `target/scenarios/frame-p99-attribution/probe.patch`. The checkout included the latest `origin/master` before testing.
- Core SHA-256: `94aaa8d17a4b4b6fac83f113836028cd4df66f0743ac672aa51b01eb75b56a3e`; `dev-test-games,dev-performance`, release profile. No stack sampling in this build.
- Claim `loving-wiles-p99`, created 2026-11-14 05:38:51 +08:00; process 90164, hidden, seed 123456. `menu.start country=MNG`, then pause and ten seconds to settle. After the paused window: speed 5, resume, five seconds to settle, then two successive 120-second windows in the same world.
- Camera stayed at x=2995, y=1428, height=310, province 112. No camera or input requests during collection. Inherited playset and settings were retained locally in `settings/` under the result folder.
- Settings: 3840×2160 borderless, refreshRate=144, vsync=yes, multisampling=8; framework drawing unlimited. This differs from the earlier P95 baseline's saved refreshRate=60. Neither comparison is an optimization A/B.
- No external game was listed at the initial preflight, but post-release status listed external game process 96668. Its start time and contribution were not measured, and it was left alone. No build or binary analysis ran during these windows. System CPU percentages include the other process and ordinary desktop load; this is not an exclusive-machine benchmark.

## Complete windows

| Window | Seconds | Completed intervals | P50 / P95 / P99 / max, ms | Completed days | System CPU busy |
| --- | ---: | ---: | --- | ---: | ---: |
| Paused | 15 | 2,156 | 6.947 / 7.333 / 7.738 / 10.383 | 0 | 21.94% |
| Speed 5, first | 120 | 14,825 | 6.923 / 15.302 / 39.605 / 200.387 | 1,175 | 41.08% |
| Speed 5, second | 120 | 13,399 | 6.987 / 18.688 / 55.457 / 158.308 | 937 | 52.78% |

All ordinary reports are complete, have one context segment, zero failed submissions and zero dropped events. All temporary probe overflow counters are zero. Offline replay matches each formal report's interval sample count and P99 exactly. Interior preparation and lock-hold pairs pass the pairing checks; an in-flight hold or day at a window boundary is clipped and explicitly counted. The first running window ended around 1448.3.23, the second around 1450.10.21; they are successive world states, not identical replays.

## What occupied the P99 neighborhood

The nearest-rank 98.5–99.5% band contains 149 intervals in the first window (31.783–66.681 ms) and 135 in the second (46.537–77.770 ms). The columns below partition the elapsed time of those intervals; invalidation excludes time already assigned to a lock wait or other recorded stage.

| Stage | First band | Second band |
| --- | ---: | ---: |
| Lock after Present | 51.86% | 64.46% |
| Lock before drawing | 15.50% | 12.74% |
| Invalid frame outside those waits | 19.40% | 11.17% |
| Pre-Present drawing work | 5.13% | 5.15% |
| Present | 3.09% | 1.96% |
| Drawing return and residual | 5.02% | 4.52% |

Thus lock waits plus invalidation account for 86.76% and 88.37% of the neighborhood's time. Neither neighborhood overlaps a marked local save. In the upper 1% tail, seven and five intervals overlap saves respectively; residual time is larger there (8.95% and 14.07%), motivating the next run's explicit drawing-skip markers. The first build cannot explain those residuals from its own records alone.

The exact P99 samples are also lock-dominated:

| Interval | Lock after Present | Lock before drawing | Drawing work | Present | Remaining |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 39.605 ms | 35.108 | 2.133 | 1.923 | 0.428 | 0.013 |
| 55.457 ms | 48.696 | 3.820 | 2.380 | 0.548 | 0.013 |

For the first sample, the main thread acquired the lock for a crossing 1.530 ms before the previous successful submission, released it 35.098 ms after it, then ran an application-frame update and preparation. No actual invalidation was recorded at that first acquisition. The source permits this through its lock-order guard, which takes the presentation lock before a game lock without invalidating the frame. The [source follow-up](2026-11-14-frame-p99-lock-sources.md) records those entries explicitly.

## A separate invalidation example

A 42.971 ms interval at 18.024842–18.067813 seconds in the first running window overlaps a popup (`0x8c1430`). The popup starts at 18.021226 s, returns at 18.026326 s (5.100 ms inclusive wall time), and the main thread releases the lock at 18.027048 s. Preparation does not complete until 18.065096 s: 38.770 ms after the popup returned. The last part of the popup overlaps the old frame's in-flight Present, so it is wrong to charge the whole popup call to this interval.

The interval partitions into 35.413 ms of invalidation outside lock waits, 4.923 ms of lock waits, 2.187 ms drawing, 0.446 ms Present, and 0.002 ms residual. This directly shows a short popup followed by a much longer wait for a drawable prepared frame. The popup's return does not make the already invalidated frame drawable again.

## Evidence, limitations, and cleanup

Local evidence is under `target/scenarios/frame-p99-attribution/`: three formal JSON reports, raw `traces/90164-{1,2,3}.txt`, per-interval replay JSON, summary JSON, nearest-P99 timelines, the popup example, module base, settings, hashes and the probe patch. Lock spans in this build reconstruct the start from duration at the probe call; their placement has small recording offsets. Do not interpret residual microseconds as precise OS scheduling delay, or inclusive function-body wall time as CPU self time. Main-thread lock ownership can overlap Present and is not another additive stage.

Released at 05:43:55 +08:00, exit 1, no crash. Kept logs: `%LOCALAPPDATA%/eu4-harmony-dev/claims/loving-wiles-p99-20261114-053851/kept/`. `game status` after release reported zero held test games. The temporary probes remained for the separately recorded source follow-up and are removed as part of the investigation's final cleanup.
