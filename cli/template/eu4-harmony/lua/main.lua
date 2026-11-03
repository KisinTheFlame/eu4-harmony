-- Template Mod: an EU4 Harmony extension.
--
-- Its button sits in the army panel's button area. It shows while an own
-- land army is selected and logs each click to
-- Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\template-mod.log.
-- Its texts are in eu4-harmony/localisation/, one file per language.
-- An editor with the Lua language server completes game, its capabilities
-- and the handlers from .eu4-harmony/types/ (eu4-harmony types).
local clicks = 0

---@type eu4.Handlers
return {
  on_session = function(game)
    game.ui.button.tooltip = text.get("TEMPLATE_MOD_TOOLTIP")
  end,

  on_frame = function(game)
    game.ui.button.visible = game.army.selected() ~= nil
  end,

  on_button = function(game, name, click)
    clicks = clicks + 1
    local army = game.army.selected()
    log(name .. " clicked (right: " .. tostring(click.right) .. "), click #" .. clicks .. ", army " .. tostring(army))
  end,
}
