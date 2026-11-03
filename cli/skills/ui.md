ui: interface fragments

An interface fragment is Paradox .gui text the framework splices into one of
the game's windows as the game reads it. A fragment goes in
eu4-harmony/ui/<window>/<name>.gui, where <window> is the host window's
name; the army panel, single_unitpanel, is the one host there is now.
Coordinates are relative to the host window. A fragment in
eu4-harmony/ui/<window>/buttons/ goes in that window's button area instead
(eu4-harmony skills ui buttons).

- Name every element you will reach from Lua. The framework prefixes the
  names it splices, so they never clash; Lua uses the names as written.
- Sprites go in a .gfx file of the mod's own, in the mod's interface/
  folder, as in any Paradox mod.
- Texts: a localisation key where the .gui takes text (eu4-harmony skills
  localisation).

From Lua, game.ui.<name> is the element: set visible and enabled to true or
false, text and tooltip to a string (nil is none). They are set, never
read: keep what you set in the mod's own state when you need it again.

  on_frame = function(game)
    game.ui.button.visible = game.army.selected() ~= nil
  end,

Done when every element has a name, every name used in Lua exists in a
fragment, and each element shows where it should in the game.
