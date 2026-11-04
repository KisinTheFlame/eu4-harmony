tasks: waiting for an answer

Every handler call runs as a task. A function that waits (eu4-harmony wiki
lua marks each: http.fetch, and capabilities whose answer comes later, such
as diplomacy.propose_alliance) gives up the task's turn and returns its
answer when it has come, frames later, as await does, with no keyword:

  on_button = function(game, name)
    local response, err = http.fetch("https://api.example.com/score")
    if err then return log("not sent: " .. err.message) end
    -- frames have passed here: read the game again before acting on it
    local session = game.session.get()
  end,

While a task waits, the game and the mod's other handlers go on. After a
wait, treat everything read before it as stale: read the game again, and
check the mod's own state again (another handler may have changed it).
Done when no value read before a wait is used after it without reading it
again.

spawn(fn) starts fn as a task of its own and returns once fn first waits or
ends: for two requests at once, or a request that outlives the handler that
sent it. A spawned task cannot be waited for or cancelled; keep its result
in the mod's own state for later handlers to read.

Where nothing waits: at the top of main.lua and in save_data (they raise an
error), and in a coroutine of the mod's own (coroutine.wrap and the like).
Wait in the handler or in a spawned task.

A session's waiting tasks are dropped when it ends (back to the main menu, a
save loaded, a new game) and are never taken up again; the log says how
many. A task waiting for an answer about the old session simply stops.
Anything a task must finish, write before it waits, or redo it in the next
session's on_session.

The time limit holds for each stretch a task runs, not for its waits.
