# Remote control is off by default and enabled in settings or by switch

Remote control's named pipe (`\\.\pipe\eu4-harmony`) is no longer opened automatically on ordinary player starts. It opens only when:

- the player enables **Enable remote control** (`remote_control=yes`) on the **Advanced** page of Harmony settings (takes effect at the next start), or
- the process is started with `-harmony_remote`, `-harmony_hidden`, or (in `dev-test-games` builds) `-harmony_test_game=<name>`.

`eu4-harmony start` and isolated test games pass `-harmony_remote` automatically so CLI automation, hidden starts, and test suites keep working without modifying the player's `settings.txt`.

This supersedes [ADR remote-control-is-always-on](2026-10-29-remote-control-is-always-on.md).

## Considered Options

- **Keep remote control always on (ADR remote-control-is-always-on)**: convenient for developers, but ordinary players never use the pipe and can reasonably expect an external control interface with game-console access to be opt-in and visible in settings.
- **Toggle the pipe live without restarting (`WhenSaved`)**: adds thread teardown, in-flight request draining, and pipe re-creation complexity for a setting changed rarely by developers and automation users.

## Consequences

- Ordinary player launches from Steam or the Paradox launcher do not create the named pipe.
- Developers connecting to a manually launched game (`eu4-harmony game` or `eu4-harmony reload`) enable **Enable remote control** in **Harmony settings → Advanced** once and restart, or start the game with `eu4-harmony start`.
- `eu4-harmony` connection failures guide users to that setting when the pipe is not open.
