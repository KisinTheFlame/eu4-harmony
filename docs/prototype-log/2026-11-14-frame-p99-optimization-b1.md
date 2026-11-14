# 2026-11-14 Submission P99 scope comparison B1 (tested by me)

This is the prototype run B1 of the sequential A1/B1/B2/A2 [bounded lock-order experiment](../plans/frame-p99-lock-scope-validation.md). The [research note](../research/frame-drawing-p99.md) compares its result with the other runs; one window alone does not establish an optimization effect.

## Setup

- Base source `8d5358f1`, release with `dev-test-games,dev-performance`; the prototype's complete patch is retained locally with the evidence.
- Loaded core SHA-256: `a6423c8eee52f7797c8eee654163fc2e354ed50e470084d4fddacc67ec68d642`.
- Claim owner `loving-wiles-p99-b1`. Claim identity and process are in `claim.json` in the evidence folder.
- Hidden new Ming campaign, seed 123456, frozen inherited playset and settings: 3840x2160, borderless, 144 Hz, vsync, multisampling 8, framework drawing without a frame limit.
- Pause and settle 10 s; collect 10 s paused. Resume at speed 5 and settle 5 s; collect 120 s running. No compilation, binary analysis or screenshots during either collection window.
- Running camera: x=2995, y=1428, height=310, province 112. Running date bracket: 1444.12.30 to 1448.7.1. These dates bracket control requests, not exact completed-day events.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days | Completed days/min | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,437 | 6.943 / 7.334 / 7.904 / 10.075 | 0 | 0.0 | 23.73% |
| running, 120 s | 14,091 | 6.942 / 20.539 / 37.767 / 122.439 | 1275 | 637.5 | 30.49% |

Both windows reached their deadlines, have one context segment, zero dropped events and zero failed submissions. The formal JSON retains Present durations, interface age, long-gap counts, unfinished gaps and warnings. The metric is the interval between successful Present-wrapper returns, not GPU completion or physical display latency.

## Evidence and limits

Local evidence: `target/scenarios/frame-p99-optimization/b1/`, including both formal reports, loaded build identity, settings, claim/start/release command output and retained core log. `after.png`, when present, was requested only after the running window ended. Its visual interpretation is recorded in the research comparison.

Equal seed and wall time do not replay identical work: running warmup and different throughput can reach different dates. Background machine load is not controlled. This early-game view does not validate all saves, cameras, dialogs or exceptional paths. The two builds contain the same ordinary performance collector; neither contains the preceding investigation's stage probes.

## Release

```text
released loving-wiles-p99-b1's test game, held since 2026-11-14 06:35
  eu4.exe 59672 had ended by 06:38:26: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-b1-20261114-063527\kept
```

Postflight status:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player's files were not changed. Final prototype-source and build cleanup is recorded in the research comparison.
