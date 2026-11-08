# Scenarios drive the game through remote control only, from Rust in xtask

A scenario plays the game only through remote control, asking its capabilities with their own Rust request and answer types, from code in xtask that runs in the runner's process ([scenarios.md](../plans/scenarios.md)). Nothing clicks at places on the screen or compares images, and nothing runs the command line tool for an answer. So a scenario measures only the framework's own builds, never vanilla, and compares a run with the framework's earlier runs, not with the game without it.

## Considered Options

- **Driving the game by its screen, as `perf-run` did**: reaches vanilla too, but hangs on one screen size, interface scale and set of reference images taken on one computer, and breaks whenever the game draws something new.
- **Running `eu4-harmony game` for each request, as `leak-run` did**: needs no library, but a process for each answer, a command line to put together and JSON to pick apart, and a capability whose fields change shows only when a run reaches it.
- **Scenarios as files read by the runner (TOML, Lua)**: shorter to write, but the leak run decides what to watch from what an answer said, which a list of steps cannot, and Lua would need a host of its own in xtask.

## Consequences

- What the framework costs is followed from its own earlier runs; the figures measured against vanilla in [performance.md](../performance.md) stay as they were and are not taken again.
- A capability a scenario needs that remote control lacks is added to remote control, under ADR remote-control-uses-capabilities, not reached around it.
- What only looking can judge, such as text drawn on screen, is not a scenario's check.
