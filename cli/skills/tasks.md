tasks: waiting, and doing several things at once

Every handler call runs as a task. A function that waits (eu4-harmony wiki
lua marks each: http.fetch, await_all, await_any, a task's await, and
capabilities whose answer comes later, such as diplomacy.propose_alliance
and game.time.wait)
gives up the task's turn and returns its answer when it has come, frames
later, as await does in JavaScript, with no keyword:

  -- 1. A capability that waits needs no await.
  on_button = function(game, name)
    local reply, err = game.diplomacy.propose_alliance("CAS")
    if not reply then
      log("not proposed: " .. err.message)   -- the game said no: nil, err
      return
    end
    log("Castile " .. reply.answer)          -- accepted or declined
  end,

While a task waits, the game and the mod's other handlers go on. After a
wait, treat everything read before it as stale: read the game again, and
check the mod's own state again (another handler may have changed it).
Done when no value read before a wait is used after it without reading it
again.

Several things at once: spawn and async

spawn(fn, ...) starts fn, with the values after it, as a task of its own and
returns the task as soon as fn first waits or ends. async(fn) makes fn an
async function: each call starts it as a task and returns the task, as an
async function in JavaScript returns a promise. A task t has:

  t:await()   waits until it has ended and returns what fn returned
  t:cancel()  ends it at the wait it is in, or at its next
  t:done()    whether it has ended

and await_all(tasks) waits for every task, returning each one's first
result in order (as Promise.all), await_any(tasks) for the first to end,
returning its place in the list and what it returned (as Promise.race).

  -- 2. Several things at once.
  on_button = function(game, name)
    local tasks = {}
    for _, tag in ipairs({ "CAS", "POR", "ARA" }) do
      tasks[#tasks + 1] = spawn(game.diplomacy.improve_relations, tag)
    end
    local results = await_all(tasks)
  end,

Game days: game.time.wait(days) waits until that many days have passed in
the game and returns the date reached. Days the game stands paused do not
count; it neither runs nor pauses the game.

  -- 3. An async function, and game days.
  local check_later = async(function(game, tag, days)
    game.time.wait(days)
    return game.country.get(tag).stability
  end)

  on_button = function(game, name)
    local t = check_later(game, "FRA", 30)
    log("France's stability in 30 days: " .. t:await())
  end,

Failing and cancelling

An error raised in a task is written to the mod's log, as any handler's
error, and raised again where the task is awaited, as a rejected promise
throws at its await. A cancelled task raises an error whose code is
cancelled there.

  -- 4. Cancelling: an order by hand takes the army back.
  local pending = {}

  on_army_ordered = function(game, army)
    if pending[army] then pending[army]:cancel() end
    pending[army] = spawn(function()
      game.time.wait(10)
      game.army.stop(army)
      pending[army] = nil
    end)
  end,

  -- 5. A time-out.
  local reply = spawn(http.fetch, "https://api.example.com/plan")
  local timer = spawn(game.time.wait, 5)
  local first = await_any({ reply, timer })
  if first == 2 then reply:cancel() end

Catch a failed or cancelled task's error with pcall:

  -- 6. Errors and cancelling, caught.
  local ok, err = pcall(t.await, t)
  if not ok and not (type(err) == "table" and err.code == "cancelled") then
    error(err, 0)                          -- anything else goes on up
  end

Cancelling takes back nothing the task already asked of the game: a
proposal sent is sent, a request on its way still arrives; its answer is
dropped, and the task's code after its wait never runs. A task cannot await
itself.

Where nothing waits: at the top of main.lua and in save_data (they raise an
error), and in a coroutine of the mod's own (coroutine.wrap and the like).
Wait in a handler or in a task.

A session's waiting tasks are dropped when it ends (back to the main menu, a
save loaded, a new game) and are never taken up again; the log says how
many. A task dropped so counts as cancelled: awaiting it later raises
cancelled. Anything a task must finish, write before it waits, or redo it in
the next session's on_session.

The time limit holds for each stretch a task runs, not for its waits.

Done when every task the mod starts is awaited, or kept where a later
handler can await or cancel it, and every await that can fail is either
meant to end the handler or wrapped in pcall.
