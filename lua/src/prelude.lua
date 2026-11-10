-- Run once in every extension's Lua state, before its main.lua, with what
-- the Lua host gives it. It finishes the sandbox and builds `game`.
local call, WAIT, task, names, set_element, fetch, resuming, resumed, set_window, show_popup, close_popup, start, cancel_task, awaiting = ...

-- The sandbox (ADR extensions-are-lua-in-a-sandbox): code from text only,
-- nothing that reads files.
local raw_load = load
load = function(chunk, name, _, ...)
  -- An env given, even nil, is the chunk's; none given, the globals are.
  if select("#", ...) == 0 then
    return raw_load(chunk, name, "t")
  end
  return raw_load(chunk, name, "t", (...))
end
dofile, loadfile = nil, nil
string.dump = nil

local yield, running = coroutine.yield, coroutine.running

-- The time limit holds for a coroutine of the mod's own as for the task
-- that resumes it, which the host is told of: Lua's hook reaches only the
-- thread it is set on.
local create, raw_resume, close, status = coroutine.create, coroutine.resume, coroutine.close, coroutine.status
local function left(co, ...)
  resumed(co)
  return ...
end
local function resume(co, ...)
  if type(co) ~= "thread" then
    return raw_resume(co, ...)
  end
  resuming(co)
  return left(co, raw_resume(co, ...))
end
coroutine.resume = resume

-- coroutine.wrap as Lua's: the coroutine's error raised again where it was
-- called, the coroutine closed.
local function unwrapped(co, ok, ...)
  if ok then
    return ...
  end
  if status(co) == "dead" then
    close(co)
  end
  error((...), 2)
end
coroutine.wrap = function(f)
  local co = create(f)
  return function(...)
    return unwrapped(co, resume(co, ...))
  end
end

-- A function that waits (ADR handlers-run-as-tasks-that-can-wait) is called
-- only in a task: raises the error at `level` otherwise.
local function can_wait(name, level)
  local current = task()
  if current == nil or running() ~= current then
    error(name .. " waits for its answer, which it cannot do here: in save_data, at the top of main.lua, or in a coroutine of the mod's own", level + 1)
  end
end

-- A capability: its answer, or for one that waits, the task gives up its
-- turn until the answer has come.
local function finish(name, first, ...)
  if first ~= WAIT then
    return first, ...
  end
  can_wait(name, 3)
  return yield(WAIT, (...))
end

-- Tasks awaited as promises are (ADR tasks-are-awaited-like-promises).
-- spawn(fn, ...) runs fn as a task of its own and returns its handle at once,
-- or as soon as fn first waits; the handle keeps how the task ended: `ended`,
-- then `results` (table.pack of what fn returned) or `error` (what it raised,
-- or the cancelled error). The host keeps the task while it waits, and wakes
-- whoever awaits it once `ended` is set.
local pack, unpack = table.pack, table.unpack

local Cancelled = {
  __tostring = function(err)
    return err.code .. ": " .. err.message
  end,
}

local function cancelled()
  return setmetatable({ code = "cancelled", message = "the task was cancelled" }, Cancelled)
end

-- Each spawned task's handle by its thread, for a task that awaits itself.
local handles = setmetatable({}, { __mode = "k" })

local Task = {}
Task.__index = Task
Task.__name = "eu4.Task"

-- Whether `value` is a task's handle; raises the error at `level` for a
-- function `name` given something else.
local function check_task(name, value, level)
  if getmetatable(value) ~= Task then
    error(name .. " takes tasks, as spawn and async functions return them, not a " .. type(value), level + 1)
  end
end

-- The task gives up its turn until one of `tasks` has ended.
local function await_some(name, tasks, level)
  can_wait(name, level + 1)
  local mine = handles[running()]
  for _, t in ipairs(tasks) do
    if t == mine then
      error(name .. ": a task cannot await itself", level + 1)
    end
  end
  local _, number = awaiting(tasks)
  yield(WAIT, number)
end

local function raise(err)
  error(err, 0)
end

function Task:await()
  check_task("await", self, 2)
  while not self.ended do
    await_some("await", { self }, 2)
  end
  if self.error ~= nil then
    raise(self.error)
  end
  return unpack(self.results, 1, self.results.n)
end

function Task:cancel()
  check_task("cancel", self, 2)
  if self.ended then
    return
  end
  self.ended, self.error = true, cancelled()
  cancel_task(self)
end

function Task:done()
  check_task("done", self, 2)
  return self.ended == true
end

-- How a task's run ended: its results kept, or its error kept and raised
-- again, so that the host logs it as any task's. A task cancelled first
-- keeps its cancellation.
local function settle(handle, ok, ...)
  if not handle.ended then
    handle.ended = true
    if ok then
      handle.results = pack(...)
    else
      handle.error = (...)
    end
  end
  if not ok then
    raise((...))
  end
end

