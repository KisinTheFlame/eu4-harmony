# The lock-order source of long submission intervals

The [P99 investigation](../research/frame-drawing-p99.md) added a bounded stack capture only when the main thread first acquired the presentation lock for lock ordering, and transition records for the drawing thread's reasons to skip drawing. This follows the two-window [attribution run](2026-11-14-frame-p99-attribution.md); it changes no scheduling or ownership rules.

## Setup and result

- Source `8d5358f1`, release with `dev-test-games,dev-performance`; temporary diff `target/scenarios/frame-p99-attribution/probe-with-lock-sources.patch`.
- Core SHA-256 `b8bb0a66ea8451a68776ed6a48d9f4882c8741f5a37fd4261f6d988d67d2b542`.
- Claim `loving-wiles-p99-sources`, 2026-11-14 05:45:45 +08:00, process 87072. Hidden, seed 123456, new Ming game, inherited playset. Pause, settle ten seconds, 15-second paused collection; speed 5, resume, settle five seconds, 120-second collection.
- Camera x=2995, y=1428, height=310, province 112 throughout. Same 3840×2160, borderless, vsync, multisampling and drawing-unlimited setup as the preceding run. Settings and runtime module base were captured before collection.
- No compilation or binary analysis during collection. The preceding run's postflight listed an external game; this run's postflight listed none. Its exact exit time and contribution were not measured; no process outside the owned claim was controlled.

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days | System CPU busy |
| --- | ---: | --- | ---: | ---: |
| Paused, 15 s | 2,161 | 6.937 / 7.390 / 7.794 / 10.981 | 0 | 28.93% |
| Speed 5, 120 s | 14,646 | 6.945 / 16.279 / 38.583 / 147.236 | 1,235 | 31.79% |

Both reports are complete with one segment, no failed submissions and no event or probe overflow. Offline replay matches formal sample counts and P99. The world reached approximately 1448.5.24. This run is not an optimized-build comparison, and stack-capture overhead has not been isolated.

## What the added observations established

The 147 intervals at ranks 98.5–99.5% span 28.971–65.271 ms. Lock waits occupy 61.82% of their total time, invalidation outside those waits 26.32%, map names due 5.40%, pre-Present drawing 4.86%, and Present 1.58%. The residual is about 0.02%, plus 0.01% for drawing return; rounding prevents exact summation. No marked save overlaps that band. The upper tail contains seven intervals overlapping local saves and leaves 0.61% residual.

The exact P99 interval, 64.597488–64.636071 s, contains 32.574 ms waiting after Present, 3.136 ms waiting before drawing, 2.035 ms drawing, 0.823 ms Present and 0.015 ms remaining. Its main-thread lock-order hold began at 64.595310 s and ended at 64.630050 s, a total 34.740 ms. The preceding successful return occurred inside that hold; only the portion after that return belongs to this interval.

That hold was acquired before entering the critical section at RVA `0x233cdd0`, through `EnterCriticalSection` at IAT RVA `0x1b66750`. The native return-address stack, leaf first, is `0x1a0c764`, `0x1a0c42a`, `0x1a1558`, `0xa3829`, `0xb21f4`, `0x75ba18`, `0x75c313`, `0x758af4`, `0xb795df`. External core/hook frames are retained in the raw record. ASLR addresses were translated using this process's recorded module base.

The same native chain occurs in 1,015 holds totaling 17,111.578 ms and overlapping 3,158.460 ms of the P99 neighborhood. The largest hold in that group is 51.394 ms. This is ownership time, not an additional wait to add to the partition. Existing notes identify the chain as stream construction during naval-doctrine work in an AI country's task. The binary follow-up in the research note identifies the short stream-related lock scope. A [separate run](2026-11-14-frame-p99-lock-scope.md) observes its return, so that long ownership after the scope can be distinguished from contention inside it.

The drawing-skip transitions also expose a third reason to stop drawing: map names are due and must be prepared on the main thread. This explains a measured part of the previous builds' unclassified region without retroactively assigning every old residual to the same cause.

## Evidence and cleanup

Formal reports, copied settings, claim and core hash: `target/scenarios/frame-p99-attribution/lock-sources/`. Raw traces: `traces/87072-{1,2}.txt` under the same root. Replay summaries, interval partitions and lock-source groups use the `87072-` prefix. Equal-microsecond probe records must retain insertion order: sorting full record tuples would reorder stack frames and move a lock object before its source marker; the replay was corrected before drawing these conclusions.

Released at 05:48:49 +08:00, exit 1, no crash. Kept logs: `%LOCALAPPDATA%/eu4-harmony-dev/claims/loving-wiles-p99-sources-20261114-054545/kept/`. Postflight status: zero test games held or running, no external game listed. Runtime probes were retained only for the final scope check and then removed from source and rebuilt outputs.
