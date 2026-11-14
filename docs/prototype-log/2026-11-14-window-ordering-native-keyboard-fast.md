# 2026-11-14 Native drawing with rapid keyboard requests

This run repeated the [threaded keyboard course](2026-11-14-window-ordering-keyboard.md) with native main-thread drawing. Branch `claude/zealous-carson-b8374d`, base `8d5358f1`, uncommitted ordering implementation including scrollbar grouping and additional dependency checks. Release features: `dev-test-games,dev-test-switches`; temporary checkout `dev.toml`: `[test-switches] game-paths = ["drawing-thread"]`. The hidden test used `qa-window-ordering`, a fresh France game paused on 1444.11.11, and only its own pipe.

Claim `ordering-native-keys-1de2b155`, created 16:15:49, process 92604, main thread 102576. Directory: `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-native-keys-1de2b155-20261114-161549`. The core logged `drawing: on the main thread, no limit`.

## Failed acceptance

A single driver called `time.pause`, read `question.list`, answered the original France introduction event, then sent Escape, Escape and Return, calling `popup.list` after each key. It did not wait for individual extension close callbacks. The first two reads retained `Ordering marker 1`; the last returned an empty list rather than the expected success marker.

The extension log records:

- 16:17:06.612: opened first, pop-up, last.
- 16:17:06.919: text and repeated open applied.
- 16:17:44.797: last closed; first reopened.
- 16:17:44.917: `Return ran before both expected window closes`, an assertion failure in the pop-up callback.

A later screenshot still showed `z_first`, with `a_last` and the marker pop-up absent, and the framework's extension-error notice present. Thus the missing close was not merely a delayed log read. This run **failed** the intended course.

The implementation's diagnostic checks reported five verified placements, 140 stable paired refreshes and 61 matching independent traversals, with zero `FAILED`, zero deferred refresh and no ordering panic. Those checks do not turn the failed course into a pass.

## Interpretation and cleanup

The driver did not establish whether the original question's native interface cleanup or the extension's reopen phase had completed before each subsequent key. The remote input barrier promises a presentation after the input, not completion of every downstream extension callback. Source inspection found that the fixture's programmatic false/true notifications are queued separately, so there is no demonstrated path by which its `reopening` flag discards a later genuine keyboard close. The exact earlier key recipient was not traced. Neither a drawing-mode defect nor an input-ordering defect is established by this failure alone; a separate run must observe each phase before issuing the next key.

Released at 16:19:31, exit code 1, no crash reported. Logs remain under `kept\eu4-harmony-logs` in the claim directory. No pointer clicks, player-process input or installation occurred. Pointer activation, same-native-batch behavior, the army picker and scrollbar movement remain unverified.
