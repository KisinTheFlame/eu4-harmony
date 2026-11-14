# Early release is followed immediately by another lock-order acquisition

This investigation follows the [first submission-P99 optimization experiment](frame-drawing-p99.md#bounded-ownership-experiment), which found no repeatable gain from ending presentation-lock ownership at the stream shared-state getter's normal return. It establishes whether that release actually happens and what blocks subsequent drawing. The findings guide the drawing thread's lock-order coordination; they do not introduce a production synchronization change.

Status: four diagnostic games completed on 2026-11-14. Early release was observed, followed by an adjacent unscoped acquisition of the same game lock. No stable end-to-end gain is established. Temporary runtime source was restored; final artifact cleanup is recorded with the local evidence.

## Method and identity

One release core, SHA-256 `cb72fb13edab185fb05bfa13155713b6f201ad31362307593b9ee0564a05f637`, was used for all four games. It has `dev-test-games,dev-performance` and the same temporary stage/scope instrumentation in both modes. A disposable process-start variable, `EU4_HARMONY_P99_EARLY_RELEASE`, selects the old lifetime (0) or the previous bounded-scope prototype (1). This variable is removed with the prototype and is not a supported setting. Each loaded copy's hash and startup mode were checked.

The raw recorder uses the ordinary performance window's clock and retains getter entry, normal return, exit state, scoped acquisition, actual early release and promotion/fault markers, as well as main-thread lock acquisitions/releases, drawing waits and preparation/invalidation transitions. State bits identify existing ownership, daily work, crossing lifetime and scoped order-only ownership. Only active bounded windows record events; exports occur after collection. The original replay verifies completed interval counts and P99 against the formal reports, rejects overflow and failures, and partitions elapsed time without adding overlapping ownership durations twice.

All games were hidden claims with seed 123456, fresh Ming campaigns and byte-identical settings/playset: 3840x2160 borderless, vsync at 144 Hz, multisampling 8, drawing on its own thread without the framework's frame limit. After paused settling and screenshot verification, each collected 10 seconds paused, resumed at speed 5, settled five seconds and collected 120 seconds running. No compilation, binary analysis, screenshots or control inputs ran during collection.

The first pair retains the original introduction window and unexplored-map camera. The second pair closes the introduction with Escape, answers the sole `flavor_mng.42` option through `question.answer`, verifies that the question list is empty, and fixes the camera at visible Ming province 1816 (Beijing), x=4629, y=1325, height=310. Screenshots establish an unobstructed starting map. Later game events are not dismissed during collection and are visible in the ending screenshots; this is not a continuously unobstructed map benchmark.

## Formal running-window results

| Run, execution order | P95 / P99 / max, ms | Completed days/min | Submissions/s | Interface age P95, ms | System CPU busy |
| --- | --- | ---: | ---: | ---: | ---: |
| [Original view, baseline](../prototype-log/2026-11-14-frame-p99-release-old-a.md) | 14.552 / 31.259 / 118.254 | 639.0 | 127.200 | 71.381 | 31.54% |
| [Original view, prototype](../prototype-log/2026-11-14-frame-p99-release-old-b.md) | 20.415 / 37.543 / 139.137 | 594.5 | 123.100 | 76.298 | 36.13% |
| [Beijing view, prototype](../prototype-log/2026-11-14-frame-p99-release-map-b.md) | 14.461 / 40.216 / 132.192 | 534.5 | 119.542 | 85.571 | 47.05% |
| [Beijing view, baseline](../prototype-log/2026-11-14-frame-p99-release-map-a.md) | 16.358 / 44.727 / 168.457 | 547.5 | 112.192 | 91.174 | 46.13% |

Paused P99, in the same order: 7.885, 7.948, 8.145 and 8.150 ms. All eight windows reached their deadlines, have one context segment, zero failed submissions, zero dropped events and no raw-probe overflow. Raw replay matches all formal interval counts and P99 values.

The original-view prototype has P99 +20.10% and completed-day rate -6.96% against its baseline. The Beijing-view prototype has P99 -10.09% and day rate -2.37%. These diagnostic results do not establish a stable gain. A second session's test game, process 46848, appears in the status after the Beijing prototype and before the Beijing baseline; its full overlap with sampling was not recorded, and it was left alone. Whole-machine CPU load also differs. Equal instrumentation adds different total cost when the prototype causes more acquisitions and therefore more captured stacks. Equal seeds and wall duration do not replay identical daily work.

## The first scope really releases

| Measurement | Original baseline | Original prototype | Beijing prototype | Beijing baseline |
| --- | ---: | ---: | ---: | ---: |
| Recorded main-thread getter entries | 70,303 | 46,624 | 46,615 | 37,118 |
| Paired acquisitions starting in an unheld outer scope | 1,285 | 1,268 | 1,060 | 1,058 |
| Actual early releases | 0 | 1,268 | 1,060 | 0 |
| Acquired-to-released median, microseconds | 14,084 | 15 | 16 | 14,656 |
| Acquired-to-released P99, microseconds | 37,199 | 33 | 34 | 50,729 |
| Getter-return-to-release median, microseconds | 14,070 | 0 | 0 | 14,643 |
| Getter-return-to-release P99, microseconds | 37,185 | 1 | 1 | 50,710 |

The paired scope-only acquisitions all released early in both prototype windows. No scope promotion, real-crossing promotion or abnormal-return marker was observed in these windows. That does not replace the state tests for exceptional paths. Most getter calls enter with presentation-lock ownership already held and cannot own an early release. The trace therefore proves actual activation and shortened ownership, not merely that the observer was installed.

Microsecond values include recording and stack-capture costs. A zero-microsecond return-to-release duration means both observations fell in the same clock bucket, not literally zero work.

## A second call takes the same lock again

The dominant original acquisition's native return chain includes:

`0x1a0c764 <- 0x1a0c42a <- 0x1a1558 <- 0xa3829 <- 0xb21f4 <- 0x75ba18`.

The prototype's subsequent acquisition instead includes:

`0x1a0c764 <- 0x91a8b <- 0x1a1585 <- 0xa3829 <- 0xb21f4 <- 0x75ba18`.

Both enter the same critical-section object, whose runtime address maps to RVA `0x233cdd0`. These are observed native **return addresses**, not inferred function starts. The previously retained disassembly of stream initialization `0x1a14f0` confirms a call at `0x1a1553` to getter `0x1a0c408`, followed later in the same function by a call at `0x1a1580` to `0x91a70`. The second call returns to `0x1a1585` in that initializer. The address lookup places return address `0x91a8b` inside `0x91a70`. That helper's full body, callees and safe release boundary have not yet been established here; no new early-release observer was added to it.

The first operation opts into early release. The second is outside that scope and retains the old frame-check lifetime. This explains why the first-scope optimization activates without removing the long hold from the surrounding AI work.

| Consecutive acquisition pairs on the AI naval-doctrine chain | Original view prototype | Beijing view prototype |
| --- | ---: | ---: |
| Pairs, same game-lock object | 979 | 784 |
| Release-to-next-acquisition median, microseconds | 1 | 1 |
| Release-to-next-acquisition P99, microseconds | 24 | 23 |
| Second presentation-lock hold median, ms | 12.541 | 16.483 |
| Second presentation-lock hold P99, ms | 39.463 | 44.539 |
| Second presentation-lock hold maximum, ms | 60.724 | 73.478 |

The pairing follows consecutive observed lock-order acquisitions and verifies the same object and both caller chains. The gap is between recorded ownership boundaries; it is not a separate measurement of OS scheduling latency. The baseline usually already owns the presentation lock at the second call, so that call does not create another ownership acquisition there. It becomes visible as a separate long hold after the prototype releases the first one.

## Where the remaining long intervals spend time

The P99 neighborhood remains the ordered 98.5th through 99.5th percentile band of actual intervals. Shares are of the band's elapsed time, with disjoint stages; they are not independent stage percentiles.

| Stage | Original baseline | Original prototype | Beijing prototype | Beijing baseline |
| --- | ---: | ---: | ---: | ---: |
| Lock waiting after Present | 60.61% | 53.72% | 56.54% | 48.22% |
| Lock waiting before drawing | 12.32% | 14.70% | 14.73% | 14.40% |
| Invalid frame outside the waits | 14.79% | 17.40% | 12.77% | 17.30% |
| Map names due outside the preceding stages | 3.94% | 5.97% | 6.40% | 5.90% |
| Drawing before Present | 5.57% | 5.18% | 5.74% | 10.34% |
| Present | 2.74% | 2.98% | 3.79% | 3.81% |

No marked local save overlaps these bands. The two prototype bands still spend 68.42% and 71.27% waiting for the presentation lock. On the dominant original-view chain, the first, now-short hold overlaps only 0.563 ms of the band, while its subsequent acquisition overlaps 2,720.130 ms. This overlap is supporting evidence, not an additive stage share.

An original-view interval near P99 lasts 37.627 ms. Its first scope holds the presentation lock from 62.104683 to 62.104698 s: 15 microseconds. The second acquisition begins at 62.104699 s and lasts until 62.140627 s: 35.928 ms. The previous successful Present returns at 62.108799 s, and the next at 62.146426 s. The second ownership therefore occupies 31.828 ms inside the gap; recorded waiting after Present is 31.835 ms. The remaining partition is 2.680 ms waiting before drawing, 2.301 ms drawing, 0.798 ms Present and 0.013 ms return/residual. No save overlaps it.

The exact P99 samples in the two prototype windows happen to be Present-dominated (34.565 ms of 37.543 ms, and 37.766 ms of 40.216 ms). They must not be mislabeled as lock-dominated. The neighboring actual intervals and their acquisition stacks establish the continuing lock bottleneck; neither one exact sample nor an unrelated stage percentile explains the whole band.

## Decision, evidence and cleanup

The first prototype was too narrowly scoped: it removes one excessive ownership span and immediately exposes the next acquisition's identical lifetime problem. It should not be accepted as a demonstrated optimization. A further change requires reading the second helper and the enclosing stream initialization through their normal and abnormal paths, then choosing a boundary that covers the necessary internal locks while preserving real crossings and existing ownership. That boundary has not been justified or implemented in this experiment.

Evidence is retained locally under `target/scenarios/frame-p99-followup/`: complete diagnostic patch, build identity, check output, formal JSON, raw timelines, ASLR bases, entry-stack grouping, scope pairs, reacquisition pairs and screenshots. `analyze.py`, `summarize.py`, `lock_sources.py` and `reacquisition.py` replay the results. Source and input settings remain English except game-text data. Each game has its own run record linked above.

All four owned games released with exit 1 and no crash. Their core/user folders were removed by claim release; retained logs were copied into the evidence folder. Runtime source was restored, both debug and release core-package artifacts cleared, and the normal test/performance core rebuilt. Final scans and remaining claim contents are recorded in `cleanup.json`. No installed game files, player files, registrations, commits, pushes or releases were changed.
