-- Combined native window ordering fixture (keyboard + pointer + army panel).
-- Phase A (keyboard): opens z_first, a popup, and a_last; after a non-activating
-- text refresh and duplicate open, two Escape keys and Return must dispatch in
-- reverse open order.
-- Phase B (pointer): triggered automatically when Return completes Phase A;
-- opens ptr_first and ptr_second so clicking ptr_first's exposed button activates
-- it once without replay and Escape closes ptr_first before ptr_second.
local active, frames, refreshed = false, 0, false
local closed, answered, reopening = {}, 0, false
local clicks, host_clicks, armed, second_closed = 0, 0, false, false

local function words(text) return { text = text } end
local function kb_window(title, x)
  return {
    size = { 340, 230 }, title = words(title), panel = false,
    anchor = "top_left", offset = { x, 140 },
    items = { { type = "text", name = "status", size = { 280, 45 }, text = words(title) } },
  }
end
local function ptr_window(title, x)
  return {
    size = { 340, 250 }, title = words(title), panel = false,
    anchor = "top_left", offset = { x, 140 },
    items = {
      { type = "button", name = "activate", size = { 109, 31 }, at = { 24, 90 }, text = words("Activate once") },
    },
  }
end

return {
  windows = {
    z_first = kb_window("Ordering: opened first", 100),
    a_last = kb_window("Ordering: opened last", 140),
    ptr_first = ptr_window("Pointer first", 100),
    ptr_second = ptr_window("Pointer second", 280),
  },
  on_session = function(game)
    active, frames, refreshed, closed, answered, reopening = true, 0, false, {}, 0, false
    clicks, host_clicks, armed, second_closed = 0, 0, false, false
    game.window.open("z_first")
    game.popup {
      title = words("Ordering marker 1"),
      text = words("Press Escape twice, then Return. The first Escape must close the last-opened window."),
      buttons = { { text = words("Finish keyboard course"), action = function()
        answered = answered + 1
        assert(answered == 1, "the native Return action ran more than once")
        assert(closed.a_last == 1 and closed.z_first == 1, "Return ran before both expected window closes")
        log("qa-window-ordering keyboard passed")
        game.window.open("ptr_first")
        game.window.open("ptr_second")
        log("qa-ordering-pointer ready")
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
  on_button = function(game, name)
    if name == "ptr_first.activate" then
      clicks = clicks + 1
      assert(clicks == 1, "activating click ran more than once")
      armed = true
      log("qa-ordering-pointer first action once")
    elseif name == "ptr_second.activate" then
      error("activation was replayed into the other window")
    elseif name == "ordering_probe" then
      host_clicks = host_clicks + 1
      log("qa-ordering-pointer host action " .. host_clicks)
    end
  end,
  on_window = function(game, name, open)
    if not active or open then return end
    if name == "z_first" and reopening then
      reopening = false
      return
    end
    if name == "a_last" or name == "z_first" then
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
    elseif name == "ptr_second" then
      second_closed = true
      assert(not armed, "the later window received Escape after activating first")
    elseif name == "ptr_first" and armed then
      assert(clicks == 1 and not second_closed, "pointer order or action count changed")
      armed = false
      log("qa-ordering-pointer passed")
      game.window.close("ptr_second")
    end
  end,
  on_session_ended = function() active = false end,
}
