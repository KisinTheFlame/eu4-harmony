-- Template Mod's tests: eu4-harmony test runs them against a fake game, in
-- the game's Lua host and sandbox. eu4-harmony skills testing says how.

local ARMY = 545460846646

test("the button shows only while an own army is selected", function()
  local mod = open()
  mod:refuse("army.selected", "not_found", "no own land army selected")
  mod:frame()
  assert(mod:element("button").visible == false)
  mod:answer("army.selected", ARMY)
  mod:frame()
  assert(mod:element("button").visible == true)
end)

test("a click writes a line to the log", function()
  local mod = open()
  mod:answer("army.selected", ARMY)
  mod:button("button", { right = true })
  local log = mod:log()
  assert(log[#log] == "button clicked (right: true), click #1, army " .. ARMY, log[#log])
end)

test("the tooltip is the mod's text in the player language", function()
  local mod = open()
  mod:session(nil)
  assert(mod:element("button").tooltip:find("Template Mod", 1, true))
  -- A language the mod has no texts in shows the English ones.
  local other = open()
  other:language("turkish")
  other:session(nil)
  assert(other:element("button").tooltip == mod:element("button").tooltip)
end)
