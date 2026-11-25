# Auto-update the Workshop proxy on startup and restart the game

When Harmony's core is loaded from Harmony's own Steam Workshop item, it compares `<game>\version.dll` with `<workshop_item>\version.dll` before `eu4.exe` creates its window (at step 3 of `main` via `hidden::suppress`, or in `fail_startup`'s thread when an outdated proxy fails the interface check). Whenever their bytes differ—and both files carry Harmony's proxy marker—the core stages the newer file as `version.dll.new-<...>`, renames the currently mapped `version.dll` aside as `version.dll.old-<...>`, moves the staged file into place, shows a brief native notice (`EU4HARMONY_PROXY_RESTARTING`, in non-hidden starts), and restarts `eu4.exe` immediately with its original command line so the new proxy takes effect right away. The restarted startup removes retired `version.dll.old-*` and stale `version.dll.new-*` files once the previous process has exited. This supersedes the consequence of [ADR the-framework-ships-on-the-workshop](2026-11-11-the-framework-ships-on-the-workshop.md) that `version.dll` is never updated automatically.

## Considered Options

- **Require the player to copy `version.dll` manually whenever the proxy changes**: works, but creates avoidable friction whenever the proxy fixes a bug or raises its interface version.
- **Update `version.dll` inside `proxy`'s `DllMain`**: `DllMain` runs under the Windows loader lock, where file I/O and process creation are unsafe; keeping update logic in the core also lets the update policy evolve with Workshop updates.
- **Update `version.dll` later (at `PHYSFS_init`) or wait for the next manual launch**: `PHYSFS_init` runs after `SDL_CreateWindow`, flashing the game window before restarting and requiring window-close interception; waiting for the next launch leaves the player on the older proxy for the current session and still requires manual restart if `proxy::OLDEST` is raised.

## Consequences

- Automatic updates derive the companion `version.dll` directly from the running core's own parent folder (`<core_dir>\..`) whenever `<core_dir>` is an `eu4-harmony\` folder outside `game_dir`, without depending on any Workshop mod id or Steam path layout, and never run for local/manual installs beside `eu4.exe` or test games (`-harmony_core` / `-harmony_test_game`).
- Because both compatible and outdated-proxy paths run before `eu4.exe` creates its window, the player never sees a game window flash; a console control handler guards the brief file swap against graceful shutdown signals, and if shutdown was requested the process exits cleanly after completing the swap instead of spawning a new `eu4.exe`.
- Restarting sets `EU4_HARMONY_PROXY_UPDATED=1` on the child process and clears it on read, preventing an infinite restart loop if the files still differ after restarting.
- If updating fails while the loaded proxy is still compatible, startup logs the error and continues normally; if the loaded proxy is incompatible (`found < OLDEST`), startup falls back to notifying the player to copy `version.dll` manually (or to start the game again if `version.dll` was updated but relaunching failed).
