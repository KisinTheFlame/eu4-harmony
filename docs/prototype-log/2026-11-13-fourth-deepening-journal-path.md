# 2026-11-13 Fourth deepening journal path (tested by me)

The second native window run used the implementation at `450b5c34` with the test-layout correction and early startup-error check, through `cargo xtask scenario windows --owner native-lifetimes-4510f0 --out target/scenarios/fourth-deepening-native-r2-20261113`. It follows the [rejected probe layout](2026-11-13-fourth-deepening-probe-layout.md).

Claim `native-lifetimes-4510f0-windows-epr2ez` ran process 60832 hidden and reached the main menu after 32 seconds. All four seven-step courses ran: a new game, extension reload, an in-game load and a second session. Eight native screenshots were retained, and no native window or late-tooltip error was found. The scenario nevertheless failed its final count, reporting zero completed steps instead of 28.

The new scenario and the existing `extensions` scenario still chose the old `<id>.log` filename. Current journals use `extension-<id>.log`, with `source=extension-<id>` on each line. The source-prefix reader added in this change was correct but was being given a nonexistent file. The actual `extension-qa-windows.log` contains all 28 completion messages and the panel-open/close transitions. Both callers now choose the current filename as well as parsing its source prefix. This is a validation-reader defect, not proof of a native lifetime failure.

Release reported no crash and exit code `0x1`, released the claim and kept its logs and eight screenshots. This run does not claim a passing scenario or a visual inspection of all eight images. The corrected rerun and the tooltip checks are recorded separately.
