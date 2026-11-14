# 2026-11-14 Release-path diagnosis map-b (tested by me)

This is the prototype map-b run of the [release-path validation](../plans/frame-p99-release-path-validation.md). It measures actual native-scope entries, scoped acquisitions and early releases, then attributes successful-submission intervals. The [research note](../research/frame-drawing-p99-release-path.md) compares the runs and explains the subsequent reacquisition.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`, temporary matched stage/scope instrumentation; core SHA-256 `cb72fb13edab185fb05bfa13155713b6f201ad31362307593b9ee0564a05f637`.
- Disposable process-start switch `EU4_HARMONY_P99_EARLY_RELEASE=1`. The loaded core hash and the startup log's selected mode were verified.
- Claim `loving-wiles-p99-follow-map-b`, process 93816, held since 2026-11-14 14:52 +08:00. Hidden new Ming campaign, seed 123456; frozen inherited settings/playset, 3840x2160 borderless, vsync at 144 Hz, multisampling 8, drawing on its own thread without the framework's frame limit.
- Pause and settle ten seconds, verify the view, collect ten seconds paused; resume at speed 5, settle five seconds, collect 120 seconds running. No compilation, binary analysis or screenshots during collection.
- The introduction window was closed with one Escape. The sole option of startup event `flavor_mng.42` was answered through `question.answer`, and the question list was empty. Province 1816 was checked as visible land owned and controlled by Ming. An unobstructed screenshot was verified before collection. Further events could open during the running window; no inputs were sent to clear them during sampling.
- Running camera x=4629, y=1325, height=310, province 1816; unchanged across the window. Control-query date bracket 1444.12.30 to 1447.12.11.

## Formal results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days | Days/min | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,439 | 6.943 / 7.870 / 8.145 / 13.538 | 0 | 0.0 | 29.88% |
| running, 120 s | 14,344 | 6.977 / 14.461 / 40.216 / 132.192 | 1069 | 534.5 | 47.05% |

Both windows reached their deadlines, with one context segment, zero dropped events, zero failed submissions and no raw-probe overflow. Raw replay matches each formal interval count and P99. These are Present-wrapper-return intervals, not GPU completion or physical display latency. No marked local save overlaps the running P99 neighborhood.

## Scope evidence

During running collection, the getter had 46,615 recorded main-thread entries and 1,060 actual early releases. No scope fault or promotion was observed in this window. The 1,060 paired acquisitions owned by previously unheld outer scopes have total presentation-lock hold median 16 microseconds and P99 34 microseconds; remaining hold after getter return has median 0 microseconds and P99 1 microseconds. These durations include probe overhead and must not be treated as native CPU self time.

The raw state bits and complete pairs are retained in `diagnosis.json`. The interval partition, entry stacks, exact P99 and nearby lock-dominated example remain separate: ownership overlaps are not added to the interval's exclusive stage shares. The prototype's later reacquisition is documented in the comparison, without claiming the exact P99 sample itself is always lock-dominated.

## Evidence, limits and release

Evidence is local under `target/scenarios/frame-p99-followup/map-b/`; raw records are `traces/93816-{1,2}.txt` in its parent. Settings, mode, core identity, ASLR base, screenshots, formal reports and core logs are retained. The diagnostic build adds locking, memory recording and entry-stack capture; equal instrumentation does not mean equal cost when the prototype creates additional acquisitions. These runs do not quantify uninstrumented optimization gain or all-view safety. Background machine load and different campaign progress remain limitations.

Preflight lists no other game, but postflight lists another session's test game, process 46848 (great-turing-gray, claim since 14:53). Its exact overlap with sampling and CPU/GPU contribution were not recorded. It was left alone. This is not an isolated performance comparison.

Release:

```text
released loving-wiles-p99-follow-map-b's test game, held since 2026-11-14 14:52
  eu4.exe 93816 had ended by 14:57:30: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-follow-map-b-20261114-145255\kept
```

Postflight:

```text
1 test games held, 1 of them running, 4 at most
  great-turing-gray since 2026-11-14 14:53 (Inspect the army button area's background color)
    folder: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\great-turing-gray-20261114-145349
    eu4.exe 46848, on \\.\pipe\eu4-harmony-great-turing-gray
```

The installed framework and player's files were not changed. Final removal of the diagnostic source and artifacts is recorded in the research note.
