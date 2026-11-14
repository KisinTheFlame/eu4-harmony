# 2026-11-14 Safe-boundary validation diag-e (tested by me)

This is the mechanism recheck (not a matched speed comparison with earlier builds having fewer scopes) run diag-e of the [safe-boundary validation](../plans/frame-p99-safe-boundary-validation.md). The [boundary research](../research/frame-drawing-p99-safe-boundary.md) records the native safety argument and the measured comparison.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`. Loaded core SHA-256 `f32afc0ab807b715ec26cf2763d0bdf93340563243ccd09ef5f66866394c0cff`, build `diagnostic-final-repeat`, early release `True`.
- Hidden test game, owner `loving-wiles-p99-bound-diag-e`, held since 2026-11-14 16:19; process 103524. Seed 123456, new Ming campaign, byte-identical inherited settings/playset, 3840x2160 borderless, vsync 144 Hz, multisampling 8, drawing on its own thread without the framework frame limit.
- Paused settling for ten seconds; introduction closed with Escape and the sole `flavor_mng.42` option answered through the capability. Empty question list and a visible Beijing province 1816 view at height 310 verified before collection. A screenshot is checked outside collection; its approval file records any initial tooltip. Later events can open without being cleared inside the running window.
- Ten seconds paused, then five seconds of speed-5 settling and 60 seconds running. No compilation, binary analysis or screenshots during collection. A Toolhelp process snapshot every five seconds records other `eu4.exe` processes without controlling them; brief processes between samples can be missed.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days/min | Submissions/s | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,436 | 6.932 / 7.470 / 7.957 / 30.396 | 0.0 | 143.700 | 33.22% |
| running, 60 s | 7,986 | 6.939 / 7.801 / 16.209 / 180.882 | 475.0 | 133.117 | 60.43% |

Completed windows reached their deadlines with one context segment, zero lost events and zero failed submissions. These are successful Present-wrapper-return intervals, not GPU completion or monitor latency. External-game presence was observed in: paused, running. Background machine work remains uncontrolled; equal seed and wall duration do not produce identical daily progress. A single window does not establish gain or all-path safety.


## Evidence and release

Local evidence: `target/scenarios/frame-p99-boundary/diag-e/`. Formal reports, process-presence samples, settings, loaded build identity, startup/exit logs and screenshots are retained. Raw timelines and offline ownership/interval replay are also retained; their agreement and observed promotions are discussed in the research note.

```text
released loving-wiles-p99-bound-diag-e's test game, held since 2026-11-14 16:19
  eu4.exe 103524 had ended by 16:22:11: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-bound-diag-e-20261114-161953\kept
```

Postflight:

```text
1 test games held, 1 of them running, 4 at most
  ordering-native-repeat-1de2b155 since 2026-11-14 16:20 (Verify native-drawing keyboard course with each callback observed before the next key)
    folder: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-native-repeat-1de2b155-20261114-162011
    eu4.exe 99432, on \\.\pipe\eu4-harmony-ordering-native-repeat-1de2b155
```

The installed framework and player files were not changed. Temporary runtime source and build cleanup are recorded with the experiment, separately from this run.