spawn = function(fn, ...)
  if type(fn) ~= "function" then
    error("spawn takes a function, not a " .. type(fn), 2)
  end
  local args = pack(...)
  local handle = setmetatable({}, Task)
  start(handle, function()
    handles[running()] = handle
    return settle(handle, pcall(fn, unpack(args, 1, args.n)))
  end)
  return handle
end

async = function(fn)
  if type(fn) ~= "function" then
    error("async takes a function, not a " .. type(fn), 2)
  end
  return function(...)
    return spawn(fn, ...)
  end
end

-- The tasks a list holds, checked.
local function task_list(name, tasks)
  if type(tasks) ~= "table" then
    error(name .. " takes a list of tasks, not a " .. type(tasks), 3)
  end
  for i = 1, #tasks do
    check_task(name, tasks[i], 3)
  end
  return tasks
end

await_all = function(tasks)
  task_list("await_all", tasks)
  while true do
    local running_still = {}
    for _, t in ipairs(tasks) do
      if not t.ended then
        running_still[#running_still + 1] = t
      elseif t.error ~= nil then
        raise(t.error)
      end
    end
    if #running_still == 0 then
      break
    end
    await_some("await_all", running_still, 2)
  end
  local results = {}
  for i, t in ipairs(tasks) do
    results[i] = t.results[1]
  end
  return results
end

await_any = function(tasks)
  task_list("await_any", tasks)
  if #tasks == 0 then
    error("await_any takes at least one task", 2)
  end
  while true do
    for i, t in ipairs(tasks) do
      if t.ended then
        if t.error ~= nil then
          raise(t.error)
        end
        return i, unpack(t.results, 1, t.results.n)
      end
    end
    await_some("await_any", tasks, 2)
  end
end

-- A task dropped with its session: whoever awaits it later is told it was
-- cancelled.
local function dropped(handle)
  if not handle.ended then
    handle.ended, handle.error = true, cancelled()
  end
end

-- http.fetch, the network permission: it always waits.
http = {
  fetch = function(url, options)
    can_wait("http.fetch", 2)
    return finish("http.fetch", fetch(url, options))
  end,
}

local game = {}
for _, name in ipairs(names) do
  local group, action = name:match("^([^.]+)%.(.+)$")
  game[group] = game[group] or {}
  game[group][action] = function(...)
    return finish(name, call(name, ...))
  end
end

-- game.ui.<element>.visible = true: an element is set, never read.
game.ui = setmetatable({}, {
  __index = function(_, element)
    return setmetatable({}, {
      __newindex = function(_, property, value)
        set_element(element, property, value)
      end,
      __index = function(_, property)
        error("game.ui." .. tostring(element) .. "." .. tostring(property) .. " is set, not read", 2)
      end,
    })
  end,
})

-- game.window.open(name), game.window.close(name), and
-- game.window.<window>.<widget>.<property> = value: set, never read.
game.window = setmetatable({
  open = function(name)
    set_window("open", name)
  end,
  close = function(name)
    set_window("close", name)
  end,
}, {
  __index = function(_, window)
    return setmetatable({}, {
      __index = function(_, widget)
        return setmetatable({}, {
          __newindex = function(_, property, value)
            set_window("set", window, widget, property, value)
          end,
          __index = function(_, property)
            error("game.window." .. tostring(window) .. "." .. tostring(widget) .. "." .. tostring(property) .. " is set, not read", 2)
          end,
        })
      end,
    })
  end,
})

-- game.popup { title = key, text = key, buttons = { { text = key, action =
-- function(game, popup) ... end }, ... } } shows a pop-up and gives it back
-- at once; it waits for nothing. A button's action runs when it is pressed,
-- and the pop-up closes after it unless it answers { close = false }; a
-- button with no action closes it. popup:close() closes it from anywhere.
local actions = {}

local function handle(number)
  return setmetatable({ number = number }, {
    __index = {
      close = function(popup)
        actions[popup.number] = nil
        close_popup(popup.number)
      end,
    },
  })
end

game.popup = setmetatable(game.popup or {}, {
  __call = function(_, spec)
    if type(spec) ~= "table" then
      error("game.popup takes a table: { title = key, text = key, buttons = { { text = key, action = function } } }", 2)
    end
    local number = show_popup(spec)
    local kept = {}
    for i, button in ipairs(type(spec.buttons) == "table" and spec.buttons or {}) do
      kept[i] = button.action
    end
    actions[number] = kept
    return handle(number)
  end,
})

-- A button of the pop-up `number` pressed: its action, and the pop-up
-- closed after it unless it says not to, or when it fails.
local function press(number, button)
  local action = actions[number] and actions[number][button]
  local keep = false
  if action then
    local ok, answer = pcall(action, game, handle(number))
    if not ok then
      actions[number] = nil
      close_popup(number)
      error(answer, 0)
    end
    keep = type(answer) == "table" and answer.close == false
  end
  if not keep then
    actions[number] = nil
    close_popup(number)
  end
end

return game, press, dropped
