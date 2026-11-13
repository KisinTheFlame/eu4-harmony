# 2026-11-14 Performance collection smoke check

The first successful short run used the in-progress performance implementation in worktree `great-turing-f12bde`, based on `d4b10fd4`, after correcting the overlapping save observers found in [the refused startup](2026-11-14-performance-startup-refused.md). It built only `dev-performance` and `dev-test-games`, started a new Ming game hidden, and inherited the player's settings and playset. The startup report named Regional Combat and Convenience Tools as the two Harmony extensions, Simplified Chinese as the framework language, and baseline 1.37.5. No other game was running at the start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.paused_seconds 3 --perf.running_seconds 10 --out target/scenarios/perf-smoke-fixed
```

The game reached its menu in 25.7 s and started the campaign in 4.0 s. After the scenario's warmup, both windows reached their exact in-process deadlines and were complete, with no dropped events:

| Measure | Paused, 3 s | Speed 5, 10 s |
| --- | --- | --- |
| Retained events | 863 | 1,991 |
| Completed days | 0 | 166 |
| Successful submissions | 431 | 1,161 |
| Failed / main-thread submissions | 0 / 0 | 0 / 0 |
| Ready updates | 216 | 248 |
| Submitted / superseded / invalidated / pending updates | 216 / 0 / 0 / 0 | 209 / 0 / 38 / 1 |
| Submission interval P95 / P99 / max, ms | 8.110 / 10.338 / 13.845 | 13.215 / 58.832 / 153.426 |
| Time-weighted submitted-update age P95 / max, ms | 20.193 / 27.413 | 75.848 / 157.286 |
| Unknown update-age time, ms | 20.021 | 162.685 |

The running window included a local save from 3.625 to 3.719 s. It retained an unfinished daily call of 3.187 ms, a submission gap of 7.108 ms and one pending update at its deadline. The external date observation was 1444.11.11 to 1445.4.30 in 10.253 s; its rate of 994.9 days/min differs from the exact completed-day rate of 996.0 days/min, as expected from their different boundaries. No frame target was declared, so target-relative gap counts remained unknown.

The scenario passed and released claim `great-turing-perf-perf-igjwdd`; process 35980 ended through the runner with code 1 and no crash. Reports remain in `target/scenarios/perf-smoke-fixed/report.json` and `report.md`. The runner retained the startup report and logs under the claim's `kept` folder.

This short run checks collection and attribution, not stable performance or overhead. A subsequent review found two boundary defects in this version: a Present already under way when collection began could be omitted, and a same-session context change cleared update attribution and age. The single-segment figures above are kept as this run produced them, not promoted to the corrected implementation's baseline. The inherited settings used 3840x2160 on an NVIDIA GeForce RTX 4070 Ti SUPER; the framework log said drawing ran on its own thread with no frame limit. It does not exercise main-thread drawing, disabled background compression, ironman saves, device reset, visible presentation, physical input or action response. The later consolidation of the save observers' declaration requires its own checks and runs.
