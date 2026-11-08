-- QA failing, a test mod of the extensions scenario (docs/plans/whole-game.md):
-- raises an error in every on_day, and calls a capability wrongly, with a
-- text for a province, in every on_month. Neither stops it.
return {
  on_day = function(game, date)
    error("qa-failing: on purpose")
  end,

  on_month = function(game)
    game.province.get("not a province")
  end,
}
