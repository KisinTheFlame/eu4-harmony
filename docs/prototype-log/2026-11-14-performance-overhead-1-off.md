# 2026-11-14 Collection overhead, pair 1, off

The second of six overhead runs followed [pair 1 on](2026-11-14-performance-overhead-1-on.md), using the same instrumented build with collection disabled. It restarted a hidden new Ming game with the inherited settings and playset, the same warmup, 5 s paused and 30 s at speed 5. No other game was running at its start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.collect false --perf.paused_seconds 5 --perf.running_seconds 30 --out target/scenarios/perf-overhead-1-off
```

The run passed. Both `metrics` fields were null and no completed-day rate was reported, as collection was off. External dates advanced from 1444.11.11 to 1446.5.25 in 30.084 s: 1,116.9 date-days/min. CPU was 0.725 cores paused and 3.598 running. The camera matched the preceding run at x=2995, y=1428, height=310, province 112.

Claim `great-turing-perf-perf-sc19rw` was released without a crash. Reports are in `target/scenarios/perf-overhead-1-off/`, with the runner log beside the folder. This comparison uses external date rates in both states, not the on run's completed-day rate. It does not measure distributions while collection is off, or the fixed cost of the instrumented build. The remaining pairs are needed before interpreting the difference.
