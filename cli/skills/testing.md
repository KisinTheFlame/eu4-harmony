testing: tests against a fake game

eu4-harmony test runs the mod's eu4-harmony/tests/*.lua against a fake
game, in the game's own Lua host and sandbox: the same main.lua, the same
errors and time limit, capability calls checked by the catalogue as in the
game. eu4-harmony new writes eu4-harmony/tests/main_test.lua, a worked
example to copy from.

steps:

1. Register each test: test("what it checks", function() ... end). Open the
   mod afresh in each with local mod = open(); main.lua runs again.
2. Answer every capability the code under test calls, before raising the
   signal that calls it: describe a world for the map, the countries and the
   armies, and answer the rest one by one. The report notes "<capability>
   had no answer" for any you left out: the mod got nil, not_ready.
3. Raise the signal, then read back what the mod did and assert on it.
   A test fails when it raises an error, assert included.
4. Run eu4-harmony test. Done when every test is ok, no note says a
   capability had no answer, and each handler you changed has a test.

a world, which answers session.get, country.get, hostile, opinion and
opinions, province.get, list and neighbors, army.get, list, at, selected,
route_to, can_step, can_attack, move and stop, and the script group's flags
and variables, kept as the mod sets them (a flag on the world's date):

  mod:world{
    player = "CAS", date = "1444.11.11", hostile = { { "CAS", "GRA" } },
    countries = { CAS = { treasury = 120 } },      -- fields as country.get has them
    provinces = {
      { id = 217, name = "Madrid", owner = "CAS", neighbors = { 219 } },
      { id = 219, name = "Toledo", owner = "CAS", straits = { 223 } },
      { id = 1300, kind = "sea" },
    },
    armies = { { id = 1, owner = "CAS", province = 217, regiments = 10 } },
    selected = 1,
    opinions = { { of = "GRA", toward = "CAS", opinion = -50, trust = 30 } },
  }

- Neighbors count both ways; a route is the shortest over land, across
  straits. An order sets or clears the army's route as this mod's; armies
  do not move as days pass.
- The world refuses as the game does, in its words: an ID of no army of the
  player's, a navy where an army is asked for, an order to an army with no
  regiments or with anything in its blocked list, a target in a province
  that is not visible. Give the armies you order regiments.
- A pair of countries left out of opinions thinks nothing of each other,
  with the game's trust of 50.
- game.time.wait(days) is answered once a mod:day the test raises reaches
  the date that many days on from the last day raised (or the world's date,
  or 1444.11.11).
- Put a world shared by several tests in eu4-harmony/tests/helpers/<name>.lua
  returning the table, and local spain = require("helpers.spain").

A module of the mod's own with no game in it, such as a decision core, is
tested on its own: require("core") loads eu4-harmony/lua/core.lua into the
test (a helper in eu4-harmony/tests/ of that name goes first). It runs in the
test's state, not the mod's sandbox: log, json and game are not there.

answering one by one, which goes before the world:
  mod:answer(capability, value)          the answer, whatever the request
  mod:answer(capability, function(request) ... end)
                                         an answer from the request's arguments;
                                         return nil, code, message to refuse
  mod:refuse(capability, code, message)  nil, { code, message }
  mod:later(capability)                  the answer comes when the test replies
  mod:reply(capability, value)           gives the oldest such answer
  mod:reply_refusal(capability, code, message)
  mod:respond(n, { status, headers, body })   the nth http.fetch request's response
  mod:fail(n, "timeout" | "network" | "too_large")
  mod:language("simp_chinese")           the player language, english until set

An answer is held to its capability's result (eu4-harmony wiki <group>
<name>, and the editor's eu4.<Class>): a field the result does not have, or
a value of the wrong type, is an error at that line. Leave out the fields
the test is not about.

signals, as the game raises them, through the dispatch the game uses:
  mod:session(saved)   mod:session_ended(reason)   mod:day("1444.11.12")
  mod:raise(kind, { field = value, ... })
                       any signal eu4-harmony wiki signals lists, its fields as
                       the handler gets them (a date { year, month, day }),
                       held to the signal's shape:
                       mod:raise("war_declared", { war = "x", attacker = "CAS", defender = "GRA" })
  mod:frame()          mod:button(name, { right, shift, ctrl })
  mod:army_ordered(army)   mod:army_defeated(army)   mod:map_lost()
  mod:poll()           takes up waiting tasks whose answer came; frame does it too
  mod:save()           calls save_data: its data, or nil and why it failed

the mod's windows and pop-ups, as the player uses them (eu4-harmony skills ui
windows):
  mod:press(window, widget, { row, right, shift, ctrl })
                       presses a button of an open window, in a list's row
                       when row is given: on_button hears "window.widget"
  mod:entry(window)    the window's entry button: opens or closes it
  mod:close(window)    the player closes it, by its close button or Esc
  mod:popups()         the open pop-ups: { popup, header, title, text,
                       buttons } each, in the player language, the
                       buttons' words left to right, or the game's "OK"
  mod:answer_popup(popup, button)
                       presses a pop-up's button, from 1 left to right:
                       its action runs, and the pop-up closes after it
                       unless it answers { close = false }
A window the mod opens or closes itself is heard by on_window at the next
mod:frame(); the player's are heard at once. Opening one of the mod's big
panels closes its others, as in the game.

A day is followed by month when it is the first heard in its month, and by
year in a new year, as in the game; the first day of a session by neither.
session_ended's reason is return, load, new_game or country.

what the mod did:
  mod:log()            its log lines; mod:errors() those of errors
  mod:calls(capability)    the requests it made, every one without a capability
  mod:element(name)    { visible, enabled, text, tooltip } as it set them
  mod:window(name)     { open, [widget] = { visible, text, ... } as it set
                       them, [list] = { rows = { { [widget] = { ... } } } },
                       entry = { ... } }
  mod:map()            { [province] = { r, g, b, a } }, or nil
  mod:requests()       { url, method, headers, body, timeout } each
  mod:waiting()        how many tasks wait
  mod:told()           what the player was told: errors, a stop
  mod:stopped()        why the mod was stopped, or nil

A waiting task: mod:later(...), raise the signal, mod:reply(...), then
mod:poll() takes the task up (eu4-harmony skills tasks).

files is a folder of the test's own, empty at each open(); mod_files reads
the mod's real files.
