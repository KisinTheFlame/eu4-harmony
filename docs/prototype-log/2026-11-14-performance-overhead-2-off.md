# 2026-11-14 Collection overhead, pair 2, off

The third of six runs began the reversed-order pair, off then on. It used the [same overhead protocol](2026-11-14-performance-overhead-1-on.md): identical instrumented build, hidden new Ming campaign, inherited settings and playset, 5 s paused and 30 s at speed 5 after warmup. No other game was running at its start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.collect false --perf.paused_seconds 5 --perf.running_seconds 30 --out target/scenarios/perf-overhead-2-off
```

The run passed. Both in-process metric fields were null, with no completed-day rate reported. External dates advanced from 1444.11.11 to 1446.4.4 in 30.095 s: 1,014.8 date-days/min. CPU was 0.731 cores paused and 3.378 running. The camera matched the first pair at x=2995, y=1428, height=310, province 112.

Claim `great-turing-perf-perf-rxn9ar` was released without a crash. Reports are in `target/scenarios/perf-overhead-2-off/`, with the runner log beside the folder. The differing rate from the first off run is retained as run-to-run variation; this one result does not identify collection overhead. In-process distributions, player-build overhead, display latency and action response were not measured.
