# 2026-11-14 Window ordering traversal with native drawing

This repeats the [first traversal comparison](2026-11-14-window-ordering-traversal.md) with the drawing-thread speed-up left to the game. The build and temporary probe were unchanged: the core copied into this claim was the same probe build, before the subsequent logical appearance-state edits. This is an O1 traversal experiment, not a test of a completed ordering implementation.

Claim `ordering-o1-native-1de2b155`, created at 15:14 on 2026-11-14, used its own core, pipe and user directory. A temporary checkout `dev.toml` supplied `[test-switches] game-paths = ["drawing-thread"]`; that file was removed after the run. The test started hidden with the existing `qa-windows` mod. A default-date France game was started through `menu.start`, paused through `time.pause`, and all seven native-window course markers were answered through `popup.list` and `popup.answer`. No screen-coordinate input was used, and the player's running process was left untouched.

The probe compared node sequence, 64-byte world transforms and clipping bytes with a second native traversal into a separate native-owned vector, preserving the rendered-node counter and freeing its temporary storage. Sampling followed the same first-eight/every-300th-draw schedule as the first run.

Results:

- The log confirms `drawing: on the main thread, no limit`.
- 44 comparisons, zero failures; node counts were 0, 5, 29, 370 and 379.
- The last sample reads `sample=10800 nodes=370 thread=102832 main_holds=false`. This is expected: leaving the drawing-thread speed-up to the game omits its application-frame lock observer. Main-thread ownership in this mode cannot be inferred from `main_holds()` alone.
- All seven window-course markers completed. Periodic sampling does not establish a comparison at every intermediate state.

`game release` ended process 102740 with exit code 1 and reported no crash. Logs were retained in `ordering-o1-native-1de2b155-20261114-151430/kept` under the development claims directory. Afterwards `cargo xtask game status` reported zero test games held or running.

Not verified: live input-list replacement, candidate eligibility invalidation, accepted mouse activation, actual scene reordering, shortcut precedence, same-batch input, or the army attach picker. Together the two runs establish the sampled independent-traversal equivalence in both drawing modes; they do not complete O1's remaining event and lifecycle obligations.
