# 2026-11-12 Checking the first event option's texture with the published core's sources (tested by me)

The player reported that an event's first option appeared to show other interface textures again after the picture-freeze change. This run separates the published 0.1.3 core from master, which already has an additional event-window change. It reproduces a text-element initialization race, but not the player's exact texture corruption. No new product change was made.

## Build and setup

- Worktree based on master `b484480a`. `core/src/drawing/crossings.rs` was temporarily restored from `v0.1.3` (`7f694b85`), removing the later event-showing crossing. `git diff --exit-code v0.1.3 -- core` then passed: the entire core source tree matched that tag. The picture-freeze change stayed in place. This is a developer build from those sources, not the Workshop's packaged DLL.
- Built with `cargo build --release --features eu4-harmony-core/dev-test-games,eu4-harmony-core/dev-call-trace`. The copied core's SHA-256 was `d14aac041f40ec1c2fd08c7a3828a19dec3b5300e8c970e450560a24ec6b6411`.
- Existing call tracing timed `0x1550d10` (a text element given its text) and `0x1550e90` (the text laid out into its texture), with `calls = 20`, `times = true`, no fields. No new tracing code, setting or diagnostic capability was added. Timing changes the run's scheduling, so this does not measure the race's frequency in the player build.
- Hidden test game `event-regression`, the same copied playset and late save as [the master check](2026-11-12-event-texture-master-check.md). The unsuccessful temporary test mod was no longer enabled. Loaded `observer-1700.1.1` as the Ottomans, took a baseline copy of the trace after loading, and ran at speed 5 for six minutes. Three 50% screenshots for each batch of pending questions; each first available option answered, then time resumed.

## Result

The run listed and answered 41 questions in 37 captured batches, with 111 screenshots. The first batch was captured at 1700.3.25, the last at 1716.5.2. The first screenshot from every batch was inspected, including a contact sheet retained as local evidence outside the repository. No other panel, top bar, missing button, or whole frame appeared in the first option in those inspected images. These are samples, not a continuous screen recording, and a question behind another window may not have been visible in its batch's screenshots.

After the baseline, the trace contained 5,385 calls to the two functions. Seven calls to `0x1550d10` were on the drawing thread, each inside a main-thread `0x1550e90` call for the same object. In all seven, the main thread's call returned after the drawing thread's. The dates were 1702.3.21, 1704.12.23, 1708.4.4, 1711.5.30, 1712.12.31, 1713.4.9 and 1716.4.28.

One example, in microseconds from the trace's start, with the same object in all rows:

| Function | Thread | Start | End |
| --- | --- | ---: | ---: |
| `0x1550e90` | main | 152446799 | 152451402 |
| `0x1550d10` | drawing | 152450188 | 152450455 |

This is the overlapping text-element initialization signature described in [the earlier event-option investigation](2026-11-12-event-option-text-left.md). The timing records do not contain the caller, the text or the option's identity; this run does not independently prove that each overlapping object was an event option. It also does not show the order of the writes inside the calls. In particular, a later function return alone is not proof of which texture was stored last.

## What the versions establish

- The published 0.1.3 core still permits the observed text-element overlap. A negative screenshot result is not evidence that its interface is free of races.
- Master adds the event-showing crossing at `0x828c00`. The earlier investigation traced 40 events without it and 60 with it, finding no drawing-thread texture creation in the latter run. That is evidence for the event activation change, not a reproduction of another interface's pixels in the first option.
- The freeze change changes whether renderer calls made with the presentation lock already held invalidate the prepared frame; it retains the renderer-table redirects and lock acquisition. The original render-target read/set race's protection was not removed. Neither source inspection nor this run establishes that the freeze change caused the player's current symptom.
- The earlier event-activation report was already from a player on 0.1.2. This is a remaining defect whose fix is absent from the Workshop's 0.1.3, rather than proof that it first appeared in 0.1.3.

## Limits and cleanup

The player's exact corrupted option and playset were not available for this run. The console-triggered and failed test-mod attempts do not count as daily-event coverage. No new change was guessed from the timing signature, and the freeze change was not rolled back. The current master's trace was not repeated here; its existing 60-event evidence and this run's [16 captured master batches](2026-11-12-event-texture-master-check.md) remain separate results.

The master version of `crossings.rs` was restored and `cargo build --release --features eu4-harmony-core/dev-test-games` passed again. The temporary call-trace options, scripts, test mod, raw screenshot copies and trace copies were removed after recording the evidence. These run notes are retained in the repository; contact sheets are retained as local evidence outside it; the claim tool keeps the original logs for seven days at `claims/event-regression-20261112-042423/kept` under the development data folder.

`game release --owner event-regression` reported released, all four processes ended with exit code 1 and no crash. `game status` then listed no claim owned by `event-regression`. The installed framework, the player's files and other claims were not changed. No code was committed, pushed or published.
