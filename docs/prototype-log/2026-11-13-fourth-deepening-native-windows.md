# 2026-11-13 Fourth deepening native window courses (tested by me)

The corrected `windows` scenario passed on EU4 1.37.5. This follows the [layout rejection](2026-11-13-fourth-deepening-probe-layout.md) and [journal-path failure](2026-11-13-fourth-deepening-journal-path.md). The core implementation is `450b5c34`; the scenario corrections are committed as `3ecdc4f2`, with master `e902f0a0` included.

`cargo xtask scenario windows --owner native-lifetimes-4510f0 --out target/scenarios/fourth-deepening-native-r3-20261113` claimed `native-lifetimes-4510f0-windows-7d77lw`, started process 82740 hidden, and reached the main menu after 28 seconds. The player build had test-game pipes only. The private playset added `qa-windows` to the inherited playset. One other hidden verification game was running; these are behavior checks, not performance measurements.

All 28 steps passed, seven in each course: a new French game, extension reload, an in-game load of the autosave, and another session after a return. Each course constructed a native window, replaced and cleared its list rows, cleared a tooltip, closed and reopened the window, opened another big panel and returned to the first. Pop-up markers appeared after twelve extension frames and were answered through `popup.answer`. A window remained open for each reload or session transition.

The final record has `completed_steps: 28`, `extension_errors: []` and `native_errors: []`. Eight frames were captured. Selected captures after extension reload/reopening and replacement in the next session were visually inspected: the current text and list rows appeared in the native window. The other captures remain in the kept logs; passing the counters alone is not their visual verification.

Release reported no crash and exit code `0x1`; the claim was released, with logs and captures retained in its `kept` folder. This run does not inject faults inside native constructors or list insertion, exercise every native destruction interleaving, or prove device-loss recovery. The separate [tooltip and stationary replacement check](2026-11-13-fourth-deepening-tooltip-handoff.md) supplies the direct hover evidence.
