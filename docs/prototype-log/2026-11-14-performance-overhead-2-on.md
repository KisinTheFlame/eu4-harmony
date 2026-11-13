# 2026-11-14 Collection overhead, pair 2, on

The fourth of six runs followed [pair 2 off](2026-11-14-performance-overhead-2-off.md), completing the reversed-order pair. It used the [same overhead protocol](2026-11-14-performance-overhead-1-on.md): identical instrumented build, hidden new Ming campaign, inherited settings and playset, 5 s paused and 30 s at speed 5 after warmup. No other game was running at its start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.collect true --perf.paused_seconds 5 --perf.running_seconds 30 --out target/scenarios/perf-overhead-2-on
```

The run passed. Both windows were complete, reached their deadlines and dropped no events. The paused window recorded 718 successful submissions and 359 updates. The running window recorded 534 completed days (1,068.0/min), 3,835 successful submissions and 761 updates, with no failed submissions. External dates advanced from 1444.11.11 to 1446.5.2, giving 1,056.8 date-days/min. CPU was 0.742 cores paused and 3.629 running. The camera again matched x=2995, y=1428, height=310, province 112.

Claim `great-turing-perf-perf-v0ckx5` was released without a crash. Reports are in `target/scenarios/perf-overhead-2-on/`, with the runner log beside the folder. External and in-process rates have different boundaries and are not substituted for each other. This pair's collection-on rate was higher, reversing the first pair's ordering; the complete comparison retains both results rather than selecting one. Player-build overhead, display latency and action response were not measured.
