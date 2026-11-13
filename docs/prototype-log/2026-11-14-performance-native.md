# 2026-11-14 Performance collection on main-thread drawing

This run exercised the performance implementation in worktree `great-turing-f12bde`, based on `d4b10fd4`, after the save-observer consolidation and the fixes for pre-window Present returns and same-session update lifetimes. The build enabled `dev-performance`, `dev-test-games` and `dev-test-switches`; `drawing-thread` and `background-compression` were left to the game. The startup report confirmed both switches and 2,360 matching dependency points. It used baseline 1.37.5, a new Ming campaign, the inherited player settings and playset, and hidden presentation. Regional Combat and Convenience Tools were loaded as Harmony extensions, with Simplified Chinese chosen by the framework. No other game was running at the start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.game_paths drawing-thread,background-compression --perf.paused_seconds 3 --perf.running_seconds 15 --out target/scenarios/perf-native
```

Both windows reached their exact deadlines, retained all their events and reported complete collection. Every successful submission came from the main thread, and every submission was attributed to an update:

| Measure | Paused, 3 s | Speed 5, 15 s |
| --- | --- | --- |
| Retained events / dropped events | 861 / 0 | 1,245 / 0 |
| Completed days | 0 | 249 |
| Successful submissions, all main-thread | 430 | 371 |
| Failed / unattributed submissions | 0 / 0 | 0 / 0 |
| Ready updates | 431 | 371 |
| Submitted / superseded / invalidated / pending updates | 430 / 0 / 0 / 1 | 371 / 0 / 0 / 0 |
| Submission interval P95 / P99 / max, ms | 7.761 / 9.690 / 12.775 | 63.724 / 82.918 / 132.072 |
| Time-weighted submitted-update age P95 / max, ms | 12.418 / 20.044 | 55.384 / 135.021 |
| Unknown update-age time, ms | 6.854 | 49.775 |

The local-save markers were present with background compression disabled: 4.105–4.223 s and 13.993–14.086 s into the running window. The game advanced externally from 1444.11.11 to 1445.7.23; the exact window counted 996.0 completed days/min. The run passed, and the runner released claim `great-turing-perf-perf-higp3u`; process 68020 ended with code 1 through the runner and no crash. Reports are in `target/scenarios/perf-native/report.json` and `report.md`.

This validates the alternate observer registration and main-thread submission attribution. It is not a controlled performance comparison with the drawing thread: the two short checks had different lengths and both drawing and compression changed. Native readiness is measured at drawing entry, so its readiness-to-submission duration includes native preparation work. Ironman saves, reset handling, visible display latency and action response were not exercised.
