# 2026-11-14 Collection overhead, pair 1, on

The first of six overhead runs used the same source and instrumented build as [the complete baseline](2026-11-14-performance-baseline.md): `dev-performance` and `dev-test-games`, no test switches, a hidden new Ming campaign, and inherited player settings and playset. Every run restarts the game. The order is on/off, off/on, on/off, with 5 s paused and 30 s at speed 5 after the same warmup. No other game was running at this run's start. These shorter runs compare collection, not the instrumented build against a player build.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.collect true --perf.paused_seconds 5 --perf.running_seconds 30 --out target/scenarios/perf-overhead-1-on
```

The run passed. Both windows were complete, reached their deadlines, and lost no events. The paused window recorded 719 successful submissions and 359 ready updates. The running window recorded 488 completed days (976.0/min), 3,805 successful submissions and 746 ready updates, with no failed submissions. External dates advanced from 1444.11.11 to 1446.3.18, giving 975.7 date-days/min. CPU was 0.672 cores paused and 3.404 running. The camera was at x=2995, y=1428, height=310, province 112.

Claim `great-turing-perf-perf-8mis40` was released without a crash. Reports are in `target/scenarios/perf-overhead-1-on/`, and the runner log is `target/scenarios/perf-overhead-1-on.log`. One run does not establish overhead; the comparison uses all three runs per state and their spread. Display latency, input response and late-game behavior were not measured.
