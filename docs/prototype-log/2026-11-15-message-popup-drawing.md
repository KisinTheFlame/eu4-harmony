# 2026-11-15 Native message button display and drawing (tested by me)

This run follows the [annexation-result investigation](2026-11-15-message-popup-buttons.md), which did not reproduce the reported missing action buttons. It checks the native display and drawing path without requiring the reporter's save. The reported disappearance remains unreproduced; this run establishes no cause or repair.

## Build and setup

- Checkout `claude/peaceful-curie-70ea5b`, fast-forwarded through the app's base-branch synchronization to `9cedf65702`, the fetched `origin/master`. The earlier run's baseline remains as recorded in its own note.
- Built successfully with `cargo build --release --features eu4-harmony-core/dev-test-games`. No product source was changed for this investigation.
- Hidden, silent test game `loving-wiles-message-draw`, PID 79868, with its own core, user folder and pipe. The startup report identifies Harmony 0.2.1, API 0.3, a developer build with the test games' pipes. No game-folder installation or control of another game.
- EU4 1.37.5, a new Saxony campaign, the same eight-mod copied playset as the earlier run: Graphical Map Improvements, Emoji Universalis, Chinese Language Mod for 1.37, Chinese Language Supplementary Mod for 1.37, the modifier-sorting mod, Harmony, Regional Combat and Convenience Tools. Copied settings select 3840x2160 and UI scale 1.500. Screenshots below are at 50% of the frame's dimensions.

## Static path checked alongside the run

The complete message factory, popup constructor, filler, base-window initialization and the relevant display methods were read. The [action study](../research/message-pop-up-actions.md) and [button drawing study](../research/message-button-drawing.md) record the call sites and field checks.

The base dialog first shows the root. That operation visits the button group and calls each button's show method, activating its graphics node. The message-specific filler then sizes the background and places the buttons from the text height, hides the unselected group, and sets the selected labels. Its label helpers do not explicitly show a button. The final graphics-tree insertion is separate from the initial show. Thus, the absence of a second show call in the filler is not evidence of a missing show in the complete creation path.

The standard message action buttons use a text sprite, caching their background and label together. Visible element state, active node state, retained texture, mesh data, draw-list membership, clipping and drawing color are separate observations. The current framework already protects the message factory and base window creation as crossings; this investigation did not demonstrate a missing protection boundary.

## Initial two-button control

A normal alliance proposal to Bohemia was declined, creating `MILALLREJECT`. The campaign introduction initially covered it; Escape dismissed the introduction. The unobstructed frame showed both the left Go to and right confirmation buttons. The message-settings icon opened a separate settings panel, whose choices did not include the message's single/two-button mode; Cancel dismissed that panel without changing a setting.

A disposable Python probe used only `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)` and `ReadProcessMemory` on this claim's PID. It resolved the actual executable base through Toolhelp, selected open, unfinished native message popups by their class, looked up the named buttons in the root's actual element tables, checked each graphics-node class, and read the prepared lists without calling native code or writing memory. The button-table vector layout and list layout were checked against the game's complete lookup/drawing functions before use.

For the two selected buttons, the corrected snapshot recorded:

| Observation | DeclineButton | AgreeButton |
| --- | --- | --- |
| Element visible / dead / suppressed | 1 / 0 / 0 | 1 / 0 / 0 |
| Graphics-node class | `0x1d933b0` | `0x1d933b0` |
| Node active | 1 | 1 |
| Retained texture | Nonzero | Nonzero |
| Mesh first count | 4 | 4 |
| Drawing color RGBA | 1, 1, 1, 1 | 1, 1, 1, 1 |
| Clipping enabled | 0 | 0 |
| In first / second GUI list | Yes / no | Yes / no |

`CenterOK` was hidden and inactive, had no retained texture, and was absent from both lists: the expected state for this two-button message. Settings was visible and used a different graphics-node class, which the probe did not interpret as a text node.

Two provisional field labels were corrected before evaluating the samples: node `+0x1b8` is animation time, not alpha; node `+0xf0` is the previous sibling, not the parent. The checked parent is `+0x110`, and drawing alpha is the fourth float at `+0x48`, namely `+0x54`. A zero animation time coexisted with normal visible buttons and was not treated as a transparency defect. Preserved snapshots use the corrected names.

