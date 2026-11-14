# 2026-11-14 Safe-boundary validation diag-a (tested by me)

This is the matched diagnostic run diag-a of the [safe-boundary validation](../plans/frame-p99-safe-boundary-validation.md). The [boundary research](../research/frame-drawing-p99-safe-boundary.md) records the native safety argument and the measured comparison.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`. Loaded core SHA-256 `7ce52ef6e4015617ee659ee1d41e7e7a04bee84892a540309d368fcac4b86302`, build `diagnostic`, early release `False`.
- Hidden test game, owner `loving-wiles-p99-bound-diag-a`, held since 2026-11-14 15:32; process 7200. Seed 123456, new Ming campaign, byte-identical inherited settings/playset, 3840x2160 borderless, vsync 144 Hz, multisampling 8, drawing on its own thread without the framework frame limit.
- Paused settling for ten seconds; introduction closed with Escape and the sole `flavor_mng.42` option answered through the capability. Empty question list and a visible Beijing province 1816 view at height 310 verified before collection. A screenshot is checked outside collection; its approval file records any initial tooltip. Later events can open without being cleared inside the running window.
- Ten seconds paused, then five seconds of speed-5 settling and 120 seconds running. No compilation, binary analysis or screenshots during collection. A Toolhelp process snapshot every five seconds records other `eu4.exe` processes without controlling them; brief processes between samples can be missed.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days/min | Submissions/s | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,439 | 6.940 / 7.392 / 7.975 / 10.452 | 0.0 | 144.000 | 23.55% |
| running, 120 s | 14,865 | 6.932 / 15.152 / 35.686 / 116.373 | 614.5 | 123.883 | 34.36% |

Completed windows reached their deadlines with one context segment, zero lost events and zero failed submissions. These are successful Present-wrapper-return intervals, not GPU completion or monitor latency. External-game presence was observed in: none of the recorded samples. Background machine work remains uncontrolled; equal seed and wall duration do not produce identical daily progress. A single window does not establish gain or all-path safety.


## Evidence and release

Local evidence: `target/scenarios/frame-p99-boundary/diag-a/`. Formal reports, process-presence samples, settings, loaded build identity, startup/exit logs and screenshots are retained. Raw timelines and offline ownership/interval replay are also retained; their agreement and observed promotions are discussed in the research note.

```text
released loving-wiles-p99-bound-diag-a's test game, held since 2026-11-14 15:32
  eu4.exe 7200 had ended by 15:35:57: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-bound-diag-a-20261114-153215\kept
```

Postflight:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player files were not changed. Temporary runtime source and build cleanup are recorded with the experiment, separately from this run.
