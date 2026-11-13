# 2026-11-14 Collection overhead, pair 3, on

The fifth of six runs began the final on/off pair, using the [same overhead protocol](2026-11-14-performance-overhead-1-on.md): identical instrumented build, hidden new Ming campaign, inherited settings and playset, 5 s paused and 30 s at speed 5 after warmup. No other game was running at its start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.collect true --perf.paused_seconds 5 --perf.running_seconds 30 --out target/scenarios/perf-overhead-3-on
```

The run passed. Both windows were complete, reached their deadlines and dropped no events. The paused window recorded 722 successful submissions and 361 updates. The running window recorded 534 completed days (1,068.0/min), 3,831 successful submissions and 755 updates, with no failed submissions. External dates advanced from 1444.11.11 to 1446.5.2, giving 1,061.2 date-days/min. CPU was 0.735 cores paused and 3.625 running. The camera again matched x=2995, y=1428, height=310, province 112.

Claim `great-turing-perf-perf-byd13b` was released without a crash. Reports are in `target/scenarios/perf-overhead-3-on/`, with the runner log beside the folder. This is one observation in the repeated external-rate comparison, not a standalone estimate of overhead. Player-build overhead, display latency and action response were not measured.
