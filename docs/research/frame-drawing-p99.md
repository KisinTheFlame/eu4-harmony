# Why successful-submission interval P99 is long

This investigation follows the successful Present-wrapper returns that the framework's drawing performance report counts. It examines which stages delay the next successful submission, for choosing work on the drawing thread and its coordination with the main thread. The earlier [P95 investigation](frame-drawing-p95.md) found candidates but did not establish their contribution to the measured percentile.

Status: individual-interval attribution and the lock-scope mechanism verified. A bounded ownership prototype completed an A/B/B/A comparison on 2026-11-14 without establishing a repeatable P99 improvement. The prototype was not accepted; runtime source was restored and its patch retained as evidence.

## The question and the measurement

An interval starts at one successful return and ends at the next. Work after the first return, including taking the presentation lock again, belongs to this interval. A failed call would not close it. This is neither the native drawing function's duration nor GPU completion or physical display latency. The source boundaries are [performance.rs](../../core/src/performance.rs), [performance_hooks.rs](../../core/src/performance_hooks.rs), and [framework_ui/hook.rs](../../core/src/framework_ui/hook.rs).

The ordinary report retains distributions, not the events of each slow interval. The existing ten-second drawing log uses a different endpoint (the whole drawing function's return), a different long-gap threshold, and a largest-component category. Its categories cannot be treated as an exact explanation of submission P99.

A temporary `DEBUG-P99` probe retains the ordinary raw events and bounded stage records on the same `Instant` clock. It records:

- Drawing-thread presentation-lock waits of at least 100 microseconds, separately before drawing and after Present. Shorter waits remain in the residual.
- Main-thread lock acquisition and release, with the acquisition reason: application frame, crossing, or day end.
- Drawing start before following map icons and the tooltip, and return from the native drawing function.
- Frame preparation entry, completed preparation, first invalidation source, and the nine specifically observed crossing functions' body entry and return.
- The existing successful and failed submissions, day boundaries, context changes, and local-save boundaries.

The probe changes no scheduling, lock ownership, invalidation rule, draw setting, or game simulation. It adds collection overhead and observers at crossing returns. Records stay in memory during collection; export happens only after the bounded window has ended and its state lock has been released. Raw records are timestamped before the recorder lock. A stage record's reconstructed start uses its measured duration; small instrumentation offsets remain. Overflow is explicit and disqualifies a trace. Native function-body time is inclusive of callees and scheduling, not CPU self time.

The offline replay partitions every completed interval into mutually exclusive time: Present, pre-Present drawing work, lock after Present, lock before drawing, invalidation outside those stages, drawing return outside the lock waits, and residual. Independently reported main-thread lock ownership, day execution, preparation, invalidation, and save overlaps are **not additive**. In particular, lock waiting and invalidation can describe the same time.

The P99 neighborhood is the ordered 98.5th through 99.5th percentile band, and the upper tail starts at the nearest-rank P99 sample. The sample at P99 is also shown as a chronological example. No sum of separately calculated stage percentiles is used.

## Why a short crossing can cause a long gap

The source in [drawing/turns.rs](../../core/src/drawing/turns.rs) establishes two distinct gates. A crossing takes the presentation lock if necessary and retains it until a later in-day frame check. Separately, [drawing/crossings.rs](../../core/src/drawing/crossings.rs) invalidates the prepared frame until another preparation completes. Returning from the crossing function does neither of these things by itself.

The game checks for an in-day frame at selected places, not continuously: [drawing-beside-the-day.md](drawing-beside-the-day.md#where-the-game-lets-a-frame-in-inside-the-day) records the direct call sites and the check after every 32 country updates. This explains the mechanism by which a short function can prevent drawing during a longer stretch of the day's other work. The runtime trace must establish whether this mechanism actually dominates the measured P99.

## Observed causes

Three 120-second speed-5 windows recorded P99 of 39.605, 55.457, and 38.583 ms. The first two are successive windows in one world; the third restarts with the same seed and adds lock-entry stacks and drawing-skip transitions. They are not an optimization A/B, and their different world progress and external load preclude pooling their percentiles. The records are [the first attribution run](../prototype-log/2026-11-14-frame-p99-attribution.md) and [the lock-source run](../prototype-log/2026-11-14-frame-p99-lock-sources.md).

In the third window's 147 intervals around P99 (28.971–65.271 ms), the recorded elapsed time partitions as follows:

| Stage | Share of the band's elapsed time |
| --- | ---: |
| Presentation-lock waits, after Present | 45.74% |
| Presentation-lock waits, before drawing | 16.08% |
| Invalid frame, outside the lock waits | 26.32% |
| Map names due, outside the preceding stages | 5.40% |
| Pre-Present drawing work | 4.86% |
| Present | 1.58% |
| Drawing return and residual | about 0.02% |

The first two windows independently put 86.76% and 88.37% of their neighborhood time in lock waits plus invalidation. This does not measure P95, nor does it establish that every machine has the same mixture.

### The dominant lock hold starts inside AI work

The source run identifies the critical section at `0x233cdd0` as the dominant lock-order trigger. The common observed native return-address chain, leaf first, is:

`0x1a0c764 ← 0x1a0c42a ← 0x1a1558 ← 0xa3829 ← 0xb21f4 ← 0x75ba18 ← 0x75c313 ← 0x758af4`.

The existing notes identify the caller as naval-doctrine work (`0xa3620`) inside an AI country's task (`0xb0fa0`), on the main thread during the AI phase (`0x75b4b0`); see [ai.md](ai.md) and [ai-stage-wait.md](ai-stage-wait.md). The exact chain occurs in 1,015 lock-order holds in this window and overlaps 3,158.460 ms of its P99 neighborhood. That overlap is supporting context, not another additive contribution.

The actual P99 sample is 38.583 ms: 32.574 ms waiting for the lock after the previous Present, 3.136 ms waiting before drawing, 2.035 ms drawing work, 0.823 ms Present, and 0.015 ms remaining. Its lock-order hold begins 2.178 ms before the first successful return and lasts 34.740 ms. The trigger is the chain above. It is not a popup or a new invalidation: the previously prepared update remains valid through that long hold.

This exposes a separate case from a genuine crossing. `lock_taken()` invokes `MainSide::crossing()` to preserve lock order but does not invalidate the frame. `MainSide` records that ownership with the same lifetime as an actual crossing: until a later frame check. Returning from the short game-lock operation does not release the framework's presentation lock. The AI stage has no internal frame checks ([drawing-beside-the-day.md](drawing-beside-the-day.md#where-the-game-lets-a-frame-in-inside-the-day)), so remaining AI work can retain it.

### What the small game-lock scope does

The relevant functions were looked up with `xtask what` and read directly from the installed 1.37.5 binary after the test ended. The retained instructions are `evidence/stream-lock.asm` under the local result folder.

- `0x1a14f0` initializes a stream object, including its state flags, formatting fields, associated buffer and locale state. At `0x1a1553` it calls `0x1a0c408` with a true flag; the observed return address is `0x1a1558`. Its error paths contain `ios_base::badbit set`, `failbit set`, and `eofbit set`.
- `0x1a0c408` constructs a lock guard through `0x1a0c750` with lock number zero, reads the cached shared state at `0x233b2e0`, initializes it if absent, conditionally retains it through its virtual method, destroys the guard through `0x1a0c7c8`, and returns the shared state. Calling it a stream locale getter is an inference from this use, not a recovered original symbol. The once-only initialization helpers were not exhaustively reconstructed.
- Lock number zero takes `0x1a5f504`: after its helper, it enters the critical section at `0x233cdd0` through IAT slot `0x1b66750`. The matching guard destructor takes `0x1a5f520`, which passes the same critical section to IAT slot `0x1b66758`. The PE import table independently identifies these slots as `EnterCriticalSection` and `LeaveCriticalSection`.
- Therefore the normal return from `0x1a0c408` is after this game critical section has been released. The [final scope run](../prototype-log/2026-11-14-frame-p99-lock-scope.md) observes that return without bypassing either lock. In 676 paired scopes, acquisition to getter return has median 14 microseconds and P99 32 microseconds, while the **subsequent** presentation-lock hold has median 12.522 ms and P99 34.270 ms. The shorter 60-second window has submission P99 30.872 ms; its exact P99 sample acquires the lock at 42.461425 s, returns from the getter at 42.461439 s, and releases the presentation lock only at 42.491707 s. The internal scope is over after 14 microseconds; ownership continues for 30.268 ms. The old Present returns partway through that hold, at 42.466957 s, leaving 24.750 ms of ownership inside the measured gap. This distinguishes excessive ownership lifetime from contention inside the small internal lock scope.

### Invalidation outlasts the operation that triggers it

The first run contains a 42.971 ms interval in which a message popup call lasts 5.100 ms, returns, and the next preparation completes another 38.770 ms later. The interval contains 35.413 ms of invalidation outside lock waits. The prepared frame becomes unavailable at the popup entry and remains unavailable until preparation publishes another one; neither returning from the popup nor releasing the lock restores it. The run record gives all timestamps and avoids charging the part of the popup that overlaps the preceding in-flight Present twice.

### Map names form a third gate

The explicit skip transitions in the source run account for 5.40% of neighborhood time and 9.32% of upper-tail time as map names due, outside the other stages. `must_draw_here()` rejects a drawing-thread frame when `prepare::names_due()` is true because name rebuilding reads the live world. This can stop drawing even with no recorded crossing invalidation. The source run reduces the neighborhood residual to about 0.02%; the first two runs lacked this marker and their residual cannot retrospectively be assigned entirely to names.

Marked local saves do occur in the most extreme tail, but none overlaps the P99 neighborhood in these three windows. They are not the leading explanation of those measured P99 values. Present dominates the paused P99 neighborhood; under speed 5, its contribution to the neighborhood falls to 1.58–3.09%. The long running gaps are primarily intervals in which the drawing thread cannot draw the next frame.

## What to optimize first

The first target is the lifetime of ownership acquired **only** to preserve game-lock order. It should be distinguished from application-frame ownership and genuine crossings before testing whether it can end when the relevant game-lock scope ends. Removing lock ordering is unsafe: it prevents an observed deadlock. Such a change must handle nested and recursive locks and promote ownership correctly if a real crossing occurs inside the scope; it needs in-game verification before any gain is claimed.

The second target is the lifetime of invalidated prepared frames, followed by the map-names gate. Making their consumers independent of live state, or safely completing their preparation earlier, is a behavior-preserving design problem. Simply drawing the invalidated data or rebuilding names concurrently is not justified. The repeated terrain/tree calculations from the P95 work remain candidates for ordinary drawing cost, but these traces do not support making them the first P99 intervention.

## Bounded ownership experiment

The [validation plan](../plans/frame-p99-lock-scope-validation.md) tested one change: main-thread entries into `0x1a0c408` opt into a bounded scope, and presentation-lock ownership acquired solely for lock order in that scope may end at its outermost normal return. Preexisting frame/crossing ownership stays intact. A real crossing promotes transient ownership to the existing frame-check lifetime, including renderer calls that would otherwise skip crossing detection because the main thread already holds the lock. Abnormal returns and relevant frame/lifecycle transitions cancel early release. Other lock-order acquisitions retain their previous lifetime.

Twenty ownership-state tests passed, including nesting, preexisting ownership, real crossing promotion, abnormal returns, frame reentry and day end. The full test suite, both ordinary and full developer Clippy checks, and both configurations of the offline byte/observer checks passed. These establish checked transitions and dependency compatibility, not comprehensive runtime safety.

The experiment used release builds with `dev-test-games,dev-performance`, without the earlier per-stage probes. Baseline SHA-256 was `c1d198fa3950abf894707cee4f323c89200676b8fc588844863df659fa9adc45`; prototype SHA-256 was `a6423c8eee52f7797c8eee654163fc2e354ed50e470084d4fddacc67ec68d642`. The loaded copy was checked in each claim. Each hidden new Ming campaign used seed 123456, byte-identical rendering settings and playset, a 10-second paused control after 10 seconds to settle, and a 120-second speed-5 window after five seconds to settle. No compilation or binary analysis ran during measurement.

| Run, execution order | Submission P95 / P99 / max, ms | Completed days/min | Submissions/s | Gaps over 50 / 100 ms | Interface age P95, ms | System CPU busy |
| --- | --- | ---: | ---: | --- | ---: | ---: |
| [A1 baseline](../prototype-log/2026-11-14-frame-p99-optimization-a1.md) | 19.230 / 38.402 / 124.148 | 633.5 | 120.083 | 114 / 8 | 73.832 | 30.87% |
| [B1 prototype](../prototype-log/2026-11-14-frame-p99-optimization-b1.md) | 20.539 / 37.767 / 122.439 | 637.5 | 117.433 | 111 / 5 | 74.334 | 30.49% |
| [B2 prototype](../prototype-log/2026-11-14-frame-p99-optimization-b2.md) | 20.372 / 39.379 / 114.647 | 653.5 | 118.017 | 116 / 8 | 72.176 | 27.48% |
| [A2 baseline](../prototype-log/2026-11-14-frame-p99-optimization-a2.md) | 20.484 / 39.277 / 118.689 | 642.0 | 117.025 | 120 / 8 | 73.987 | 29.08% |

The paired P99 changes are **-1.65%** (B1 against A1) and **+0.26%** (B2 against A2). Completed-day rates change by +0.63% and +1.79%. There is no repeatable P99 reduction, and no consistent improvement across submission rate, P95 or interface age. The small throughput differences do not establish a simulation speed-up. This prototype is not accepted as an optimization.

All eight windows are complete, reach their deadlines, have one context segment and zero dropped events or failed submissions. All four games released normally with exit 1 and no crash. Surrounding status observations list no other game. The four post-window screenshots show coherent text and panels in the same view, but leave the introduction window open over mostly unexplored map at x=2995, y=1428, height=310, province 112. They do not validate ordinary play over Ming's territory, camera movement or all graphics paths.

Paused P99 was 7.696, 7.904, **20.870**, and 7.646 ms in execution order. B2's paused Present-call P99 was 19.766 ms, its submission rate 122.4/s and interface age P95 50.097 ms. The extra presentation disturbance has no established cause and is retained, not discarded. System CPU busy is a whole-machine measure, not the game's CPU cost. Equal seed and wall duration do not replay identical daily work: the running windows begin on 1444.12.30 through 1445.1.1 and end on different dates in 1448.

The ordinary collector does not count scoped acquisitions, promotions or early releases, and these comparison builds intentionally omit the earlier stage traces. Their identity and installed observer are verified, but the measurements do **not** establish how often early release actually occurred or which stage replaced the original wait at the new P99. The negative end-to-end result does not refute the previously observed excessive ownership; it rejects the claim that this narrow prototype has demonstrated a useful improvement. Distinguishing an infrequent eligible path from a shifted bottleneck requires direct acquisition/release evidence in a representative, fixed view.

The complete source patch, build identities, settings, reports, screenshots, test output and retained core logs are under `target/scenarios/frame-p99-optimization/`. The disposable source changes were restored; experimental DLL copies were removed, and package build artifacts were cleared before rebuilding the ordinary test/performance core. No installation, commit or publication was performed.

## Release-path follow-up

The [matched diagnostic follow-up](frame-drawing-p99-release-path.md) establishes what the ordinary comparison reports could not: early release occurred 1,268 times in the original view and 1,060 times in a visible Beijing view, reducing the scoped ownership to median 15 and 16 microseconds. The same stream initialization then entered the same game lock through a second helper outside that scope. On the dominant AI chain, 979 and 784 consecutive pairs left a median one-microsecond gap before reacquiring presentation ownership, which then lasted a median 12.541 and 16.483 ms. The first prototype therefore covers only the first of successive acquisitions.

The four diagnostic windows do not establish a stable gain; another test game appears around the normal-view comparison, and additional acquisitions incur additional probe cost. The note distinguishes exact P99 samples dominated by Present from the surrounding band dominated by lock waiting. At that stage, no broader release boundary had been justified or implemented.

## Four checked boundaries

The subsequent [safe-boundary investigation](frame-drawing-p99-safe-boundary.md) read the successive helpers and the remaining captured formatting path. A complete four-scope diagnostic recorded 41,197 eligible early releases with no lost probes; the fourth helper's hold P99 was 4 microseconds. Cold initialization and abnormal completion retain the original protection, and genuine crossings cannot borrow the temporary ownership for an early release.

Final A/B/B/A timing without temporary stage probes repeated a submission-P99 reduction from 44.338/43.929 ms to 13.166/13.248 ms, or 70.31%/69.84%. Completed-day rates changed by -17.87%/-0.34%, with materially higher system CPU load in the first prototype; a universal daily-rate cost is not isolated. The note preserves all diagnostic failures, the disturbed preliminary baseline, the four completed comparison runs and the broader safety limits. The disposable runtime implementation was removed, with its patch and evidence retained.

## Limits and retained evidence

The original attribution and single-scope experiment established no percentage gain; the later four-scope result is bounded to its measured scene. In the earlier attribution runs, probe overhead was not separately measured, and the lock-source build captures entry stacks. External game processes appeared during parts of the investigation and were left alone; their CPU/GPU contribution is unknown. Direct timing establishes the blocked stages and the lock lifetime mechanism, not how much external load lengthened the AI work. These are early-game Ming windows in the inherited playset, not a late-game or all-view benchmark.

The temporary diagnostic diffs, raw events, executable identities, settings, and replay outputs are retained locally under `target/scenarios/frame-p99-attribution/`. Replay rejects failures, context changes, overflow and overlapping interior pairs, and checks interval counts and P99 against the formal report. Equal-microsecond records retain original insertion order. The probe is not a supported performance API; final cleanup removes it from runtime sources and the normal build outputs.
