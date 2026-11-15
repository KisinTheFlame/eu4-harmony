# 2026-11-14 Identifying the initial Escape's button

This isolated run narrows the unresolved recipient in the [keyboard-entry run](2026-11-14-window-ordering-keyboard-entry.md). Base `8d5358f1`, branch `claude/zealous-carson-b8374d`, uncommitted ordering implementation; native main-thread drawing, release features `dev-test-games,dev-test-switches`. A temporary observer around the already-studied base button and ordinary-window input handlers records only Escape/Return calls that change an initially zero handled mask. It copies object identity before native dispatch and reads only the event after return.

Claim `ordering-recipient-1de2b155`, created 16:48:24, hidden process 103556, main thread 100852. Fresh France session, paused on the opening date; `flavor_fra.206` answered through `question.answer`, with `question.list = []` before the first Escape. The probe's first build failed because its log call named the module rather than `log::write`; the corrected build and startup succeeded.

## Results

| Time | Input | Accepted button | Observed fixture result |
| --- | --- | --- | --- |
| 16:51:50.488 | Escape | `close_button`, native button class `0x1d58218` | Neither fixture window closed |
| 16:52:51.059 | Escape | `eu4h_close` | `a_last` closed; `z_first` reopened at 16:52:51.086 |
| 16:53:35.980 | Escape | `eu4h_close` | Reopened `z_first` closed |
| 16:54:02.622 | Return | `eu4h_button_1` | Close-count and exactly-once assertions passed; `popup.list` returned `Ordering keyboard passed` |

All four events had modifiers 0 and event byte `+0x56 = 0`. The first accepted button's parent owner was `0x20cbe227260`; this probe could not name it because windows do not use the common element name getter. The button's identity is distinct from the fixture's generated `eu4h_close`, but this run does not establish which original window contains it or why that window precedes the fixture.

After dismissing the success pop-up with Return, `input.key page_up` followed by `army.selected` returned `not_found: army panel not open`. Thus the native next-unit shortcut did not establish an initial army selection in this setup. No coordinate input was sent, and the picker was not exercised.

## Repository checks and cleanup

Before this temporary probe was added, the final implementation passed `cargo fmt --all`, `cargo build --release`, `cargo test` (1,158 passed, six ignored across the workspace), ordinary and full-development `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo xtask offline-check`. Offline checks passed all five checks in both ordinary and development builds, including every installed dependency byte and observer argument forwarding. These checks do not establish pointer behavior.

Released at 16:55:02 with exit code 1 and no crash. Retained logs: `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-recipient-1de2b155-20261114-164824\kept`. The next diagnostic run adds read-only window-name inspection; no installation or player input occurred here.
