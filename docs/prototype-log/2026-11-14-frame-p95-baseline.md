# 2026-11-14 Submission-interval P95 investigation baseline

This run investigated the interval between successful submissions on source `8d5358f1`, current with origin/master before the run. It used only `dev-performance` and `dev-test-games`, a hidden new Ming game, the inherited playset, and no other game running at startup. The scenario built the core before starting its game. A concurrent Ghidra whole-program import overlapped part of this run and later timed out; this is a loaded-machine observation, not a quiet-machine baseline or a controlled optimization comparison. The [repeat after analysis stopped](2026-11-14-frame-p95-repeat.md) is kept separately.

```powershell
cargo xtask scenario perf --owner loving-wiles-p95 --out target/scenarios/frame-p95-research
```

Setup: 3840x2160 borderless, `vsync=yes`, `multi_sampling=8`, framework `drawing_unlimited=yes`. The game's settings file says `refreshRate=60`, while the framework detected a 144 Hz display; neither field is evidence of physical display timing in this hidden run. Camera: x=2995, y=1428, height=310, province=112. Terrain, trees, shadows and reflections were enabled. Both Regional Combat and Convenience Tools were in the inherited playset. Settings and the playset were copied into the report's `setup/` before claim release.

After the scenario's ten-second paused warmup it measured 30 seconds paused and 120 seconds at speed 5. Both windows reached their deadlines, retained their full duration, dropped no records and had no failed submissions; every successful submission came from the drawing thread.

| Measure | Paused | Speed 5 |
| --- | --- | --- |
| Successful submissions | 4,315 | 15,810 |
| Submission interval samples | 4,314 | 15,809 |
| Submission interval P50 / P95 / P99 / max, ms | 6.946 / 7.418 / 8.133 / 15.084 | 6.965 / 9.911 / 31.249 / 168.426 |
| Present-wrapper call mean / P95 / P99 / max, ms | 5.443 / 6.208 / 6.765 / 13.823 | 4.140 / 6.309 / 8.547 / 29.390 |
| Completed gaps over 50 / 100 ms | 0 / 0 | 70 / 14 |
| Interface updates/s | 71.933 | 23.717 |
| Time-weighted interface age P95 / P99, ms | 20.094 / 20.767 | 63.424 / 95.801 |
| Completed days/min | 0 | 929.0 |
| CPU, cores | 0.748 | 3.368 |

There were ten local saves, about 105–121 ms each, retained in every inclusive metric. The final unfinished submission gap was 42.729 ms; the final prepared update was 2.643 ms old and still pending. The five year starts cannot establish a stable year-start percentile.

## Drawing-log clues

Twelve ten-second log buckets ending from 05:09:43.478 through 05:11:33.551 contained 313 late completed drawings: 109 attributed to hold-back, 58 to the lock before drawing, 2 to drawing itself, 60 to Present, 61 to the lock after Present, and 23 to other time. The buckets only approximately cover the performance window. A native draw excluding Present and the subsequent lock wait averaged 2.0–2.8 ms per bucket. The drawing thread's lock waits totaled 862–2,110 ms per bucket.

The first crossing most often named for held-back late drawings was a message popup (`0x8c1430`), with the interface's monthly refresh (`0x81e280`) and event-window activation (`0x828c00`) also present. These identities come from `cargo xtask what` and the existing [drawing research](../research/drawing-beside-the-day.md), not newly inferred symbols.

The classification is **not a P95 decomposition**. `core/src/drawing/course.rs::Gaps` measures whole-drawing-return gaps, uses twice the detected refresh interval even with framework limiting disabled, gives hold-back precedence, and otherwise selects the largest elapsed stage. `CrossingsTracker` charges the time from the first invalidation until preparation to that first crossing; it does not time the crossing function itself. One bucket's large port-model entry is not proof of expensive port-model calls. Lock-after-Present time lies after one measured successful submission and can delay the next one.

The 168.426 ms maximum interval cannot be explained solely by the final Present call, since no measured Present call exceeded 29.390 ms and there were no failures. This does not assign the remaining time to one stage or justify subtracting P95s. The 70 intervals above 50 ms are only about 0.44% of interval samples: reducing rare freezes alone need not improve P95.

## Evidence and cleanup

Reports: `target/scenarios/frame-p95-research/report.json`, `report.md`, `drawing-log.txt`, and `setup/`. The scenario released `loving-wiles-p95-perf-2yfq3a`; process 83108 ended with code 1 and no crash. Its retained framework log was under `%LOCALAPPDATA%/eu4-harmony-dev/claims/loving-wiles-p95-perf-2yfq3a-20261114-050758/kept/eu4-harmony-logs/core.log`.

No optimization was applied. GPU time, displayed-frame timing, exact slow-frame stage attribution, late-game behavior and collection overhead were not measured. System-wide CPU load was not recorded in this run, and the concurrent analysis prevents a quiet-machine claim.
