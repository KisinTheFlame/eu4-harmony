-- QA hungry, a test mod of the extensions scenario (docs/plans/whole-game.md):
-- grows a table in every on_month until the memory limit refuses it, and
-- logs the refusal; the table is let go, and the extension goes on.
return {
  on_month = function(game)
    local held = {}
    local ok, err = pcall(function()
      for i = 1, 100000000 do
        held[i] = string.rep("x", 100000) .. i
      end
    end)
    held = nil
    log("qa-hungry: " .. tostring(ok) .. " " .. tostring(err))
  end,
}
