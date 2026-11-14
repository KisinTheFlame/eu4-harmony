# 2026-11-14 Startup screen attribution and three-key acceptance

This run resolves the original-window identity left open by the [recipient trace](2026-11-14-window-ordering-keyboard-recipient.md). Base `8d5358f1`, branch `claude/zealous-carson-b8374d`, uncommitted ordering implementation; release features `dev-test-games,dev-test-switches`, with native main-thread drawing. The same temporary button/window observers now copy the ancestor window name before dispatch. Ordinary-window name inspection follows the checked layout documented in [window activation](../research/window-activation.md#naming-an-accepted-ordinary-window-for-diagnosis), without executing an arbitrary definition getter.

Claim `ordering-owner-1de2b155`, created 16:57:03, hidden process 102964, main thread 103108. Fresh France game paused on 1444.11.11; the opening `flavor_fra.206` was answered and `question.list` returned empty.

## Initial recipient

At 16:58:17.191, Escape was accepted by native button `close_button` under `startup_screen`. Event modifiers were 0 and `+0x56` was 0. The installed `interface/startup_screen.gui` independently names that window and button and assigns `shortcut = "ESCAPE"`. No fixture window closed. The original startup screen is separate from the country event: an empty question list did not establish its absence.

This identifies a native window that legitimately precedes the fixture in this setup. It does not identify a lost remote event or a duplicate fixture callback. Earlier failed runs remain failed records; this later trace explains the extra Escape without retroactively claiming their intended three-key sequence passed.

## Prepared three-key course

After that native close, `extension.reload --id qa-window-ordering` returned `{ "lost": null, "session": true }`, rerunning the fixture's opening order with the startup screen gone. `popup.list` confirmed the fresh marker. Exactly two Escape requests and one Return request then produced:

| Time | Input | Observed accepted button and parent | Result |
| --- | --- | --- | --- |
| 16:59:15.737 | First Escape | `eu4h_close`, `eu4harmony_qa_window_ordering_a_last_4` | Last-opened window closed; first-opened window reopened, callback at 16:59:15.755 |
| 16:59:47.111 | Second Escape | `eu4h_close`, `eu4harmony_qa_window_ordering_z_first_4` | Reopened window closed, callback at 16:59:47.129 |
| 17:00:05.846 | Return | `eu4h_button_1`, `eu4h_popup_2` | Both close counts and exactly-once action assertion passed; `popup.list` returned `Ordering keyboard passed` |

The first request was issued immediately after reload and a popup-list check, rather than relying on a long arbitrary delay. Later requests followed observation of the previous close. This validates the staged course and the original-window precondition; it does not establish arbitrary downstream readiness after rapidly queued remote calls or multiple events within one native batch.

## Cleanup and limits

Released at 17:00:44, exit code 1, no crash. Retained logs: `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-owner-1de2b155-20261114-165703\kept`. The temporary observers, module registration and checkout `dev.toml` were removed after this run. No screenshot files, coordinate input, installation or player-process changes were used.

Pointer activation and preservation of its native action, same-batch freshness, army-picker overlap/input/restoration, and live scrollbar group movement remain unverified. The previous run found that Page Up did not open the initial army panel; the existing army capability reads the selection but does not make one. The current no-coordinate-input constraint therefore still blocks the original pointer acceptance case. The standard repository checks passed before these temporary probes, as recorded in the recipient run; those results do not replace the missing acceptance checks.
