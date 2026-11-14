# 2026-11-14 Safe-boundary validation a2 (tested by me)

This is the timing without temporary stage events or entry-stack capture run a2 of the [safe-boundary validation](../plans/frame-p99-safe-boundary-validation.md). The [boundary research](../research/frame-drawing-p99-safe-boundary.md) records the native safety argument and the measured comparison.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`. Loaded core SHA-256 `062ff31762d87ab160c99ae3bf42f6b5e1588d5a6e7eaeea3770fdfe8e2bbfd3`, build `baseline`, early release `False`.
- Hidden test game, owner `loving-wiles-p99-bound-a2`, held since 2026-11-14 16:44; process 102056. Seed 123456, new Ming campaign, byte-identical inherited settings/playset, 3840x2160 borderless, vsync 144 Hz, multisampling 8, drawing on its own thread without the framework frame limit.
- Paused settling for ten seconds; introduction closed with Escape and the sole `flavor_mng.42` option answered through the capability. Empty question list and a visible Beijing province 1816 view at height 310 verified before collection. A screenshot is checked outside collection; its approval file records any initial tooltip. Later events can open without being cleared inside the running window.
- Ten seconds paused, then five seconds of speed-5 settling and 120 seconds running. No compilation, binary analysis or screenshots during collection. A Toolhelp process snapshot every five seconds records other `eu4.exe` processes without controlling them; brief processes between samples can be missed.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days/min | Submissions/s | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,439 | 6.931 / 7.747 / 8.675 / 22.736 | 0.0 | 144.000 | 24.50% |
| running, 120 s | 14,591 | 6.935 / 15.206 / 43.929 / 162.954 | 581.5 | 121.600 | 40.84% |

Completed windows reached their deadlines with one context segment, zero lost events and zero failed submissions. These are successful Present-wrapper-return intervals, not GPU completion or monitor latency. External-game presence was observed in: none of the recorded samples. Background machine work remains uncontrolled; equal seed and wall duration do not produce identical daily progress. A single window does not establish gain or all-path safety.


## Evidence and release

Local evidence: `target/scenarios/frame-p99-boundary/a2/`. Formal reports, process-presence samples, settings, loaded build identity, startup/exit logs and screenshots are retained.

```text
released loving-wiles-p99-bound-a2's test game, held since 2026-11-14 16:44
  eu4.exe 102056 had ended by 16:48:23: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-bound-a2-20261114-164451\kept
```

Postflight:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player files were not changed. Temporary runtime source and build cleanup are recorded with the experiment, separately from this run.
