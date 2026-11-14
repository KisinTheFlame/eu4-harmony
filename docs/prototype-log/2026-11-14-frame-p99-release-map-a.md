# 2026-11-14 Release-path diagnosis map-a (tested by me)

This is the baseline map-a run of the [release-path validation](../plans/frame-p99-release-path-validation.md). It measures actual native-scope entries, scoped acquisitions and early releases, then attributes successful-submission intervals. The [research note](../research/frame-drawing-p99-release-path.md) compares the runs and explains the subsequent reacquisition.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`, temporary matched stage/scope instrumentation; core SHA-256 `cb72fb13edab185fb05bfa13155713b6f201ad31362307593b9ee0564a05f637`.
- Disposable process-start switch `EU4_HARMONY_P99_EARLY_RELEASE=0`. The loaded core hash and the startup log's selected mode were verified.
- Claim `loving-wiles-p99-follow-map-a`, process 90268, held since 2026-11-14 14:57 +08:00. Hidden new Ming campaign, seed 123456; frozen inherited settings/playset, 3840x2160 borderless, vsync at 144 Hz, multisampling 8, drawing on its own thread without the framework's frame limit.
- Pause and settle ten seconds, verify the view, collect ten seconds paused; resume at speed 5, settle five seconds, collect 120 seconds running. No compilation, binary analysis or screenshots during collection.
- The introduction window was closed with one Escape. The sole option of startup event `flavor_mng.42` was answered through `question.answer`, and the question list was empty. Province 1816 was checked as visible land owned and controlled by Ming. An unobstructed screenshot was verified before collection. Further events could open during the running window; no inputs were sent to clear them during sampling.
- Running camera x=4629, y=1325, height=310, province 1816; unchanged across the window. Control-query date bracket 1444.12.24 to 1448.1.1.

## Formal results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days | Days/min | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,437 | 6.942 / 7.627 / 8.150 / 41.740 | 0 | 0.0 | 39.24% |
| running, 120 s | 13,462 | 7.521 / 16.358 / 44.727 / 168.457 | 1095 | 547.5 | 46.13% |

Both windows reached their deadlines, with one context segment, zero dropped events, zero failed submissions and no raw-probe overflow. Raw replay matches each formal interval count and P99. These are Present-wrapper-return intervals, not GPU completion or physical display latency. No marked local save overlaps the running P99 neighborhood.

## Scope evidence

During running collection, the getter had 37,118 recorded main-thread entries and 0 actual early releases. No scope fault or promotion was observed in this window. The 1,058 paired acquisitions owned by previously unheld outer scopes have total presentation-lock hold median 14656 microseconds and P99 50729 microseconds; remaining hold after getter return has median 14643 microseconds and P99 50710 microseconds. These durations include probe overhead and must not be treated as native CPU self time.

The raw state bits and complete pairs are retained in `diagnosis.json`. The interval partition, entry stacks, exact P99 and nearby lock-dominated example remain separate: ownership overlaps are not added to the interval's exclusive stage shares. The prototype's later reacquisition is documented in the comparison, without claiming the exact P99 sample itself is always lock-dominated.

## Evidence, limits and release

Evidence is local under `target/scenarios/frame-p99-followup/map-a/`; raw records are `traces/90268-{1,2}.txt` in its parent. Settings, mode, core identity, ASLR base, screenshots, formal reports and core logs are retained. The diagnostic build adds locking, memory recording and entry-stack capture; equal instrumentation does not mean equal cost when the prototype creates additional acquisitions. These runs do not quantify uninstrumented optimization gain or all-view safety. Background machine load and different campaign progress remain limitations.

Preflight lists another session's test game, process 46848 (great-turing-gray, claim since 14:53); postflight lists none. Its exit time and overlap with sampling were not recorded. It was left alone. This is not an isolated performance comparison.

Release:

```text
released loving-wiles-p99-follow-map-a's test game, held since 2026-11-14 14:57
  eu4.exe 90268 had ended by 15:01:54: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-follow-map-a-20261114-145730\kept
```

Postflight:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player's files were not changed. Final removal of the diagnostic source and artifacts is recorded in the research note.