## Repeated creation and interruptions

Twelve additional alliance proposals to Bohemia returned `answer = declined`, with a screenshot and a corrected state snapshot after each. The final nine ran from `1445.7.26` through `1446.5.2`, advancing 35 actual simulated days between proposals and confirming native notifications through normal Return input. The first three captures also include naturally arriving messages and overlapping windows.

The initial driver needed corrections that are not instances of the reported bug:

- Changing the date alone did not finish a diplomat's return; the next proposal was correctly rejected for a recent dispatch and an unavailable diplomat. The driver switched to actual bounded simulation.
- A Return confirmation could close a newer notification while leaving the alliance result open. A single-popup-count assumption failed with two normal `MILALLREJECT` objects. Both objects had ready action buttons. The driver was changed to drain ordinary native notifications before the next proposal.
- A 35-day advance can return early because a notification pauses time or an event needs an answer. The test stopped at `1445.6.21` for `trade_policy_events.1`; its option 0 was answered through `question.answer`. The final driver handled these interruptions before accepting a completed advance. Early returns were not counted as 35 completed days.

Across the twelve preserved snapshots there are 26 open-message samples, including 13 `MILALLREJECT` samples because one remained open across captures. The other keys were `GOLDEN_BULL_PICKED`, `NEWMONOTHER`, `NEWACTIVECARDINAL`, `MAJOREVENTHAPPENOTHEROPTION` and `MAJOREVENTHAPPENOTHER`. Every sampled message's selected action buttons were visible and active, retained a texture, had a nonzero mesh count, had positive drawing alpha, and appeared in the first GUI draw list. These are repeated samples, not 26 independent messages; native addresses may also be reused after destruction.

All twelve screenshots were inspected together as cropped contact-sheet panels. Captures 1 and 4 through 12 show the alliance result with both buttons. Captures 2 and 3 have newer notifications in front; their visible action buttons are normal, but those pictures alone do not verify every covered button behind them. The state snapshots independently cover the open alliance messages. No captured frame showed the reported whole action-button group missing.

## Evidence and limits

Useful local evidence is retained in the ignored `target/diagnostics/message-popup-drawing-2026-11-15/`: the twelve full frames and corrected snapshots, `repeat-contact.png`, the initial unobstructed control and corrected snapshot, `observations.json`, bounded-advance records, driver outcome logs, the build log, startup report and release record. These are diagnostic records, not product assets or regression fixtures.

Reads were not atomic with the game's update or drawing, and the screenshot and state snapshot were separate operations. A node's list membership in one sample does not prove continuous membership; a nonzero texture does not prove the content of its GPU pixels. The probe can expose several missing-button states, but no check was observed failing because of the user's actual symptom. Its passing controls are not a demonstrated failing regression test.

This run did not repeat the annexation sequence, reproduce the reporter's exact framework build or environment, test graphics-device reset, or capture a transient failure between screenshots. It supports the display-path description and the normal controls, not a conclusion that the report is resolved. The missing evidence is a captured failing display state; the original save is not a prerequisite for further work on this path. No product-code fix was justified or applied.

## Validation and cleanup

The test build completed, the address index was regenerated with the new checked names, documentation links and TOML parsing were checked, and `git diff --check` passed. The final changes are research, the game's interface page, the address index and run records; Rust tests and clippy were not rerun for documentation-only changes.

`xtask game release --owner loving-wiles-message-draw` reported `released`, controlled exit `0x1`, and no crash. `game-ends.tsv` records PID 79868 ending at `05:14:53` with that code; the retained emergency log contains only its owned-log header. `xtask game status` no longer lists this owner. The normal seven-day `kept/` folder retains the game's logs; release removed the temporary user folder and copied core. The one-use probe, repeat driver, contact-sheet script, loose disassembly outputs, provisional snapshots and obscured initial control were removed. The earlier run's evidence and the user's original screenshot were preserved. No installation, registration or player-file change was made. Nothing was committed or pushed.
