-- Open order deliberately differs from alphabetical construction order.
-- First dismiss original opening dialogs, including startup_screen, then reload
-- this extension. Drive with two Escape keys, then Return; no coordinates are needed.
-- An empty question.list does not mean that the separate startup_screen is closed.
local active, frames, refreshed = false, 0, false
local closed, answered, reopening = {}, 0, false
local function words(text) return { text = text } end
local function window(title, x)
  return {
    size = { 340, 230 }, title = words(title), panel = false,
    anchor = "top_left", offset = { x, 140 },
    items = { { type = "text", name = "status", size = { 280, 45 }, text = words(title) } },
  }
end
return {
  windows = {
    z_first = window("Ordering: opened first", 100),
    a_last = window("Ordering: opened last", 140),
  },
  on_session = function(game)
    active, frames, refreshed, closed, answered, reopening = true, 0, false, {}, 0, false
    game.window.open("z_first")
    game.popup {
      title = words("Ordering marker 1"),
      text = words("Press Escape twice, then Return. The first Escape must close the last-opened window."),
      buttons = { { text = words("Finish keyboard course"), action = function()
        answered = answered + 1
        assert(answered == 1, "the native Return action ran more than once")
        assert(closed.a_last == 1 and closed.z_first == 1, "Return ran before both expected window closes")
        log("qa-window-ordering keyboard passed")
        game.popup {
          title = words("Ordering keyboard passed"), text = words("Both Escape actions and Return ran once in the expected order."),
          buttons = { { text = words("Done") } },
        }
      end } },
    }
    game.window.open("a_last")
    log("qa-window-ordering opened first, popup, last")
  end,
  on_frame = function(game)
    if not active or refreshed then return end
    frames = frames + 1
    if frames < 12 then return end
    game.window.z_first.status.text = words("Text refresh must not activate this window")
    game.window.open("z_first")
    refreshed = true
    log("qa-window-ordering text and repeated open applied")
  end,
  on_window = function(game, name, open)
    if not active or open then return end
    if name == "z_first" and reopening then
      reopening = false
      return
    end
    closed[name] = (closed[name] or 0) + 1
    assert(closed[name] == 1, "one Escape closed a window more than once")
    if name == "a_last" then
      assert(closed.z_first == nil, "the older window received Escape first")
      reopening = true
      game.window.close("z_first")
      game.window.open("z_first")
      log("qa-window-ordering last closed; first reopened")
    elseif name == "z_first" then
      log("qa-window-ordering reopened first closed by Escape")
    end
  end,
  on_session_ended = function() active = false end,
}
