# 2026-11-14 Completed-day, submission and interface baseline

This run used the first performance implementation in worktree `great-turing-f12bde`, based on `d4b10fd4`, after the boundary fixes and [main-thread path check](2026-11-14-performance-native.md). The build enabled only `dev-performance` and `dev-test-games`. It started a new Ming campaign on baseline 1.37.5, hidden, with the player's inherited settings and playset; Regional Combat and Convenience Tools were the two loaded Harmony extensions. The framework selected Simplified Chinese. The game logged 3840x2160 and an NVIDIA GeForce RTX 4070 Ti SUPER; the framework logged drawing on its own thread with no limit. There was no declared framework frame target, and no other game was running at the start.

```powershell
cargo xtask scenario perf --owner great-turing-perf --out target/scenarios/perf-baseline
```

The menu took 25.9 s and starting the campaign 4.5 s. After ten seconds of paused warmup, the scenario collected 30 s paused and 120 s at speed 5. Both windows reached their exact in-process deadlines, retained the full duration, reported complete collection and dropped no events. The external observation ran from 1444.11.11 to 1450.6.10 in 120.265 s. The camera was fixed by the inherited new-game setup and recorded in the JSON; no interaction sequence was played.

| Measure | Paused, 30 s | Speed 5, 120 s |
| --- | --- | --- |
| Retained events | 8,644 | 25,430 |
| Completed days / per minute | 0 / 0 | 2,031 / 1,015.5 |
| Successful submissions / per second | 4,321 / 144.033 | 15,459 / 128.825 |
| Main-thread / failed submissions | 0 / 0 | 0 / 0 |
| Submission interval P50 / P95 / P99 / max, ms | 6.934 / 7.329 / 7.967 / 17.215 | 6.956 / 10.019 / 38.034 / 150.687 |
| Completed gaps over 50 / 100 ms | 0 / 0 | 78 / 14 |
| Ready updates / per second | 2,162 / 72.067 | 2,943 / 24.525 |
| Ready updates inside days | 0 | 749 |
| Ready interval P95 / P99 / max, ms | 14.325 / 14.971 / 19.726 | 62.939 / 76.513 / 142.860 |
| First-submission delay P95 / P99 / max, ms | 7.190 / 7.938 / 17.178 | 9.065 / 20.056 / 52.252 |
| Submitted / superseded / invalidated / pending updates | 2,162 / 0 / 0 / 0 | 2,775 / 0 / 168 / 0 |
| Time-weighted update age P50 / P95 / P99 / max, ms | 13.786 / 20.055 / 20.690 / 30.851 | 28.519 / 62.138 / 90.098 / 163.525 |
| Unknown update-age time, ms | 20.822 | 152.762 |
| Unattributed submissions | 2 | 2 |
| Process CPU, cores | 0.656 | 3.494 |
| Working set / private MB | 2,615 / 4,000 | 2,805 / 4,205 |

Daily call durations included nested interface work and worker waits. The 1,964 ordinary days had P95 81.984 ms; 61 month starts had P95 397.363 ms; six year starts had P95 501.432 ms. The six year-start observations do not establish a stable percentile. Overall daily P95 was 239.128 ms, and completion-interval P95 was 256.034 ms. At the deadline, a daily call was 28.182 ms under way and the last successful submission was 5.247 ms old; those unfinished intervals were retained separately.

Eleven local saves were marked in the running window, with their entry-to-return durations about 97–131 ms. Their effects remain in all inclusive statistics. The JSON contains each marker's exact time. With no declared target, target-relative long-gap counts stayed unknown instead of using an inferred frame rate.

This is a baseline for the instrumented hidden new-game setup, not a measurement of the player build or monitor display. In this run, 128.825 successful submissions/s coexisted with 24.525 ready updates/s: repeated drawing of the same update is visible in the separate metrics. It does not establish input response, GPU duration, pure interface-work duration, late-game behavior or collection overhead. Collection-on/off repeats are separate runs.

Reports are in `target/scenarios/perf-baseline/report.json` and `report.md`. The runner released claim `great-turing-perf-perf-kykxdn`; process 92796 ended through the runner with code 1 and no crash. The retained startup report verified 2,519 dependency points before play.
