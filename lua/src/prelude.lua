-- Run once in every extension's Lua state, before its main.lua, with what
-- the Lua host gives it. It finishes the sandbox and builds `game`.
local call, WAIT, task, names, set_element, fetch, resuming, resumed, set_window, show_popup, close_popup = ...

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

-- game.popup { title = key, text = key, agree = key, decline = key } waits
-- for the button the player presses: "agree", "decline", "goto", or
-- "closed".
game.popup = setmetatable(game.popup or {}, {
  __call = function(_, spec)
    can_wait("game.popup", 2)
    if type(spec) ~= "table" then
      error("game.popup takes a table: { title = key, text = key, agree = key, decline = key }", 2)
    end
    return finish("game.popup", show_popup(spec))
  end,
})
game.popup.close = function(id)
  close_popup(id)
end

return game
