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
   event that calls it. The report notes "<capability> had no answer" for
   any you left out: the mod got nil, not_ready.
3. Raise the event, then read back what the mod did and assert on it.
   A test fails when it raises an error, assert included.
4. Run eu4-harmony test. Done when every test is ok, no note says a
   capability had no answer, and each handler you changed has a test.

answering:
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

events, as the game raises them:
  mod:session(saved)   mod:session_ended(reason)   mod:day("1444.11.12")
  mod:frame()          mod:button(name, { right, shift, ctrl })
  mod:army_ordered(army)   mod:army_defeated(army)   mod:map_lost()
  mod:poll()           takes up waiting tasks whose answer came; frame does it too
  mod:save()           calls on_save: its data, or nil and why it failed

what the mod did:
  mod:log()            its log lines; mod:errors() those of errors
  mod:calls(capability)    the requests it made, every one without a capability
  mod:element(name)    { visible, enabled, text, tooltip } as it set them
  mod:map()            { [province] = { r, g, b, a } }, or nil
  mod:requests()       { url, method, headers, body, timeout } each
  mod:waiting()        how many tasks wait
  mod:told()           what the player was told: errors, a stop
  mod:stopped()        why the mod was stopped, or nil

A waiting task: mod:later(...), raise the event, mod:reply(...), then
mod:poll() takes the task up (eu4-harmony skills tasks).

files is a folder of the test's own, empty at each open(); mod_files reads
the mod's real files.
