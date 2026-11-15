# 2026-11-15 Army selection and day advancement after receiver classification

This run verifies the correction for the [derived-window hide failure](2026-11-15-army-panel-derived-window-before.md) against the unchanged actual playset. The observer now classifies the GUI receiver before requiring an ordinary window container, while preserving successful-return visibility updates and strict ordinary container ownership.

## Setup

- Source: `377ed553` plus this change, with `origin/master` synchronized before the run. The temporary receiver/stack diagnostic was removed. Release core built with `eu4-harmony-core/dev-test-games`.
- Same owner and copied user folder as the failing run: `army-panel-crash-d3c3e1`, claim folder `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\army-panel-crash-d3c3e1-20261115-093215`. Corrected process: 48972, hidden and silent, ready at the main menu after 16 seconds.
- All eight enabled Workshop descriptors were retained unchanged. No player, installed framework or Workshop file was modified. The user authorized coordinate input in this test game.
- `menu.start --country TUR` started paused on 1444.11.11; Escape dismissed the separate startup screen. Coordinates use a 1920 x 1080 screenshot with `--scale 50` against the 3840 x 2160 frame.

## Course and result

1. The original reproduction script selected the same first outliner army at `(1845,197)`, read army `231928234038`, and advanced one day per request at speed 5. The three requests reached 1444.11.12, 11.13 and 11.14. Each stopped because its requested date was reached; the final state was paused, speed 5. The script passed with exit 0. The core logged `ui: bound 2 element(s) in ArmyPanel` at `09:51:19.715`.
2. `army.split` on that army created `5257039970358` as a same-province attach target. Clicking the actual Region button at `(419,272)` logged exactly one `task on: center 149 radius 5` at `09:51:48.731`. H opened the native picker; its background covered the attached button and glow, as seen in the captured frame.
3. An opening event remained visible after the third day. An initial answer command incorrectly used the absent `question` property from `question.list` and was refused for a missing integer, without answering it. The script was corrected to use the returned `id`; the event was answered through `question.answer --question 163 --option 0`. This is a test-driver correction, separate from the native-window fix.
4. Clicking `(419,272)` while the picker covered it did not toggle the task. Clicking the target row at `(500,296)` closed the picker; clicking the restored Region button then logged exactly one `task off (toggle)` at `09:52:08.575`. The covered click produced no extra toggle. This course did not separately read back the army's native attachment state.
5. Escape closed the panel; the first outliner row reopened it for a different army, `236223201334`. The same one-day loop advanced another 14 days, from 1444.11.14 to 1444.11.28. Every request stopped at its requested date. Final `time.get` returned `{"date":"1444.11.28","paused":true,"speed":5}`; the script passed with exit 0. The final frame showed the other army's panel and its attached button.
6. The corrected process's `core.log` contained no `native window ordering`, `fatal error`, `panic on thread` or temporary diagnostic marker.

The registry regression first failed with the same ordinary-interface error before the correction. Afterwards all 39 window-ordering tests passed, including derived-receiver exclusion, hidden ordinary-window adoption, and rejection of broken ordinary container ownership. A separate static review found no concrete correctness regression.

## Cleanup and limits

At 09:52:55, `game release --owner army-panel-crash-d3c3e1` released the claim and ended corrected process 48972 intentionally with exit `0x1`. It retained all runs' logs under the claim's `kept` directory. The release's aggregate report says the claim had crashed because earlier reproduction processes 87636, 40392 and 17492 ended with `0x3`; that aggregate is not a crash of corrected process 48972. The subsequent `game status` listed only another session's unrelated claim, which was left alone.

Temporary scripts, screenshots, disassembly and receiver diagnostics were removed after their evidence was recorded here and in the research note. The retained claim logs follow the test-game tool's seven-day retention policy. This acceptance covers the reported trigger, 17 days in one Ottoman session, a different selected army, and picker coverage/input restoration with the actual playset. It does not cover every derived window, late-game saves, or all possible show/hide callers.
