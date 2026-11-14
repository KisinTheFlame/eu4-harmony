# 2026-11-14 Visible window-ordering game for the user

Started an isolated visible game at the user's request to try the implementation from PR 194. This is a launch and handoff record, not another regression run.

- Build: `21e8ca79`, after merging current `origin/master`; `cargo build --release --features eu4-harmony-core/dev-test-games` passed. Formatting and workspace tests also passed after the merge.
- Owner: `ordering-user-1de2b155`; claim folder `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-user-1de2b155-20261114-203216`; process 103072.
- The claim copied the player's settings and eight enabled Workshop descriptors, including Harmony, Regional Combat and Convenience Tools. No installation or player-file change occurred.
- Started France on 1444.11.11, paused; answered `flavor_fra.206` and dismissed the separate original startup screen. Split Paris army `545460846646`, creating `5179730559030`; `army.options` confirmed the first army could attach to the second. Both have 7 regiments.
- Inspected a screenshot before handoff, then removed it. `desktop.show` returned `hidden = false`; the game remains silent. No coordinate input was used.

The game is intentionally running for the user and its claim remains held. Release with `cargo xtask game release --owner ordering-user-1de2b155` when the user finishes. No further pointer regression, performance comparison or shutdown check was performed during this launch.
