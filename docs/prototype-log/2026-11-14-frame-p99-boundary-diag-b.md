# 2026-11-14 Safe-boundary validation diag-b (tested by me)

This is the matched diagnostic run diag-b of the [safe-boundary validation](../plans/frame-p99-safe-boundary-validation.md). The [boundary research](../research/frame-drawing-p99-safe-boundary.md) records the native safety argument and the measured comparison.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`. Loaded core SHA-256 `7ce52ef6e4015617ee659ee1d41e7e7a04bee84892a540309d368fcac4b86302`, build `diagnostic`, early release `True`.
- Hidden test game, owner `loving-wiles-p99-bound-diag-b`, held since 2026-11-14 15:35; process 102472. Seed 123456, new Ming campaign, byte-identical inherited settings/playset, 3840x2160 borderless, vsync 144 Hz, multisampling 8, drawing on its own thread without the framework frame limit.
- Paused settling for ten seconds; introduction closed with Escape and the sole `flavor_mng.42` option answered through the capability. Empty question list and a visible Beijing province 1816 view at height 310 verified before collection. A screenshot is checked outside collection; its approval file records any initial tooltip. Later events can open without being cleared inside the running window.
- Ten seconds paused, then five seconds of speed-5 settling and 120 seconds running. No compilation, binary analysis or screenshots during collection. A Toolhelp process snapshot every five seconds records other `eu4.exe` processes without controlling them; brief processes between samples can be missed.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days/min | Submissions/s | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,396 | 7.089 / 8.089 / 9.409 / 45.869 | 0.0 | 139.700 | 31.11% |
| running, 120 s | 14,235 | 6.970 / 16.499 / 38.587 / 122.271 | 606.5 | 118.633 | 34.29% |

Completed windows reached their deadlines with one context segment, zero lost events and zero failed submissions. These are successful Present-wrapper-return intervals, not GPU completion or monitor latency. External-game presence was observed in: none of the recorded samples. Background machine work remains uncontrolled; equal seed and wall duration do not produce identical daily progress. A single window does not establish gain or all-path safety.


## Evidence and release

Local evidence: `target/scenarios/frame-p99-boundary/diag-b/`. Formal reports, process-presence samples, settings, loaded build identity, startup/exit logs and screenshots are retained. Raw timelines and offline ownership/interval replay are also retained; their agreement and observed promotions are discussed in the research note.

```text
released loving-wiles-p99-bound-diag-b's test game, held since 2026-11-14 15:35
  eu4.exe 102472 had ended by 15:39:23: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-bound-diag-b-20261114-153558\kept
```

Postflight:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player files were not changed. Temporary runtime source and build cleanup are recorded with the experiment, separately from this run.
