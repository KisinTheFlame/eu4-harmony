# Performance with the drawing thread

Status: the agreed first implementation was built and verified on 2026-11-14: bounded collection, completed-day/submission/interface reports, both presenting threads, repository and offline checks, a full default baseline and three collection-on/off pairs. The [results](../performance.md#completed-days-submissions-and-updates-2026-11-14) retain all runs; their variation leaves a precise collection-cost estimate unresolved. Action response, pure interface-work duration and the full save-based comparison matrix remain deferred. The broader proposal remains the direction for later work, not a claim that all of P1 to P4 is built or measured. It adds Performance window and Interface update age to [GLOSSARY.md](../../GLOSSARY.md). It rests on [ADR simulation-and-interface-take-turns](../adr/2026-11-10-simulation-and-interface-take-turns.md). Existing results remain in [performance.md](../performance.md) under their original measurement methods.

## Design discussion

The owner requested implementation in `scenario perf`, beginning with days, drawing and interface updates; action-response measurement is deferred from this first implementation.

- **Scenario flow — delegated to the implementer.** The flow serves the measurements; preserving the old scenario is not a requirement. The initial implementation will collect the three dimensions together in separate in-process windows for paused play and speed 5, retaining interruptions in each window. The existing startup and resource results remain useful context.
- **Performance judgment — agreed.** The first report has no performance pass/fail thresholds. It reports results and variation; failed collection, lost records and missing required observations must be identified as incomplete measurement rather than a passing performance result.
- **Collection build — agreed.** Collection is an independent developer feature. `scenario perf` uses a build with that feature and test-game pipe support, adding the existing test switches only when requested. It does not enable the full developer tool set. Reports identify this as an instrumented developer build; player builds do not acquire collection code through the test-game pipe feature. Collection overhead must be measured.
- **Boundary investigation.** Existing sequence publication supports update-ready intervals and handover timing. Interface work spans multiple stretches of the main-thread frame, so a pure interface-work duration needs further boundary research; a whole-frame timer must not be substituted. Successful submission timing also requires verification of the current Present wrapper and must exclude the subsequent presentation-lock reacquisition.

The owner confirmed the complete first implementation below and requested implementation on 2026-11-14.

### First implementation

The first delivery comprises P1, the supported measurements in P2, and the integration/reporting portion of P4. P3 and P4's full save-based comparison matrix remain later work.

- Days: completed-day count and rate, day durations and completion intervals, with calendar classification and interruptions identified.
- Drawing: verified successful-submission intervals from either presenting thread, their distribution and long-gap counts, including unfinished gaps at the end of collection.
- Interface: preparation-ready intervals, ready-to-first-submission delay, superseded/invalidated/pending updates, and wall-time age of the last attributable submitted update. Pure interface-work duration is explicitly unmeasured in this delivery; it must not be replaced by whole-frame duration.
- Integration: separate in-process paused and speed-5 windows, a bounded collection/export path and results in the existing `report.json` and `report.md`. Retain startup and resource measurements as context. No player-facing performance display or new player setting.
- Verification: deterministic accounting checks, repository checks, and owned hidden test-game runs of the extended scenario after implementation, including collection-overhead checks. No preliminary run of the old scenario is required. Hidden submission timing does not establish visible display latency.

Success means the measurements are correctly bounded, attributed and reported, not that the game meets a new speed threshold. Unsupported endpoints, sample loss and incomplete collection are explicit. Final agreement was received before implementation began.

## Goal and scope

Measure four things together: how quickly days finish, how evenly frames are submitted, how often the interface updates, and how promptly an action takes effect and reaches a submitted frame. A faster date with worse interaction is a tradeoff to report, not an unqualified improvement. There is no combined performance score.

The main thread runs simulation and interface updates in turn; the drawing thread draws and handles camera input. Worker threads help with parts of the day. Two principal threads therefore do not imply only two measures of the player's experience.

This work proposes a repeatable developer measurement and report, not a new player setting or a performance display. Startup, loading, saving, CPU use and memory remain part of the performance report. Measuring the monitor's actual display latency, changing the threading model and making the simulation faster are outside this plan.

## What the measurements rest on

- A day advances its date before doing its work. Completion must be observed at the return of the full daily update, including its monthly and yearly work, not inferred from the date changing ([game/day.md](../game/day.md), [research/time.md](../research/time.md)).
- Frames inside a day update the interface without executing queued game commands. An interface update does not mean that a day or a command has completed ([game/day.md](../game/day.md)).
- Camera movement can continue while the interface still shows what its previous update left. Hover content and ordinary buttons follow a different path from camera input ([drawing-thread.md](drawing-thread.md)).
- Crossings, frame preparation and the presentation lock can hold up drawing. The two threads still share work and wait for each other ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md)).

