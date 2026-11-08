-- QA looping, a test mod of the extensions scenario (docs/whole-game.md):
-- loops without end in every on_month, which the time limit stops; after
-- three such stops in a row the framework stops it for good.
return {
  on_month = function(game)
    local n = 0
    while true do
      n = n + 1
    end
  end,
}
