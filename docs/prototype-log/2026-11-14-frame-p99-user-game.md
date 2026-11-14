# 2026-11-14 Visible game for the user

Started a visible test game at the user's request to try the current stream-return lock-order release implementation. This is a launch record, not a performance measurement or a whole-suite result.

## Build and setup

- Commit: `52ae8aebbd91ebe0cfec203c4a52084536e2848c`, already containing the current `origin/master` at launch.
- Build: `cargo build --release --features eu4-harmony-core/dev-test-games`, exit 0.
- Owner: `p99-user-f0129f`; private user folder, core and remote pipe.
- Game: EU4 1.37.5 in the installed game folder. No installation was performed.
- Settings and playset: copied by the claim from the player's configuration. Eight Workshop mod descriptors were enabled. Startup loaded the `regional-combat` 0.4.0 and `convenience-tools` 0.1.0 extensions.
- No save was supplied; launch stopped at the main menu for the user.

## Result

`game start --owner p99-user-f0129f --visible` exited 0, reporting process 99136, the main menu after 27 seconds, and `the window is shown`. The core startup log reported 2,536 matching dependency points, 309 hooks and 90 writes, with drawing on its own thread.

The game remains running for the user, so its claim is intentionally held. Release it with `game release --owner p99-user-f0129f` when the user finishes. Gameplay, performance, shutdown and the whole scenario suite were not verified by this launch.
