-- The windows scenario advances this native course by answering its pop-ups.
-- A marker appears only after several frames have applied the desired state.
-- The runner checks native errors separately: Lua readback alone cannot prove
-- the game's construction or rendering succeeded.
local step, frames, applied, waiting = 1, 0, false, false
local sessions, active = 0, false
local function words(text) return { text = text } end

local function apply(game)
  local probe = game.window.probe
  if step == 1 then
    game.window.open("probe")
    probe.status.text = words("Native window: initial rows")
    probe.status.tooltip = words("Initial native tooltip\nSecond line")
    probe.progress.value = 0.25
    probe.results.rows = {
      { result = { text = words("First native row"), tooltip = words("First row tooltip") } },
      { result = words("Second native row") },
      { result = words("Third native row") },
    }
  elseif step == 2 then
    probe.status.text = words("Native window: replaced rows")
    probe.status.tooltip = words("Changed native tooltip")
    probe.progress.value = 0.75
    probe.results.rows = { { result = { text = words("Replacement native row"), tooltip = words("Replacement row tooltip") } } }
  elseif step == 3 then
    probe.results.rows = {}
    probe.status.tooltip = nil
    probe.status.visible = false
  elseif step == 4 then
    game.window.close("probe")
  elseif step == 5 then
    game.window.open("probe")
    probe.status.visible = true
    probe.status.text = words("Native window: reopened, tooltip cleared")
    assert(probe.status.tooltip.text == "", "a cleared tooltip returned after reopening")
    probe.results.rows = { { result = words("Reopened native row") } }
  elseif step == 6 then
    game.window.open("other")
  elseif step == 7 then
    game.window.open("probe")
    probe.status.text = words("Native window: other panel closed")
  end
end

return {
  windows = {
    probe = {
      size = { 440, 500 }, title = words("Native lifetime probe"),
      anchor = "top_left", offset = { 100, 120 },
      items = {
        { type = "text", name = "status", size = { 380, 50 }, text = words("Native window"), tooltip = words("Declared native tooltip") },
        { type = "button", name = "disabled", text = words("Disabled"), tooltip = words("Disabled button tooltip") },
        { type = "progress", name = "progress", value = 0.25, tooltip = words("Progress tooltip") },
        { type = "list", name = "results", size = { 380, 160 }, tooltip = words("List tooltip"), row = {
          size = { 360, 35 }, items = {
            { type = "text", name = "result", size = { 340, 30 }, text = words("Declared row"), tooltip = words("Declared row tooltip") },
          },
        } },
      },
    },
    other = {
      size = { 320, 220 }, title = words("Second native panel"),
      anchor = "top_left", offset = { 580, 120 },
      items = { { type = "text", name = "status", size = { 260, 35 }, text = words("The first panel closes") } },
    },
  },
  on_session = function(game, saved)
    sessions = (saved and saved.sessions or 0) + 1
    active = true
    step, frames, applied, waiting = 1, 0, false, false
    game.window.probe.disabled.enabled = false
    log("qa-windows session " .. sessions)
  end,
  on_session_ended = function()
    active = false
  end,
  on_frame = function(game)
    if not active or step > 7 or waiting then return end
    if not applied then
      apply(game)
      applied = true
      frames = 0
    end
    frames = frames + 1
    if frames < 12 then return end
    waiting = true
    local ready = step
    game.popup {
      title = words("QA windows step " .. ready),
      text = words("Native lifetime course, step " .. ready),
      buttons = { { text = words("Continue"), action = function()
        log("qa-windows completed " .. ready)
        step, applied, waiting = ready + 1, false, false
      end } },
    }
  end,
  on_window = function(_, name, open)
    log("qa-windows window " .. name .. " " .. tostring(open))
  end,
  save_data = function()
    return { sessions = sessions }
  end,
}
