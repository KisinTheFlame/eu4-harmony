-- QA steady, a test mod of the extensions scenario (docs/plans/whole-game.md):
-- counts the days and months of a session, reads the country played each
-- day, and keeps the counts as its save data, logging what it gets back.
local days, months, reads = 0, 0, 0

return {
  on_session = function(game, saved)
    if saved then
      days, months, reads = saved.days, saved.months, saved.reads
      log("qa-steady session: saved back " .. days .. " days")
    else
      days, months, reads = 0, 0, 0
      log("qa-steady session: nothing saved")
    end
  end,

  on_day = function(game, date)
    days = days + 1
    local session = game.session.get()
    if session.player and game.country.get(session.player) then
      reads = reads + 1
    end
    log(string.format("qa-steady day %d %d.%d.%d reads %d", days, date.year, date.month, date.day, reads))
  end,

  on_month = function(game)
    months = months + 1
  end,

  save_data = function(game)
    return { days = days, months = months, reads = reads }
  end,
}
