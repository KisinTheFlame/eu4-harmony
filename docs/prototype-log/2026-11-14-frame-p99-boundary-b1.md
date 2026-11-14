# 2026-11-14 Safe-boundary validation b1 (tested by me)

This is the timing without temporary stage events or entry-stack capture run b1 of the [safe-boundary validation](../plans/frame-p99-safe-boundary-validation.md). The [boundary research](../research/frame-drawing-p99-safe-boundary.md) records the native safety argument and the measured comparison.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`. Loaded core SHA-256 `c480d4a51cb66ff61d211e04927a461298729ac2f8d8e5a28b256b07eb073553`, build `prototype`, early release `True`.
- Hidden test game, owner `loving-wiles-p99-bound-b1`, held since 2026-11-14 16:37; process 68848. Seed 123456, new Ming campaign, byte-identical inherited settings/playset, 3840x2160 borderless, vsync 144 Hz, multisampling 8, drawing on its own thread without the framework frame limit.
- Paused settling for ten seconds; introduction closed with Escape and the sole `flavor_mng.42` option answered through the capability. Empty question list and a visible Beijing province 1816 view at height 310 verified before collection. A screenshot is checked outside collection; its approval file records any initial tooltip. Later events can open without being cleared inside the running window.
- Ten seconds paused, then five seconds of speed-5 settling and 120 seconds running. No compilation, binary analysis or screenshots during collection. A Toolhelp process snapshot every five seconds records other `eu4.exe` processes without controlling them; brief processes between samples can be missed.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days/min | Submissions/s | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,437 | 6.966 / 7.968 / 8.617 / 11.450 | 0.0 | 143.800 | 85.11% |
| running, 120 s | 16,085 | 6.963 / 8.137 / 13.166 / 194.759 | 473.5 | 134.050 | 66.54% |

Completed windows reached their deadlines with one context segment, zero lost events and zero failed submissions. These are successful Present-wrapper-return intervals, not GPU completion or monitor latency. External-game presence was observed in: none of the recorded samples. Background machine work remains uncontrolled; equal seed and wall duration do not produce identical daily progress. A single window does not establish gain or all-path safety.


## Evidence and release

Local evidence: `target/scenarios/frame-p99-boundary/b1/`. Formal reports, process-presence samples, settings, loaded build identity, startup/exit logs and screenshots are retained.

```text
released loving-wiles-p99-bound-b1's test game, held since 2026-11-14 16:37
  eu4.exe 68848 had ended by 16:40:55: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-bound-b1-20261114-163710\kept
```

Postflight:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player files were not changed. Temporary runtime source and build cleanup are recorded with the experiment, separately from this run.
