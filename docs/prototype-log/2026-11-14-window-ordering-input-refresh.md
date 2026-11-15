# 2026-11-14 Live window input refresh

This O1 experiment exercises the proposed native input refresh adapter from [window-input-refresh.md](../research/window-input-refresh.md), after the native GUI input batch returns. It does not apply window ordering and does not establish the required between-events placement seam.

The isolated claim `ordering-refresh-1de2b155`, created at 15:28 on 2026-11-14, used a core built with `dev-test-games,dev-test-switches`. It started hidden with `qa-windows`, then loaded France through `menu.start` and paused through `time.pause`. All seven course markers were answered through `popup.list` and `popup.answer`, covering opening, row replacement, row removal, hiding, reopening and panel replacement. No screen-coordinate input was sent.

The temporary probe observes `0x14e9a70` after its native return. For the first eight calls and every 120th call afterwards, it requires main-thread presentation ownership, rebuilds the live draw/hit/GUI-owner vectors, clears current hit state on reachable GUI owners and the manager, and calls the native candidate eligibility refresh. It then repeats the refresh and compares node order, transforms, clipping, hit nodes, owner nodes, candidate GUI identities, owner hit bytes and the manager's current-hit pointer. The rendered-node statistic must equal its value before both refreshes. It also retains the earlier independent traversal comparison on sampled completed draws.

Results for process 101204:

- Drawing ran on its own thread, 94020; input refresh ran on main thread 102560 with `main_holds=true`.
- 39 live-refresh comparisons were stable, zero failed and zero deferred for missing presentation ownership.
- Refresh node counts included 6, 29, 368, 370, 379 and 390. The last sample had 370 nodes, four hits and four candidates.
- 28 independent traversal comparisons matched, zero failed.
- The seven capability-driven course answers succeeded. Periodic sampling does not prove a sample at every intermediate course state.
- Release ended the game with exit code 1 and reported no crash. Afterwards `cargo xtask game status` reported zero test games held or running.

Logs were retained under `ordering-refresh-1de2b155-20261114-152814/kept` in the development claims directory.

Not verified: ordering changes; accepted mouse activation; stale hit removal after a controlled pointer-target change; event-by-event refresh before the next event in one batch; shortcut precedence; the army attach picker; or live refresh with the drawing-thread speed-up disabled. Idempotence and absence of a crash do not establish those behaviors. This result supports the native list ownership and repeated-refresh mechanics, not completion of O1 or the feature.