These are the basis for measurement, not a claim that every required observation point is already available. Any new game observation point must be researched and declared as a dependency point under the repository's usual rules before implementation.

## Measurement boundaries

All in-process events use one monotonic clock and carry a session identifier. The report names the start and end of the measured window. A load, a new session, a speed change or a drawing-mode change starts a new segment; intervals never bridge unrelated segments.

| Event | Exact proposed boundary | What it does not establish |
| --- | --- | --- |
| Day begins / completes | Entry / return of the full daily update, labeled with the day it computes | Date increment alone does not establish completion |
| Interface update begins / ready | Entry of the identified interface-update pass / completion of its preparation and publication for drawing | Publication does not mean the update has been drawn; an update inside a day is not a whole-day snapshot |
| Frame submission begins / returns | Immediately around the actual Present call, with its result and presenting thread | CPU return does not establish GPU completion, compositor display or light from the monitor |
| Input received | The first instrumented receipt in the game process, on the applicable input path | It excludes device and operating-system delay before that point |
| Action applied | A verified state transition or camera-state change attributable to that action | A request being dequeued or acknowledged is not necessarily application |
| Result submitted | The first successfully submitted frame known to contain that action's result | The next arbitrary frame is not evidence of the result |

Interface timing needs the entry of the actual update pass, not the entry of the whole main-thread frame, which can execute a day. Its ready event includes deferred work and preparation needed for drawing. Separate intervals are recorded for updates between days and inside a day. Preparation without an ordinary update is labeled separately.

The Present boundary must be checked through the current wrapper to the actual device call, including its success result. Use submission-return intervals as the in-process frame measure, and retain call-entry timestamps and call durations to explain blocking. Count successful in-game submissions from either thread; label main-thread drawing separately. Exclude off-screen texture passes. Failed submissions and device reset are reported as interruptions, not successful frames.

A prepared sequence can be drawn several times, superseded before it is drawn, or invalidated by a crossing. Track those outcomes explicitly. Main-thread drawing also needs an update-to-frame association; it cannot silently vanish from the sample. A published sequence describes an update, not a copied world or an atomic snapshot of all game state.

## The four measures

### Pacing of days

The primary speed-5 result is **completed game days per wall-clock minute**: completed days in the measured window divided by its elapsed seconds, times 60. Prefer a fixed number of completed days for comparisons of the same calendar stretch; also record the elapsed time and actual dates. For a fixed-duration run, count completed days and report any unfinished day at the end.

Report two distributions in milliseconds:

- **Day duration**: day entry to return, including waits and interface updates inside that call. It is elapsed time, not CPU time or pure simulation work.
- **Day completion interval**: one completion to the next, including work and waits between days. This describes the pace the player experiences.

Report all days together, then ordinary days, month starts and year starts separately. Use mutually exclusive classes, with year starts taking precedence over month starts. Autosave is an additional overlapping marker; it is not another disjoint class to sum with them.

At speeds 1 to 4, report the configured pace and observed completion intervals and throughput. The game's nominal intervals are 2, 1, 0.5 and 0.2 seconds per day; record these as references, not a promise of exact scheduling. Maintaining the configured pace with lower cost is useful even when throughput cannot increase.

Keep autosaves and involuntary stalls in the overall result. Label pauses and their causes and duration. A deliberately paused interaction segment has no simulation-throughput target. An unexpected event pause is reported; a standard unattended throughput run with an unplanned pause is not comparable until its pause policy is corrected. If an active-time rate excluding pauses is also given, it is secondary and its excluded time is explicit.

### Evenness of drawing

Report successful submissions per second and the distribution of submission-return intervals. For a fixed target rate `R`, the target interval is `T = 1000 / R` milliseconds. Report:

- Median, P95, P99 and maximum interval.
- Intervals greater than `2T`, both as a count and per measured minute.
- Intervals greater than 50 ms and 100 ms, as separate overlapping counts per minute.
- The fraction of measured time covered by intervals greater than `2T`, alongside the count, so one long stop cannot disappear among many short frames.

These are threshold counts, not a count of dropped display frames. At 144 Hz, `T` is about 6.94 ms. With no frame limit, retain the absolute thresholds and distributions; do not invent a target from the measured average. Record the display rate, frame limit and vertical sync independently.

A freeze that lasts to the end of collection has no following submission. Report its unfinished gap separately, including the time since the last successful submission; never omit it just because no interval closed. The same boundary rule applies to days and interface updates.

### Interface updates and age

