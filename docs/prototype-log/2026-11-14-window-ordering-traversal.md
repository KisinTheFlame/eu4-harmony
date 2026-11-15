# 2026-11-14 Window ordering traversal comparison

This run tested the independent native traversal proposed in [window-input-refresh.md](../research/window-input-refresh.md). It did not install the ordering policy or test its mouse behavior.

## Build and setup

Branch `claude/zealous-carson-b8374d`, based on `8d5358f1`; the host synchronization reported that the branch already contained the latest `origin/master`. The working tree contained the ordering research and a temporary `dev-test-games`-only probe. Built with `cargo build --release --features eu4-harmony-core/dev-test-games,eu4-harmony-core/dev-test-switches`. The probe ran from the existing native tooltip frame observer's after callback, avoiding an overlapping observer at the same address.

Claim `ordering-o1-1de2b155`, created at 15:06 on 2026-11-14. The test game started hidden from the installed 1.37.5 game directory, with its own core, user folder and pipe. Its playset included the existing `qa-windows` scenario mod. It started a default-date France game through `menu.start` and was paused through `time.pause`. The optional drawing thread was enabled; the log says `drawing: on a thread of its own, no limit`. The player's installation was not changed.

## What was compared

After an ordinary first GUI draw, the probe copied its complete node sequence, each node's 64-byte world transform, and its clipping rectangle plus enabled byte. It invoked `0x14dd7a0` from the first scene root with an identity transform and no inherited clip, writing into a separate native-owned vector. It compared the resulting node sequence and fields exactly. The rendered-node counter was restored, and the temporary vector was freed using the game's own storage replacement routine. The probe did not replace the live draw/input lists or reorder any node.

Samples covered the first eight draws and every 300th draw afterwards, including startup/loading, the main menu and the in-game interface. The existing seven-step window course was driven by `popup.list` and `popup.answer`: initial rows, replaced rows, cleared/hidden text, close, reopen, another panel, and return to the first panel. All seven markers were answered. No screenshot was needed for this byte-for-byte comparison, and no screen-coordinate clicks were sent.

## Results

- 53 traversal comparisons, all matched; zero `traversal FAILED` lines.
- Observed list sizes: 0, 5, 29, 370, 373 and 379 nodes.
- Example main-menu sample: `sample=300 nodes=29 thread=103356 main_holds=true`.
- Example in-game sample: `sample=1200 nodes=373 thread=103352 main_holds=false` (the drawing thread owns the presentation lock there).
- Later in-game sample after the window course: `sample=9900 nodes=370 thread=103352 main_holds=false`.

Sampling was periodic, so the result does not claim a comparison for every intermediate course state. The seven marker completions establish that the lifetime course ran; the sampled node comparisons establish equality only at their sample points.

There were two setup failures before the successful run. First, the copied core lacked test-game pipes after an analysis build replaced the shared release output; rebuilding and immediately starting with the test features resolved this. Second, process 99424 refused startup with exit code 3 because the first probe declared a second observer at `0x14dd010`, overlapping the tooltip observer. The probe was changed to reuse the existing observer. This was a startup refusal, not a successful validation or an unexplained native crash.

The successful process was 103340. `game release` ended it with exit code 1 and retained logs under `ordering-o1-1de2b155-20261114-150638/kept` in the development claims directory. Release also reported the earlier exit-code-3 process and summarized the claim as having crashed; that message refers to the recorded startup refusal. Afterwards `cargo xtask game status` reported zero test games held or running. A separately running player process was left untouched.

## Not verified

No active ordering policy, pointer activation observer, live input-list replacement, stale hit-state invalidation, same-batch mouse-to-keyboard sequence, native parent show/hide observation, or host button-area repair was tested. This run supports the independent traversal contract; it is not completion of O1 or acceptance of the full implementation. Native main-thread drawing is a separate required run.
