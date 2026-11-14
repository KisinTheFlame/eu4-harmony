# 2026-11-14 Safe-boundary validation diag-d (tested by me)

This is the mechanism recheck (not a matched speed comparison with earlier builds having fewer scopes) run diag-d of the [safe-boundary validation](../plans/frame-p99-safe-boundary-validation.md). The [boundary research](../research/frame-drawing-p99-safe-boundary.md) records the native safety argument and the measured comparison.

## Setup

- Base `8d5358f1`, release with `dev-test-games,dev-performance`. Loaded core SHA-256 `a7c81e96dad9b7f81494e62b544813eef13cd1d5c54bfc2a9a5313d304cf7367`, build `diagnostic-final`, early release `True`.
- Hidden test game, owner `loving-wiles-p99-bound-diag-d`, held since 2026-11-14 16:05; process 104108. Seed 123456, new Ming campaign, byte-identical inherited settings/playset, 3840x2160 borderless, vsync 144 Hz, multisampling 8, drawing on its own thread without the framework frame limit.
- Paused settling for ten seconds; introduction closed with Escape and the sole `flavor_mng.42` option answered through the capability. Empty question list and a visible Beijing province 1816 view at height 310 verified before collection. A screenshot is checked outside collection; its approval file records any initial tooltip. Later events can open without being cleared inside the running window.
- Ten seconds paused, then five seconds of speed-5 settling and 120 seconds running. No compilation, binary analysis or screenshots during collection. A Toolhelp process snapshot every five seconds records other `eu4.exe` processes without controlling them; brief processes between samples can be missed.

## Results

| Window | Intervals | P50 / P95 / P99 / max, ms | Completed days/min | Submissions/s | System CPU busy |
| --- | ---: | --- | ---: | ---: | ---: |
| paused, 10 s | 1,439 | 6.937 / 7.709 / 7.954 / 12.911 | 0.0 | 144.000 | 18.57% |
| running, 120 s | 16,223 | 6.936 / 7.896 / 12.726 / 111.614 | 617.5 | 135.200 | 28.01% |

Completed windows reached their deadlines with one context segment, zero lost events and zero failed submissions. These are successful Present-wrapper-return intervals, not GPU completion or monitor latency. External-game presence was observed in: none of the recorded samples. Background machine work remains uncontrolled; equal seed and wall duration do not produce identical daily progress. A single window does not establish gain or all-path safety.

Run error retained:

```text
Traceback (most recent call last):
  File "D:\Workspace\eu4-harmony\.claude\worktrees\loving-wiles-f0129f\target\scenarios\frame-p99-boundary\run.py", line 155, in <module>
    collect(folder,owner,'running',120,pid,diagnostic)
  File "D:\Workspace\eu4-harmony\.claude\worktrees\loving-wiles-f0129f\target\scenarios\frame-p99-boundary\run.py", line 97, in collect
    assert 'events_dropped=0 probes_dropped=0' in first,first
           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
AssertionError: DEBUG-P99 elapsed=120000000 events_dropped=0 probes_dropped=1916959 initial=Context { session: 1, speed: 5, paused: false, target_us: None }


```

## Evidence and release

Local evidence: `target/scenarios/frame-p99-boundary/diag-d/`. Formal reports, process-presence samples, settings, loaded build identity, startup/exit logs and screenshots are retained. The running raw trace overflowed its probe buffer; no complete attribution replay or post-window screenshot is claimed for this run.

```text
released loving-wiles-p99-bound-diag-d's test game, held since 2026-11-14 16:05
  eu4.exe 104108 had ended by 16:09:13: ended by game stop, a release or a run (exit code 0x1)
  the game did not crash
  kept: C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\loving-wiles-p99-bound-diag-d-20261114-160546\kept
```

Postflight:

```text
0 test games held, 0 of them running, 4 at most
```

The installed framework and player files were not changed. Temporary runtime source and build cleanup are recorded with the experiment, separately from this run.