Report update-ready events per second, update durations, and intervals between ready events. For each published update, measure ready to its first successful submission; report how many updates were superseded or invalidated without a submission, as well as those still pending at the end. A low handover delay among the surviving updates alone does not show that all updates reached drawing.

Also report **age of the last submitted interface update**: elapsed time since that update became ready. Summarize it over wall time, including periods without submissions, so a freeze cannot produce an artificially good result. Before the first attributable submission, age is unknown. An implementation may integrate the event timeline or sample on a fixed schedule, but must report which method and its resolution.

Age measures update scheduling and handover. It does not prove that every number on screen is current; some windows refresh less often, and an update inside a day may read intermediate state. Record whether the update occurred inside a day and the most recent completed day for context. Do not express interface age solely in game days: interaction must work while paused too.

### Response to an action

Keep two populations separate:

| Path | Actions to exercise | Reported intervals |
| --- | --- | --- |
| Camera | Dragging, wheel zoom and movement keys | Input receipt to camera-state application; receipt to first attributable frame submission |
| Interface and game commands | Opening a panel, changing a selection, requesting a tooltip, pausing time, issuing an order | Input receipt to verified application; application to first attributable frame submission; receipt to that submission |

For actions with both immediate feedback and a later game effect, record them separately. A pressed button is not proof that an order has executed. A tooltip's intentional dwell delay is included in total response and also labeled separately where its boundary is known.

Actions need correlation through receipt, application, update and frame. Define the observable result for each action before timing it. Continuous camera input may be coalesced; report received, coalesced, applied and unresolved samples, and measure receipt to the first frame reflecting the input, not to the end of camera smoothing. Rejected actions and timeouts remain in the report and are excluded from successful-latency percentiles with their counts shown.

Remote request round-trip time is a separate developer measure. It includes transport and queueing, and does not stand in for player input latency. Likewise, a screenshot request answered later cannot supply the timestamp when a result first appeared. If an action cannot yet be correlated with a submitted result, report that endpoint as unmeasured.

## Explaining the cost

Use these measurements to explain an observed change, below the four experience measures:

| Resource or stage | Proposed report |
| --- | --- |
| Main thread | Elapsed simulation work, interface update and preparation, presentation-lock waits, waits for workers, waits for drawing, intentional pacing and unclassified time |
| Drawing thread | CPU-side drawing duration, presentation-lock waits, Present duration, crossing hold-back and intentional pacing |
| Worker threads | CPU seconds, active work and the stages for which the main thread waits |
| Whole process | CPU seconds per wall second in cores; working set and private bytes, with peaks where available |
| Graphics | Dedicated/shared graphics memory and GPU work only when a suitable source is available; otherwise unmeasured |

Thread CPU time and elapsed spans are different measures. A CPU-side drawing duration is not GPU time. Crossings can explain a long gap without consuming CPU for its whole length. Existing dominant-stage attribution is a clue, not proof that the named stage caused the entire delay.

Nested spans must have explicit inclusive/exclusive accounting: a day includes its interface updates, while an exclusive simulation breakdown subtracts them. No breakdown may add the same wait twice. Work on different threads overlaps; do not add their elapsed times and call the sum a player's wait. Aggregate CPU seconds may exceed wall time.

## Scenarios and comparisons

Every standard comparison records the build and features, game baseline, machine, playset and settings, save identity and date, country in view, resolution, interface scale, camera position/zoom, map mode, open windows, display rate, frame limit, vertical sync and visibility. Record other CPU/GPU load and any active profiler. Only the tested change should differ intentionally.

| Segment | Purpose |
| --- | --- |
| Paused, static view | Cost of drawing and interface maintenance with no day work |
| Paused, repeatable camera and interface actions | Drawing and response without simulation contention |
| Running at a fixed normal speed, with the same actions | Whether days keep pace while interaction remains prompt |
| Speed 5, static view and then the same actions | Maximum day throughput and its interaction with drawing and response |
| Month start, year start and scheduled autosave | Long work and its effects on all four measures |

Use an early save and late saves, with 1600, 1700 and 1800 as initial target eras from the available developer saves. Each comparison reloads the same save for each run. Cover at least one full game year for a calendar comparison; use longer runs or dedicated repeats for rare events. Do not infer a reliable year-start percentile from a few year starts.

Proposed starting protocol: a declared, identical warmup; three runs per variant with order alternated; the same calendar endpoints for throughput and the same bounded input sequence for interaction. The game does not necessarily replay identically, so record resulting dates and relevant differences and show the spread across runs. Extend the runs when variation obscures the effect. Do not compare successive eras of one continuing campaign as though their workloads were identical.

