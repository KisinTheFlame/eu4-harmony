# 2026-11-14 Native drawing with observed keyboard stages

This run follows the [rapid-request assertion failure](2026-11-14-window-ordering-native-keyboard-fast.md), using the same release core and `qa-window-ordering` fixture, with `[test-switches] game-paths = ["drawing-thread"]`. Baseline EU4 1.37.5, branch `claude/zealous-carson-b8374d`, base `8d5358f1` plus uncommitted ordering changes. The later host-button fixture and keyboard entry trace were not in this run.

Claim `ordering-native-repeat-1de2b155`, created 16:20:11, hidden process 99432, main thread 97204, fresh France game paused on 1444.11.11. Directory: `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-native-repeat-1de2b155-20261114-162011`. Native main-thread drawing was confirmed in the core log.

The original `flavor_fra.206` question was answered through `question.answer`, and a subsequent `question.list` returned empty. The first Escape still produced no extension close. A screenshot after its input barrier showed both extension windows and the marker pop-up, without the original event window. Therefore a delay between requests alone does not explain every unsuccessful first key; its native recipient or entry path remains to be traced.

The second Escape produced `last closed; first reopened` at 16:24:01.165. Only after that log was read, the next Escape was sent; it produced `reopened first closed by Escape` at 16:24:41.604. Return then produced `qa-window-ordering keyboard passed` at 16:24:49.925, and `popup.list` returned the success marker. The three accepted actions occurred in the intended order and once each, but the run required an extra initial Escape. Do not report this as an unqualified three-key acceptance pass.

The probe recorded four verified placements, 283 stable paired refreshes, and 118 matching independent traversals. There were no `FAILED`, deferred-refresh, or ordering-panic records. The current evidence establishes successful native-drawing placement and subsequent shortcut recipients; it does not establish why the first key was ineffective, pointer activation, same-batch ordering, or the army picker.

Released at 16:25:21, exit code 1, no crash reported. Logs remain in `kept\eu4-harmony-logs`. No coordinate input, player-process interaction or installation occurred.
