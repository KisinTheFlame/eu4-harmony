# 2026-11-14 Performance collection startup refused

The first short run of the new `scenario perf` used the in-progress source in worktree `great-turing-f12bde`, with only `dev-performance` and the test-game pipes. The intended course was a new Ming game, three seconds paused and ten seconds at speed 5. The claim inherited the player's settings and playset and started hidden; no other game was running at its start.

Command:

```powershell
cargo xtask scenario perf --owner great-turing-perf --perf.paused_seconds 3 --perf.running_seconds 10 --out target/scenarios/perf-smoke
```

The game did not reach the main menu. Startup refused overlapping observations:

```text
performance: local save at 0x5c6500 and local save at 0x5c6500 change the same bytes
```

The background-compression speed-up already observes local saves. The new collection tool had tried to install another observer on the same entry. The correction shares one observer at each entry: in performance builds the collection tool registers it, and its callback runs compression work only when the speed-up is enabled. Player builds keep the speed-up's registration. The addresses remain declared in the saving module.

The scenario classified the exit as a crash (framework startup exit code 3); no game crash folder was produced. No performance window ran, and this run establishes no rate, latency, stability or overhead result. The report is in `target/scenarios/perf-smoke/report.json` and `report.md`.

Claim `great-turing-perf-perf-f10s3d` was released by the scenario runner, which retained the failed startup's logs. The source correction requires a new run.
