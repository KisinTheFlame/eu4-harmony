# 2026-11-14 Safe-boundary validation a1 (tested by me)

This is the timing without temporary stage events or entry-stack capture run a1 of the [safe-boundary validation](../plans/frame-p99-safe-boundary-validation.md). The [boundary research](../research/frame-drawing-p99-safe-boundary.md) records the native safety argument and the measured comparison.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`. Loaded core SHA-256 `062ff31762d87ab160c99ae3bf42f6b5e1588d5a6e7eaeea3770fdfe8e2bbfd3`, build `baseline`, early release `False`.
- Hidden test game, owner `loving-wiles-p99-bound-a1`, held since 2026-11-14 16:25; process 100648. Seed 123456, new Ming campaign, byte-identical inherited settings/playset, 3840x2160 borderless, vsync 144 Hz, multisampling 8, drawing on its own thread without the framework frame limit.
- Paused settling for ten seconds; introduction closed with Escape and the sole `flavor_mng.42` option answered through the capability. Empty question list and a visible Beijing province 1816 view at height 310 verified before collection. A screenshot is checked outside collection; its approval file records any initial tooltip. Later events can open without being cleared inside the running window.
- Ten seconds paused, then five seconds of speed-5 settling and 120 seconds running. No compilation, binary analysis or screenshots during collection. A Toolhelp process snapshot every five seconds records other `eu4.exe` processes without controlling them; brief processes between samples can be missed.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days/min | Submissions/s | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,438 | 6.939 / 7.418 / 7.657 / 19.170 | 0.0 | 143.900 | 22.70% |
| running, 120 s | 14,244 | 6.946 / 13.751 / 50.765 / 187.164 | 501.5 | 118.708 | 60.23% |

Completed windows reached their deadlines with one context segment, zero lost events and zero failed submissions. These are successful Present-wrapper-return intervals, not GPU completion or monitor latency. External-game presence was observed in: running. Background machine work remains uncontrolled; equal seed and wall duration do not produce identical daily progress. A single window does not establish gain or all-path safety.


## Evidence and release

Local evidence: `target/scenarios/frame-p99-boundary/a1/`. Formal reports, process-presence samples, settings, loaded build identity, startup/exit logs and screenshots are retained.

```text
released loving-wiles-p99-bound-a1's test game, held since 2026-11-14 16:25
  eu4.exe 100648 had ended by 16:28:48: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-bound-a1-20261114-162529\kept
```

Postflight:

```text
1 test games held, 1 of them running, 4 at most
  ordering-keytrace-1de2b155 since 2026-11-14 16:28 (Trace the ineffective first Escape and exercise host binding)
    folder: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-keytrace-1de2b155-20261114-162815
    eu4.exe 94236, on \\.\pipe\eu4-harmony-ordering-keytrace-1de2b155
```

The installed framework and player files were not changed. Temporary runtime source and build cleanup are recorded with the experiment, separately from this run.
