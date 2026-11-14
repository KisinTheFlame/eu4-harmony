-- Dismiss original opening dialogs, then reload this extension.
-- Click the exposed first window's button, then Escape. Its action must run
-- once and its window must close before the later-opened window.
local clicks, host_clicks, armed, second_closed = 0, 0, false, false
local function text(s) return { text = s } end
local function window(title, x)
  return {
    size = { 340, 250 }, title = text(title), panel = false,
    anchor = "top_left", offset = { x, 140 },
    items = {
      { type = "button", name = "activate", size = { 109, 31 }, at = { 24, 90 }, text = text("Activate once") },
    },
  }
end
return {
  windows = {
    first = window("Pointer first", 100),
    second = window("Pointer second", 280),
  },
  on_session = function(game)
    clicks, host_clicks, armed, second_closed = 0, 0, false, false
    game.window.open("first")
    game.window.open("second")
    log("qa-ordering-pointer ready")
  end,
  on_button = function(game, name)
    if name == "first.activate" then
      clicks = clicks + 1
      assert(clicks == 1, "activating click ran more than once")
      armed = true
      log("qa-ordering-pointer first action once")
    elseif name == "second.activate" then
      error("activation was replayed into the other window")
    elseif name == "ordering_probe" then
      host_clicks = host_clicks + 1
      log("qa-ordering-pointer host action " .. host_clicks)
    end
  end,
  on_window = function(game, name, open)
    if open then return end
    if name == "second" then
      second_closed = true
      assert(not armed, "the later window received Escape after activating first")
    elseif name == "first" and armed then
      assert(clicks == 1 and not second_closed, "pointer order or action count changed")
      armed = false
      log("qa-ordering-pointer passed")
      game.window.close("second")
    end
  end,
}
