# 2026-11-14 Collection overhead, pair 3, off

The sixth and final run followed [pair 3 on](2026-11-14-performance-overhead-3-on.md), using the [same overhead protocol](2026-11-14-performance-overhead-1-on.md): identical instrumented build, hidden new Ming campaign, inherited settings and playset, 5 s paused and 30 s at speed 5 after warmup. No other game was running at its start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.collect false --perf.paused_seconds 5 --perf.running_seconds 30 --out target/scenarios/perf-overhead-3-off
```

The run passed. Both in-process metric fields were null, with no completed-day rate reported. External dates advanced from 1444.11.11 to 1446.5.2 in 30.083 s: 1,071.0 date-days/min. CPU was 0.644 cores paused and 3.249 running. The camera matched all preceding runs at x=2995, y=1428, height=310, province 112.

Claim `great-turing-perf-perf-pvclw0` was released without a crash. Reports are in `target/scenarios/perf-overhead-3-off/`, with the runner log beside the folder. After all six runs, `game status` reported zero held test games and zero running test games. All six retained startup reports named the same core image size and modification timestamp, the same two Harmony extensions, 3840x2160 and unlimited framework drawing. The comparison is in [performance.md](../performance.md#collection-onoff-comparison).

In-process distributions, the instrumented build's fixed cost compared with a player build, physical display latency and action response were not measured in this collection-off run.
