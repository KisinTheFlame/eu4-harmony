# 2026-11-14 Native window order and keyboard acceptance

This run tests actual scene placement and keyboard recipients after the [initial integration run](2026-11-14-window-ordering-integration.md). Branch `claude/zealous-carson-b8374d`, base `8d5358f1`, with the uncommitted window-ordering implementation; release build with `dev-test-games,dev-test-switches`, completed before this run. The subsequently added scrollbar support and additional dependency points were not in this core. Baseline EU4 1.37.5, hidden test game, drawing thread enabled, fresh France game paused on 1444.11.11. The added fixture was `xtask/src/scenario/mods/qa-window-ordering`.

Claim `ordering-keys-1de2b155`, created 16:09:41, process 92960, main thread 15980, drawing thread 69592. The claim's directory was `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-keys-1de2b155-20261114-160941`. Only this test game's pipe was used.

## Course and result

The extension requests `z_first`, then a one-button pop-up, then `a_last`; native construction sorts names and therefore initially constructs `a_last` before `z_first`. The implementation recorded four successful full-chain placement checks during the run. After twelve frames, the fixture changes the older window's text and repeats its open request, which must not activate it.

An initial `input.key Escape` produced no extension close notification. A screenshot showed the original France introduction event alongside the test windows. `question.list` identified `flavor_fra.206`, question 2, with one option; `question.answer --question 2 --option 0` dismissed it. This establishes a required test precondition and an observed difference with the native event present; it does not alone identify the native field or branch that intercepted the key.

After dismissing the original event:

- At 16:12:24.783, Escape closed `a_last` first. Its callback closed and reopened `z_first`, creating a real new appearance. The fixture excludes its own programmatic close notification from the keyboard count.
- At 16:12:34.677, the next Escape closed the reopened `z_first`.
- At 16:12:42.329, Return ran the pop-up callback. It asserted that each expected close occurred once and its own action ran once, then logged `qa-window-ordering keyboard passed`.
- `popup.list` returned the final `Ordering keyboard passed` pop-up, with text `Both Escape actions and Return ran once in the expected order.`

The extension log also records the text update and repeated open at 16:10:28.826, before the accepted Escape actions. Thus the keyboard result covers request order differing from construction order, repeated-open/text-refresh stability, reopening, and single execution of these three actions.

The temporary probe recorded 103 stable paired live refreshes, 80 matching independent traversals, four verified placements, zero `FAILED` records, zero deferred refreshes, and no `native window ordering:` panic. Periodic sampling does not cover every intermediate state.

## Cleanup and limits

Released at 16:13:00. Process 92960 exited with code 1 through the release operation; the claim reported no crash. Logs remain under the claim's `kept\eu4-harmony-logs`, including `core.log` and `extension-qa-window-ordering.log`.

This run does not verify pointer activation, two events in one native batch, the army attach picker, paired button visuals, a text scrollbar, or the integrated implementation with native main-thread drawing. It used separate key requests with state reads between them. No coordinate clicks, player-process input, or installation into the game directory occurred.
