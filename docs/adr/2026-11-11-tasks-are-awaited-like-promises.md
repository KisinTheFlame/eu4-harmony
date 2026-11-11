# Tasks are awaited as promises are, and an error comes back where it is awaited

`spawn(fn, ...)` returns the task it starts, and `async(fn)` makes a function that starts one each time it is called, as an async function in JavaScript returns a promise. A task is awaited with `t:await()`, which gives up the turn until the task ends and returns what `fn` returned; `await_all` and `await_any` await several, as `Promise.all` and `Promise.race` do; `t:cancel()` ends a task at the wait it is in. An error in the task is raised again where it is awaited, and a cancelled task raises one whose code is `cancelled`, as a rejected promise throws at its `await`. A capability that waits still needs no `await` (ADR handlers-run-as-tasks-that-can-wait): only a task is awaited.

## Considered Options

- **`t:wait()` returning `nil, err`**, as the framework's capabilities do for a refusal: one convention everywhere, but `nil, err` means the game said no, and a task that failed or was cancelled is the extension's own code going wrong or being stopped; a caller that forgot to look at `err` would go on with `nil`, where an error stops it at the line.
- **No handle; tasks share their results through upvalues**, as now: nothing new to learn, but every mod writes its own counting and waiting for several tasks, and none can stop one.

## Consequences

- An extension catches a task's failure with `pcall(t.await, t)`; an error not caught ends the task that awaited, logged as any error is (ADR an-error-does-not-stop-an-extension).
- Cancelling takes back nothing a task already asked of the game: a proposal sent is sent, and its answer is dropped.
- A session's tasks are still dropped when it ends; awaiting one of them in a later session raises `cancelled`.
