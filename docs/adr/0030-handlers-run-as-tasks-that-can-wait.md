# Handlers run as tasks that can wait

Every handler call runs as a task, a Lua coroutine the framework owns, so that a function which has to wait, such as `http.fetch` for an answer from the network, simply gives up the task's turn: the game goes on, and the framework takes the task up again at a later safe point, where the handler carries on from the line after the call with `game` valid again. A handler that waits reads as sequential code, as with `await` in TypeScript, but with no keyword and no callback. `spawn(fn)` starts a task of its own without waiting for it. Nothing in the game thread ever blocks: the Lua host stays on the main thread, at safe points, as before.

A session's waiting tasks are dropped when it ends, and all of them when the game closes: what they would go on with belongs to a world that is gone. `on_save`, which must return the save data at once, and the top of `main.lua`, which runs before there is a game, cannot wait.

## Considered Options

- **A callback passed with each request** (as some game engines do it): nothing new in how handlers run, but every request splits the code that sends it from the code that uses the answer, and a sequence of requests nests.
- **Mods start their own coroutines** (`coroutine.wrap` around the code that waits), as some Lua web servers have it: the same waiting, but every author has to know to do it, and a wait outside a coroutine is an error for each of them to meet.
- **Block until the answer comes**: the game would stop for as long as the network takes.

## Consequences

- While a task waits, the extension's other events run: its state may have changed when the task is taken up again, as it may after an `await`. The guides for coding agents (`eu4-harmony skills`) explain this and the dropping of a session's waiting tasks.
- The time limit (ADR 0027) applies to each stretch a task runs, not to the time it waits.
- Any later capability can wait in the same way, such as for a number of game days; the type definitions and `wiki` mark every function that waits.
- A spawned task cannot yet be waited for or cancelled; that can be added later without changing what exists.