Test games are claimed, started hidden and released through [game-test](../../.claude/skills/game-test/SKILL.md). A hidden run measures that mode only: it cannot establish visible presentation or hardware input behavior. A visible interaction/display check requires the owner's request to show the game. Unsupported hidden input paths remain unmeasured, rather than being substituted with remote commands under the same label. Record a result per run in [prototype-log/](../prototype-log/).

## Report rules

- Give metric units, event boundaries, sample count, elapsed duration and missing/failed observations. Missing data is not zero.
- Use nearest-rank percentiles for event intervals: after sorting `n` samples, rank `ceil(p * n)`, starting at one. For time-weighted age, use the cumulative duration distribution instead. Label that distinction. Show the count even when it makes a percentile little more than the maximum.
- Report each run's result, then the median and range across comparable runs. Never average percentiles and label the result as a pooled percentile. Pool only compatible raw observations with explicit weighting.
- Show all four dimensions together for each scenario, followed by resource costs and explanations. Include stall markers and unfinished intervals. Preserve measured totals with stalls included, even when a second breakdown explains them.
- Give absolute values and changes from the baseline. Do not set universal pass/fail limits before repeated measurements establish variance and the owner has chosen acceptable tradeoffs. A missing or failed response cannot count as an improvement.
- Keep historical results under their old definitions. New measurements carry a format/version identifier; the old application's frame count, drawing-function completion count and successful Present count are not interchangeable.

Startup and load retain their named endpoints and cache state. Save duration remains a separate result, correlated with its effects on days, drawing and response. Memory remains a resource measure, not a substitute for these outcomes.

## Current coverage and implementation

| Existing code or document | What can be reused | What is missing for this proposal |
| --- | --- | --- |
| [scenario/perf.rs](../../xtask/src/scenario/perf.rs) | Startup, new-game load, process CPU/memory and a date-based speed-5 rate | Completed-day boundaries, all four distributions, action correlation, save-based paired runs and a shared measurement window |
| [drawing/course.rs](../../core/src/drawing/course.rs) | Prepared/drawn sequences, invalidation and late-gap accounting | Timestamped update associations, first-submission outcomes, distributions and age |
| [drawing.rs](../../core/src/drawing.rs) | Preparation publication, drawing-thread loop and lock/wait counters | Coverage of both presenting threads under one definition; current gap timestamps follow the drawing function's return |
| [framework_ui/hook.rs](../../core/src/framework_ui/hook.rs) | Existing Present wrapper | Verified device submission boundaries/results and association with update and action identifiers |
| [profile](../dev-tools.md) | Sampled thread costs and where the main thread waits | Event timing; samples cannot reconstruct exact action or submission intervals |
| [performance.md](../performance.md) | Historical costs and setup descriptions | New measurements under the definitions above |

Use bounded in-process counters and event records, collected without a remote request per frame or day. Export after the window. Report overflow and lost records; a record limit must never quietly discard the slow tail. Measure collection overhead with collection enabled and disabled, and keep stack sampling out of the primary comparison unless it is the tested variable.

Collection will use the independent developer feature agreed above. The implementer will define bounded in-process collection and a development-only remote control surface for beginning and ending a window and retrieving its report. `perf` will use that feature rather than the full developer tool set, and extend its existing JSON and Markdown reports rather than add a separate report system. Reports must identify the build and cannot present its results as measurements of an uninstrumented player build. The feature must compose with `game_paths`; disabled collection must avoid accumulating records. Exact capability names and record representation are implementation details.

## Parts

- **P1 — Boundaries and collection.** Verify daily completion, update entry/readiness and successful submission on both threads. Define a versioned bounded record, segment boundaries, missing-data handling and overhead measurement. Agree the lasting developer-tool surface before building it.
- **P2 — Days, drawing and interface report.** Produce throughput, distributions, thresholds, update outcomes and wall-time age, plus non-overlapping diagnostic breakdowns. Keep existing historical results and mark unavailable measures explicitly.
- **P3 — Action response.** Establish receipt, application and attributable-submission boundaries for a small repeatable set of camera, interface and command actions. Report coalescing, intentional delays, failures and unresolved actions.
- **P4 — Repeatable comparisons.** Extend the performance scenario to the scenario matrix, paired saves and repeated runs; report all four measures together and publish the first baseline in `performance.md` with its run records.

P1 precedes P2 and P3; P4 combines them. Each part is done when its boundaries and limitations are documented, accounting is tested with deterministic timelines (including nested updates, repeated/superseded frames, pauses, failures, missing records and end-of-window stalls), required repository checks pass, and claimed runtime coverage has been exercised in an owned test game and recorded. No new performance claim follows from this plan alone.
