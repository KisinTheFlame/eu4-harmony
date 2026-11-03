-- Run once in every extension's Lua state, before its main.lua, with what
-- the Lua host gives it. It finishes the sandbox and builds `game`.
local call, WAIT, task, names, set_element, fetch = ...

-- The sandbox (ADR 0024): code from text only, nothing that reads files.
local raw_load = load
load = function(chunk, name, _, env)
  return raw_load(chunk, name, "t", env)
end
dofile, loadfile = nil, nil
string.dump = nil

local yield, running = coroutine.yield, coroutine.running

-- A function that waits (ADR 0030) is called only in a task: raises the
-- error at `level` otherwise.
local function can_wait(name, level)
  local current = task()
  if current == nil or running() ~= current then
    error(name .. " waits for its answer, which it cannot do here: in on_save, at the top of main.lua, or in a coroutine of the mod's own", level + 1)
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

return game
