# 2026-11-14 Release-path diagnosis old-a (tested by me)

This is the baseline old-a run of the [release-path validation](../plans/frame-p99-release-path-validation.md). It measures actual native-scope entries, scoped acquisitions and early releases, then attributes successful-submission intervals. The [research note](../research/frame-drawing-p99-release-path.md) compares the runs and explains the subsequent reacquisition.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`, temporary matched stage/scope instrumentation; core SHA-256 `cb72fb13edab185fb05bfa13155713b6f201ad31362307593b9ee0564a05f637`.
- Disposable process-start switch `EU4_HARMONY_P99_EARLY_RELEASE=0`. The loaded core hash and the startup log's selected mode were verified.
- Claim `loving-wiles-p99-follow-old-a`, process 81860, held since 2026-11-14 14:46 +08:00. Hidden new Ming campaign, seed 123456; frozen inherited settings/playset, 3840x2160 borderless, vsync at 144 Hz, multisampling 8, drawing on its own thread without the framework's frame limit.
- Pause and settle ten seconds, verify the view, collect ten seconds paused; resume at speed 5, settle five seconds, collect 120 seconds running. No compilation, binary analysis or screenshots during collection.
- The original introduction window remained open over mostly unexplored map, matching the first optimization experiment. This is a mechanism comparison, not a normal-map benchmark.
- Running camera x=2995, y=1428, height=310, province 112; unchanged across the window. Control-query date bracket 1444.12.31 to 1448.7.10.

## Formal results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days | Days/min | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,437 | 6.944 / 7.318 / 7.885 / 14.131 | 0 | 0.0 | 20.63% |
| running, 120 s | 15,263 | 6.923 / 14.552 / 31.259 / 118.254 | 1278 | 639.0 | 31.54% |

Both windows reached their deadlines, with one context segment, zero dropped events, zero failed submissions and no raw-probe overflow. Raw replay matches each formal interval count and P99. These are Present-wrapper-return intervals, not GPU completion or physical display latency. No marked local save overlaps the running P99 neighborhood.

## Scope evidence

During running collection, the getter had 70,303 recorded main-thread entries and 0 actual early releases. No scope fault or promotion was observed in this window. The 1,285 paired acquisitions owned by previously unheld outer scopes have total presentation-lock hold median 14084 microseconds and P99 37199 microseconds; remaining hold after getter return has median 14070 microseconds and P99 37185 microseconds. These durations include probe overhead and must not be treated as native CPU self time.

The raw state bits and complete pairs are retained in `diagnosis.json`. The interval partition, entry stacks, exact P99 and nearby lock-dominated example remain separate: ownership overlaps are not added to the interval's exclusive stage shares. The prototype's later reacquisition is documented in the comparison, without claiming the exact P99 sample itself is always lock-dominated.

## Evidence, limits and release

Evidence is local under `target/scenarios/frame-p99-followup/old-a/`; raw records are `traces/81860-{1,2}.txt` in its parent. Settings, mode, core identity, ASLR base, screenshots, formal reports and core logs are retained. The diagnostic build adds locking, memory recording and entry-stack capture; equal instrumentation does not mean equal cost when the prototype creates additional acquisitions. These runs do not quantify uninstrumented optimization gain or all-view safety. Background machine load and different campaign progress remain limitations.

Surrounding preflight and postflight status list no other held or running game; background machine load was not isolated.

Release:

```text
released loving-wiles-p99-follow-old-a's test game, held since 2026-11-14 14:46
  eu4.exe 81860 had ended by 14:49:34: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-follow-old-a-20261114-144610\kept
```

Postflight:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player's files were not changed. Final removal of the diagnostic source and artifacts is recorded in the research note.
