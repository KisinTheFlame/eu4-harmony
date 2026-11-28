---
name: game-launch
description: Launch an interactive test game for the user with this checkout's build, visible with sound, applying the player's current game settings, framework settings, and active playset by default. Use when the user asks to start, open, or launch a test game to play or inspect (rather than hidden agent-driven verification in game-test).
---

# Launching an interactive test game for the user

Start a test game through `cargo xtask game` so the checkout's build runs in an isolated claim without writing to the player's user folder or the installed framework. Unlike `game-test` (which runs hidden, muted, and synthetic, then releases right away), this workflow defaults to a visible, audible launch inheriting the player's current setup and leaves the game running for the user.

## Defaults and overrides

Apply these defaults unless the user explicitly asks otherwise:

| Concern | Default | Override when requested |
| --- | --- | --- |
| Core build | `dev-test-games` (player-equivalent with test-game pipes) | `dev` when the user wants developer tools / `dev.toml` |
| Window & audio | `--visible --sound` | Omit `--visible` for a hidden start; omit `--sound` for muted |
| Player setup | `--player` (current game settings, framework settings, and active playset) | Replace with the requested subset of `--player-settings`, `--player-framework-settings`, `--player-playset`, or omit all for a clean synthetic baseline |
| Extra content | None | Add `--mod <folder>` (repeatable, layered after the playset), `--save <path-or-name>`, or `--seed <n>` |

## Steps

1. **Inspect active claims**: Run `cargo xtask game status`. Pick a clear `--owner` name (such as `user-play` or the current worktree/branch name).
   - If that owner already has a running test game and the user wants a fresh launch or new build, stop it first: `cargo xtask game stop --owner <name>`.
   - Reuse an existing stopped claim for that owner, unless the user asks for a clean user folder (then `release` and re-claim).
2. **Build the core**:
   - Default: `cargo build --release --features eu4-harmony-core/dev-test-games`
   - With developer tools requested: `cargo build --release --features eu4-harmony-core/dev`
3. **Claim (if not already held)**: `cargo xtask game claim --owner <name> --purpose "<one line: interactive test game>"`.
   - If refused because the game folder's `version.dll` proxy is missing or outdated, stop and tell the user; installing (`cargo xtask install`, with the player's game closed) is theirs to decide.
4. **Start**:
   ```powershell
   cargo xtask game start --owner <name> --visible --sound --player
   ```
   Adjust flags per the override table when the user specified different window, audio, settings, playset, mod, or save options. Wait until it prints `at the main menu`.
   - **Process-tree cleanup caveat**: Some agent shell runners (such as Windows Job Objects that terminate descendant processes when a command exits) kill `eu4.exe` as soon as `game start` returns. In such runners, launch `game start` inside a persistent daemon command (`IsDaemon: true` or equivalent) that waits on the started `eu4.exe` PID (from `claims.json`) after `game start` succeeds, so the game stays open until the user quits or stops it.
5. **Hand over and leave running**: Confirm with `cargo xtask game status` that this owner's `eu4.exe` is still running on its pipe, then keep the test game running and the claim held. Report:
   - the `--owner` name and remote-control pipe (`eu4-harmony game --test-game <name> ...`),
   - the build kind (`dev-test-games` or `dev`) and which player/override flags were applied (noting any skipped missing descriptors),
   - the claim's user folder and logs path, plus the commands to restart or release when done.

Done when `cargo xtask game start` reports reaching the main menu, `cargo xtask game status` shows this owner's `eu4.exe` running on its pipe, and the hand-over summary has been reported to the user.

## Lifecycle reference

- **Rebuild and restart**: `cargo xtask game stop --owner <name>`, rebuild, then `cargo xtask game start --owner <name> ...`. Each `--player*` start re-reads the player's current settings and playset from the player's user folder (relocating `.mod` descriptors without copying mod trees).
- **Finish and clean up**: When the user is done, `cargo xtask game release --owner <name>` ends the test game if running, retains framework logs, game logs, and crash folders in `kept\` for 7 days, and removes the rest.
- **Guardrails**: Steam must be running. Start `eu4.exe` only through `cargo xtask game`, keep the player's user folder untouched, and leave other claims and the player's own game alone.
