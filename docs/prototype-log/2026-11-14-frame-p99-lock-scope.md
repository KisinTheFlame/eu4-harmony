# A short game-lock scope leaves the presentation lock held

The final [P99 investigation](../research/frame-drawing-p99.md) probe observes the normal return from the stream-related shared-state getter at `0x1a0c408`. Its guard has already released the internal critical section at that point, as established by reading its full body and the guard's release path. The probe does not release a lock early or otherwise change synchronization. This run tests whether the long hold found in the [source run](2026-11-14-frame-p99-lock-sources.md) persists after that scope ends.

## Setup and full-window results

- Source `8d5358f1`, release with `dev-test-games,dev-performance`, and temporary patch `target/scenarios/frame-p99-attribution/probe-with-lock-scope.patch`.
- Core SHA-256 `05263f7654abb8adf4f57578b4feb1aba06b1e2e56158f5cbd26d3eaffdea980`.
- Claim `loving-wiles-p99-scope`, 2026-11-14 05:53:42 +08:00, process 63288, hidden, seed 123456. New Ming game; pause and ten seconds to settle, 5-second paused collection; speed 5 and five seconds to settle, 60-second running collection.
- Same inherited playset and rendering setup as the source run, camera x=2995, y=1428, height=310, province 112. Settings, module base and core identity were captured before collection.
- No compilation, binary analysis or screenshots during collection; no other game listed at the surrounding status observations. Ordinary background load was not excluded.

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days | System CPU busy |
| --- | ---: | --- | ---: | ---: |
| Paused, 5 s | 719 | 6.941 / 7.339 / 7.638 / 11.917 | 0 | 19.49% |
| Speed 5, 60 s | 7,490 | 6.958 / 13.493 / 30.872 / 106.579 | 656 | 31.75% |

Both formal reports and all probe buffers are complete, with no failed submissions, no dropped records and one context segment. Replay agrees with each report's sample count and P99. This is a shorter diagnostic window, not an optimization: its P99 must not be presented as an improvement over the 120-second windows.

## The internal scope and the remaining hold

Of the lock-order acquisitions whose stack enters `0x1a0c408`, 676 have its return and the later presentation-lock release paired within the window. Time from acquiring the presentation lock to the getter's return includes stack capture and observation overhead; it is an upper bound on the remaining internal scope, not the complete getter's CPU self time.

| Duration | P50 | P95 | P99 | Max |
| --- | ---: | ---: | ---: | ---: |
| Acquisition to getter return, microseconds | 14 | 18 | 32 | 332 |
| Getter return to presentation-lock release, ms | 12.522 | 26.584 | 34.270 | 40.156 |

The ordinary cached path releases the critical section before the getter returns. The large duration is therefore **after** the internal lock operation, while the framework still owns the presentation lock. These are paired observed durations, not a subtraction of two unrelated percentile distributions.

The sample at the formal P99 has these timestamps, relative to the running window:

| Event | Seconds |
| --- | ---: |
| Main thread acquires presentation lock for lock order | 42.461425 |
| Stream-related getter returns, internal lock already released | 42.461439 |
| Previous successful Present returns | 42.466957 |
| Main thread releases presentation lock | 42.491707 |
| Next successful Present returns | 42.497829 |

The trigger's stack is the same stream construction in AI naval-doctrine work identified in the source run. The getter returns 14 microseconds after the framework acquired its lock, but that lock stays owned for another 30.268 ms. Of that hold, 24.750 ms falls after the previous successful submission. Recorded drawing-thread waiting after Present is 24.759 ms; the small difference includes instrumentation and lock-boundary offsets and is not interpreted as exact OS scheduling delay.

The complete 30.872 ms interval contains 24.759 ms waiting after Present, 3.298 ms waiting before drawing, 1.854 ms pre-Present drawing, 0.948 ms Present and 0.013 ms remaining. It overlaps no save. Its long initial hold does not invalidate the prepared update. This directly establishes the overly long lifetime of ownership acquired solely for lock ordering as one cause of submission P99.

## Evidence and cleanup

Formal reports, settings, core hash and claim commands: `target/scenarios/frame-p99-attribution/lock-scope/`. Raw records: `traces/63288-{1,2}.txt`. All 676 paired scopes and the example are in `lock-scope-proof.json`; the replay scripts and the temporary patch are retained as research evidence, not supported tools.

Released at 05:55:34 +08:00, exit 1, no crash. Kept logs: `%LOCALAPPDATA%/eu4-harmony-dev/claims/loving-wiles-p99-scope-20261114-055342/kept/`. Postflight status lists zero held or running test games and no external game. The three temporarily modified Rust source files were restored from their pre-probe copies, and the release core was rebuilt from restored sources. The installed framework and player files were not changed.
