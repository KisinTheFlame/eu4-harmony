-- QA tasks, a test mod of the extensions scenario (docs/dev-tools.md): in
-- each session it tries tasks (awaiting, cancelling, awaiting several, an
-- error raised again where awaited, an async function), waiting for game
-- days, the script group and the reads an extension has, and writes one
-- line for each check: "qa-tasks ok <check>" or "qa-tasks FAIL <check>: why".
-- A task left waiting at a session's end is awaited in the next one, where
-- it must raise cancelled.

-- The day a date is, counted from year 0, as the game counts (no leap years).
local BEFORE = { 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334 }
local function day_of(date)
  return date.year * 365 + BEFORE[date.month] + date.day
end

-- A task waiting far beyond the session, kept for the next one.
local waiting_past_the_session = nil

-- Runs one check: fn returns true, or false and why; an error fails it too.
local function check(name, fn)
  local ok, passed, why = pcall(fn)
  if ok and passed then
    log("qa-tasks ok " .. name)
  else
    log("qa-tasks FAIL " .. name .. ": " .. tostring(ok and why or passed))
  end
end

local function cancelled(err)
  return type(err) == "table" and err.code == "cancelled"
end

local function checks(game, tag)
  check("wait", function()
    local from = game.time.get().date
    local reached = game.time.wait(10)
    local passed = day_of(reached) - day_of(from)
    return passed == 10, "the wait of 10 days reached a date " .. passed .. " days on"
  end)

  check("all", function()
    local a = spawn(function()
      game.time.wait(3)
      return 3
    end)
    local b = spawn(function()
      game.time.wait(5)
      return 5
    end)
    local results = await_all({ a, b })
    return results[1] == 3 and results[2] == 5 and a:done() and b:done(),
      "await_all gave " .. tostring(results[1]) .. " and " .. tostring(results[2])
  end)

  check("any", function()
    local soon = spawn(function()
      game.time.wait(2)
      return "soon"
    end)
    local late = spawn(function()
      game.time.wait(30)
      return "late"
    end)
    local first, value = await_any({ soon, late })
    late:cancel()
    return first == 1 and value == "soon", "await_any gave " .. tostring(first) .. ", " .. tostring(value)
  end)

  check("cancel", function()
    local ran = false
    local t = spawn(function()
      game.time.wait(3)
      ran = true
    end)
    t:cancel()
    local ok, err = pcall(t.await, t)
    game.time.wait(5)
    return not ok and cancelled(err) and not ran and t:done(),
      "awaiting the cancelled task gave " .. tostring(ok) .. ", " .. tostring(err) .. "; its code ran: " .. tostring(ran)
  end)

  check("error", function()
    local t = spawn(function()
      game.time.wait(1)
      error("qa-tasks boom on purpose")
    end)
    local ok, err = pcall(t.await, t)
    return not ok and tostring(err):find("qa-tasks boom on purpose", 1, true) ~= nil,
      "awaiting the failing task gave " .. tostring(ok) .. ", " .. tostring(err)
  end)

  check("async", function()
    local stability_later = async(function(of, days)
      game.time.wait(days)
      return game.country.get(of).stability
    end)
    local t = stability_later(tag, 2)
    local stability = t:await()
    return type(stability) == "number", "the async function gave " .. tostring(stability)
  end)

  check("script", function()
    local scope = { country = tag }
    game.script.set_flag(scope, "qa_tasks_flag")
    local flag = game.script.flag(scope, "qa_tasks_flag")
    game.script.clear_flag(scope, "qa_tasks_flag")
    local cleared = game.script.flag(scope, "qa_tasks_flag")
    game.script.set_variable(scope, "qa_tasks_variable", 2.5)
    local variable = game.script.variable(scope, "qa_tasks_variable")
    return flag.set and flag.day ~= nil and not cleared.set and variable.set and variable.value == 2.5,
      "flag " .. tostring(flag.set) .. ", cleared " .. tostring(cleared.set) .. ", variable " .. tostring(variable.value)
  end)

  check("reads", function()
    local country = game.country.get(tag)
    local modifiers = game.country.modifiers(tag, { "discipline" })
    local opinion = game.country.opinion("ENG", tag)
    local capital = game.province.get(183)
    local armies = game.army.list()
    local regiments = #armies > 0 and game.army.regiments(armies[1].id) or {}
    return country.ruler ~= nil
      and country.legitimacy ~= nil
      and type(country.corruption) == "number"
      and modifiers.values[1].modifier == "discipline"
      and type(opinion.opinion) == "number"
      and type(capital.devastation) == "number"
      and #armies > 0
      and type(armies[1].strength) == "number"
      and #regiments > 0,
      "ruler " .. tostring(country.ruler ~= nil) .. ", legitimacy " .. tostring(country.legitimacy ~= nil) .. ", armies " .. #armies .. ", regiments " .. #regiments
  end)
end

return {
  on_session = function(game)
    local tag = game.session.get().player
    if waiting_past_the_session then
      local before = waiting_past_the_session
      waiting_past_the_session = nil
      check("dropped", function()
        local ok, err = pcall(before.await, before)
        return not ok and cancelled(err) and before:done(),
          "awaiting the task the last session left gave " .. tostring(ok) .. ", " .. tostring(err)
      end)
    end
    waiting_past_the_session = spawn(game.time.wait, 36500)
    checks(game, tag)
    log("qa-tasks checks done")
  end,
}
