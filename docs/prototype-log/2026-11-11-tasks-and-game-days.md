# 2026-11-11 Awaited tasks and waiting for game days (tested by me)

Parts M13 and M14 of [mod-capabilities.md](../plans/mod-capabilities.md): tasks awaited as promises are ([ADR tasks-are-awaited-like-promises](../adr/2026-11-11-tasks-are-awaited-like-promises.md)) and `game.time.wait`, built on M1 (`d7cc5d25`) in commits `22d77000` and `b2b74226`. The build had the test games' pipes. One test game of this checkout's, started hidden, with the player's playset (the Chinese one) and a mod of my own, `task-probe`, whose `on_session` starts, awaits and cancels tasks and logs what each came to (`game start --mod`).

- **Checks**: `cargo fmt --check`, the release build, `cargo test --release` for the workspace (the core's 645 passed, 5 ignored; the Lua host's 49, among them seven new ones on tasks; the command line tool's 51, among them the fake game's `game.time.wait`), and both clippy runs passed. `cargo xtask offline-check` was not run: neither part adds or touches a dependency point.
- **Remote control**, a new game as France (`menu start FRA`), paused at 1444.11.11:
  - `time wait 2` with `--timeout 5` was not answered: paused, the date does not move. `time wait 0` was refused `invalid_argument`, "days is from 1 to 36500, not 0".
  - `time advance 12 --speed 5` stopped at once for a question (`question answer 2 0` answered it). Run again, with `time wait 3` sent from a second client two seconds before, the advance reached 1444.11.23 and the wait answered `"1444.11.14"`.
  - `time wait 30` sent, then `menu return`: the wait answered `session_changed`, "the session the wait began in ended before its date".
- **The probe's log**, the first session (lines in order):
  - `doomed: 1444.11.11 false cancelled: the task was cancelled true`: a task waiting `game.time.wait(2)`, cancelled in the first `on_day`; awaiting it raised the cancelled error at once, and its code after the wait (`DOOMED RAN`) never ran.
  - `check_later: 1444.11.14 0.0`: an async function waiting 3 days from 1444.11.11, awaited, returned the date it reached and France's stability, read after the wait.
  - `await_any: 2 1444.11.16`: of a task waiting 4 days and one waiting 2 (second in the list), started on 1444.11.14, the second ended first.
  - `await_all: 1444.11.16 1444.11.18`: both, in the list's order.
  - `error in spawn: runtime error: main.lua:41: boom` with its traceback, then `failing: false main.lua:41: boom`: a task's error is logged as any task's and raised again where it is awaited, caught by `pcall(t.await, t)`.
  - `wait 0: nil invalid_argument days is from 1 to 36500, not 0`, and `done: 1444.11.19 true`.
- **The session's end**: after `menu return` the log said `the session ended: 1 waiting task(s) dropped` (a task waiting 1000 days); in the next game (`menu start CAS`) `on_session` awaited that task and got `old session's task: false cancelled`. `KEPT RAN` never appeared.
- The test game was released; it ended with exit code 1 (`game stop`), no crash.

Not verified: the traceback of a task's error that the log writes ends in the prelude's frames (`prelude:121: in upvalue 'raise'`), not where `error` was called, though the message keeps the file and line; a task cancelled while it runs, and one cancelled by another taken up in the same frame, were tried only in the Lua host's unit tests, not in the game; `await_all` of tasks of which one failed, and an `await` of a task awaiting a capability that answers later (a proposal), only in unit tests; the editor's types (`eu4.Task<T>`) were checked as text and compiled as Lua, not opened in an editor with LuaLS; the sweep scenario, which now asks `time.wait` only at its edges, was not played.
